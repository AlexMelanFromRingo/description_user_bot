//! Telegram client wrapper for profile management.

use std::sync::Arc;
use std::time::Duration;

use grammers_client::client::{LoginToken, PasswordToken};
use grammers_client::{sender, Client, InvocationError, SenderPool, SignInError};
use grammers_session::storages::SqliteSession;
use grammers_tl_types as tl;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use super::RateLimiter;
use crate::config::TelegramConfig;

/// Re-export types for external use.
pub use grammers_client::client::{LoginToken as Token, PasswordToken as PwdToken};

/// Errors that can occur during Telegram operations.
#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("Not authorized. Please sign in first.")]
    NotAuthorized,

    #[error("Sign in failed: {0}")]
    SignInFailed(String),

    #[error("Password required for 2FA")]
    PasswordRequired(PasswordToken),

    #[error("Invalid password")]
    InvalidPassword(PasswordToken),

    #[error("Failed to update profile: {0}")]
    ProfileUpdateFailed(String),

    #[error("Flood wait required: {0} seconds")]
    FloodWait(u32),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("API invocation error: {0}")]
    Invocation(String),
}

impl From<InvocationError> for TelegramError {
    fn from(err: InvocationError) -> Self {
        let err_str = err.to_string();

        // Check for flood wait errors
        if (err_str.contains("FLOOD_WAIT") || err_str.contains("flood"))
            && let Some(seconds) = extract_flood_wait_seconds(&err_str) {
                return Self::FloodWait(seconds);
            }

        Self::Invocation(err_str)
    }
}

/// Extracts flood wait seconds from an error message.
fn extract_flood_wait_seconds(err_msg: &str) -> Option<u32> {
    let patterns = ["FLOOD_WAIT_", "flood wait "];

    for pattern in patterns {
        if let Some(idx) = err_msg.to_lowercase().find(&pattern.to_lowercase()) {
            let start = idx + pattern.len();
            let num_str: String = err_msg[start..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(seconds) = num_str.parse() {
                return Some(seconds);
            }
        }
    }
    None
}

/// Result of QR code authentication attempt.
#[derive(Debug, Clone)]
pub enum QrAuthResult {
    /// Got a token to display as QR code.
    Token {
        /// Raw token bytes (encode as base64 for URL).
        token: Vec<u8>,
        /// Unix timestamp when the token expires.
        expires: i32,
    },
    /// Need to migrate to another DC.
    MigrateTo {
        /// Target datacenter ID.
        dc_id: i32,
    },
    /// Authentication successful.
    Success {
        /// User ID of the authenticated user.
        user_id: i64,
        /// Username if available.
        username: Option<String>,
    },
    /// 2FA password is required.
    PasswordRequired,
}

/// State of the current profile description.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ProfileState {
    /// Current bio text.
    pub current_bio: Option<String>,

    /// Index of current description in rotation.
    pub current_index: usize,

    /// Whether the current description was skipped.
    pub is_skipped: bool,
}


/// High-level Telegram client wrapper.
pub struct TelegramBot {
    /// The underlying grammers client.
    client: Client,

    /// Handle to the sender pool for disconnection.
    handle: sender::SenderPoolHandle,

    /// Rate limiter for API calls.
    rate_limiter: RateLimiter,

    /// Current profile state.
    state: RwLock<ProfileState>,

    /// Background task running the sender pool.
    _pool_task: JoinHandle<()>,
}

impl TelegramBot {
    /// Connects to Telegram with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if connection fails.
    pub async fn connect(
        config: &TelegramConfig,
        rate_limit_secs: u64,
    ) -> Result<Self, TelegramError> {
        info!("Connecting to Telegram...");

        let session = Arc::new(
            SqliteSession::open(&config.session_path)
                .await
                .map_err(|e| TelegramError::Session(e.to_string()))?,
        );

        let SenderPool {
            runner,
            updates: _updates,
            handle,
        } = SenderPool::new(Arc::clone(&session), config.api_id);

        let client = Client::new(handle.clone());

        // Spawn the sender pool runner
        let pool_task = tokio::spawn(async move {
            runner.run().await;
        });

        let is_authorized = client
            .is_authorized()
            .await
            .map_err(|e| TelegramError::Connection(e.to_string()))?;

        info!("Connected to Telegram. Authorized: {}", is_authorized);

        Ok(Self {
            client,
            handle: handle.thin,
            rate_limiter: RateLimiter::from_secs(rate_limit_secs),
            state: RwLock::new(ProfileState::default()),
            _pool_task: pool_task,
        })
    }

    /// Checks if the client is authorized.
    ///
    /// # Errors
    ///
    /// Returns an error if the check fails.
    pub async fn is_authorized(&self) -> Result<bool, TelegramError> {
        self.client
            .is_authorized()
            .await
            .map_err(|e| TelegramError::Connection(e.to_string()))
    }

    /// Requests a login code to be sent to the phone number.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn request_login_code(
        &self,
        phone: &str,
        api_hash: &str,
    ) -> Result<LoginToken, TelegramError> {
        info!("Requesting login code for phone: {}...", mask_phone(phone));

        self.client
            .request_login_code(phone, api_hash)
            .await
            .map_err(|e| TelegramError::SignInFailed(e.to_string()))
    }

    /// Signs in with the login code.
    ///
    /// # Errors
    ///
    /// Returns an error if sign in fails.
    pub async fn sign_in(&self, token: &LoginToken, code: &str) -> Result<(), TelegramError> {
        info!("Signing in with login code...");

        match self.client.sign_in(token, code).await {
            Ok(_user) => {
                info!("Successfully signed in!");
                Ok(())
            }
            Err(SignInError::PasswordRequired(password_token)) => {
                debug!(
                    "2FA password required, hint: {:?}",
                    password_token.hint()
                );
                Err(TelegramError::PasswordRequired(password_token))
            }
            Err(SignInError::InvalidCode) => {
                Err(TelegramError::SignInFailed("Invalid code".to_owned()))
            }
            Err(e) => Err(TelegramError::SignInFailed(e.to_string())),
        }
    }

    /// Checks the 2FA password.
    ///
    /// # Errors
    ///
    /// Returns an error if the password is invalid.
    pub async fn check_password(
        &self,
        password_token: PasswordToken,
        password: &str,
    ) -> Result<(), TelegramError> {
        info!("Checking 2FA password...");

        match self.client.check_password(password_token, password).await {
            Ok(_user) => {
                info!("Successfully authenticated with 2FA!");
                Ok(())
            }
            Err(SignInError::InvalidPassword(token)) => Err(TelegramError::InvalidPassword(token)),
            Err(e) => Err(TelegramError::SignInFailed(e.to_string())),
        }
    }

    /// Performs QR code authentication.
    ///
    /// Returns the login token bytes that should be displayed as a QR code.
    /// The QR code URL format is: `tg://login?token=BASE64_TOKEN`
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn export_login_token(
        &self,
        api_id: i32,
        api_hash: &str,
    ) -> Result<QrAuthResult, TelegramError> {
        debug!("Requesting QR login token...");

        let request = tl::functions::auth::ExportLoginToken {
            api_id,
            api_hash: api_hash.to_owned(),
            except_ids: vec![],
        };

        match self.client.invoke(&request).await {
            Ok(tl::enums::auth::LoginToken::Token(token)) => {
                debug!("Got login token, expires: {}", token.expires);
                Ok(QrAuthResult::Token {
                    token: token.token,
                    expires: token.expires,
                })
            }
            Ok(tl::enums::auth::LoginToken::MigrateTo(migrate)) => {
                debug!("Need to migrate to DC {}", migrate.dc_id);
                Ok(QrAuthResult::MigrateTo { dc_id: migrate.dc_id })
            }
            Ok(tl::enums::auth::LoginToken::Success(success)) => {
                debug!("QR login successful!");
                if let tl::enums::auth::Authorization::Authorization(auth) = success.authorization
                    && let tl::enums::User::User(user) = auth.user {
                        return Ok(QrAuthResult::Success {
                            user_id: user.id,
                            username: user.username,
                        });
                    }
                Ok(QrAuthResult::Success {
                    user_id: 0,
                    username: None,
                })
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("SESSION_PASSWORD_NEEDED") {
                    return Ok(QrAuthResult::PasswordRequired);
                }
                Err(TelegramError::SignInFailed(err_str))
            }
        }
    }

    /// Accepts a login token (called when QR code is scanned).
    ///
    /// # Errors
    ///
    /// Returns an error if the token is invalid or expired.
    pub async fn accept_login_token(&self, token: Vec<u8>) -> Result<(), TelegramError> {
        debug!("Accepting login token...");

        let request = tl::functions::auth::AcceptLoginToken { token };

        self.client
            .invoke(&request)
            .await
            .map(|_| ())
            .map_err(|e| TelegramError::SignInFailed(e.to_string()))
    }

    /// Updates the user's profile bio/about text.
    ///
    /// # Errors
    ///
    /// Returns an error if the update fails or if rate limited.
    pub async fn update_bio(&self, bio: &str) -> Result<(), TelegramError> {
        if !self.is_authorized().await? {
            return Err(TelegramError::NotAuthorized);
        }

        debug!("Waiting for rate limiter...");
        let waited = self.rate_limiter.wait_and_acquire().await;
        if !waited.is_zero() {
            debug!("Waited {:?} for rate limit", waited);
        }

        info!("Updating bio to: \"{}\"", truncate_for_log(bio, 30));

        let request = tl::functions::account::UpdateProfile {
            first_name: None,
            last_name: None,
            about: Some(bio.to_owned()),
        };

        match self.client.invoke(&request).await {
            Ok(_user) => {
                let mut state = self.state.write().await;
                state.current_bio = Some(bio.to_owned());
                state.is_skipped = false;
                info!("Bio updated successfully");
                Ok(())
            }
            Err(e) => {
                let err: TelegramError = e.into();
                if let TelegramError::FloodWait(seconds) = &err {
                    warn!("Flood wait triggered: {} seconds", seconds);
                    self.rate_limiter.handle_flood_wait(*seconds).await;
                }
                Err(err)
            }
        }
    }

    /// Gets the current profile state.
    pub async fn get_state(&self) -> ProfileState {
        self.state.read().await.clone()
    }

    /// Sets the current description index.
    pub async fn set_current_index(&self, index: usize) {
        let mut state = self.state.write().await;
        state.current_index = index;
    }

    /// Marks the current description as skipped.
    pub async fn mark_skipped(&self) {
        let mut state = self.state.write().await;
        state.is_skipped = true;
    }

    /// Gets the time remaining until the next API call is allowed.
    pub async fn time_until_allowed(&self) -> Duration {
        self.rate_limiter.time_until_allowed().await
    }

    /// Returns a reference to the underlying client for advanced operations.
    #[must_use]
    pub fn inner(&self) -> &Client {
        &self.client
    }

    /// Checks if the current user has Telegram Premium.
    ///
    /// # Errors
    ///
    /// Returns an error if not authorized or API call fails.
    pub async fn is_premium(&self) -> Result<bool, TelegramError> {
        if !self.is_authorized().await? {
            return Err(TelegramError::NotAuthorized);
        }

        debug!("Checking premium status...");

        let request = tl::functions::users::GetUsers {
            id: vec![tl::enums::InputUser::UserSelf],
        };

        match self.client.invoke(&request).await {
            Ok(users) => {
                if let Some(tl::enums::User::User(user)) = users.first() {
                    let is_premium = user.premium;
                    info!("Premium status: {}", is_premium);
                    Ok(is_premium)
                } else {
                    warn!("Could not get user info, assuming non-premium");
                    Ok(false)
                }
            }
            Err(e) => {
                warn!("Failed to check premium status: {}", e);
                Err(e.into())
            }
        }
    }

    /// Disconnects from Telegram.
    pub fn disconnect(&self) {
        info!("Disconnecting from Telegram...");
        self.handle.quit();
    }
}

impl std::fmt::Debug for TelegramBot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramBot")
            .field("rate_limiter", &self.rate_limiter)
            .finish_non_exhaustive()
    }
}

/// Masks a phone number for logging (shows last 4 digits).
fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    if digits.len() > 4 {
        format!("***{}", &digits[digits.len() - 4..])
    } else {
        "****".to_owned()
    }
}

/// Truncates a string for logging purposes.
fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_owned()
    } else {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_phone() {
        assert_eq!(mask_phone("+1234567890"), "***7890");
        assert_eq!(mask_phone("123"), "****");
        assert_eq!(mask_phone("+7 (999) 123-45-67"), "***4567");
    }

    #[test]
    fn test_truncate_for_log() {
        assert_eq!(truncate_for_log("Hello", 10), "Hello");
        assert_eq!(truncate_for_log("Hello, World!", 5), "Hello...");
    }

    #[test]
    fn test_extract_flood_wait() {
        assert_eq!(extract_flood_wait_seconds("FLOOD_WAIT_120"), Some(120));
        assert_eq!(extract_flood_wait_seconds("flood wait 60 seconds"), Some(60));
        assert_eq!(extract_flood_wait_seconds("some other error"), None);
    }
}
