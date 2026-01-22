//! Description configuration and validation.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{MAX_BIO_LENGTH_FREE, MAX_BIO_LENGTH_PREMIUM};

/// Errors that can occur during description validation.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Description at index {index} (id: {id}) exceeds maximum length: {length} > {max_length}")]
    TooLong {
        index: usize,
        id: String,
        length: usize,
        max_length: usize,
    },

    #[error("Description at index {index} (id: {id}) is empty")]
    Empty { index: usize, id: String },

    #[error("Duplicate description ID found: {id}")]
    DuplicateId { id: String },

    #[error("Description at index {index} (id: {id}) has invalid duration: {duration_secs} seconds (must be > 0)")]
    InvalidDuration {
        index: usize,
        id: String,
        duration_secs: u64,
    },

    #[error("No descriptions configured")]
    NoDescriptions,

    #[error("Failed to read configuration file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse configuration file: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// A single description entry with its display duration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Description {
    /// Unique identifier for this description.
    pub id: String,

    /// The bio text to display.
    pub text: String,

    /// How long to display this description in seconds.
    pub duration_secs: u64,
}

impl Description {
    /// Creates a new description entry.
    #[must_use]
    pub const fn new(id: String, text: String, duration_secs: u64) -> Self {
        Self {
            id,
            text,
            duration_secs,
        }
    }

    /// Returns the character count of the description text.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Checks if the description fits within the free user limit.
    #[must_use]
    pub fn fits_free_limit(&self) -> bool {
        self.char_count() <= MAX_BIO_LENGTH_FREE
    }

    /// Checks if the description fits within the premium user limit.
    #[must_use]
    pub fn fits_premium_limit(&self) -> bool {
        self.char_count() <= MAX_BIO_LENGTH_PREMIUM
    }
}

/// Configuration containing all descriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptionConfig {
    /// List of descriptions to rotate through.
    pub descriptions: Vec<Description>,

    /// Whether the user has Telegram Premium (affects max bio length).
    /// When `auto_detect_premium` is true, this value is updated at runtime.
    #[serde(default)]
    pub is_premium: bool,

    /// If true, automatically detect Premium status from Telegram.
    /// Defaults to true for new configs.
    #[serde(default = "default_auto_detect")]
    pub auto_detect_premium: bool,
}

fn default_auto_detect() -> bool {
    true
}

impl DescriptionConfig {
    /// Loads configuration from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ValidationError> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Saves configuration to a JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), ValidationError> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Validates all descriptions in the configuration.
    ///
    /// # Errors
    ///
    /// Returns the first validation error encountered.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.descriptions.is_empty() {
            return Err(ValidationError::NoDescriptions);
        }

        let max_length = if self.is_premium {
            MAX_BIO_LENGTH_PREMIUM
        } else {
            MAX_BIO_LENGTH_FREE
        };

        let mut seen_ids = std::collections::HashSet::new();

        for (index, desc) in self.descriptions.iter().enumerate() {
            // Check for duplicate IDs
            if !seen_ids.insert(&desc.id) {
                return Err(ValidationError::DuplicateId { id: desc.id.clone() });
            }

            // Check for empty text
            if desc.text.is_empty() {
                return Err(ValidationError::Empty {
                    index,
                    id: desc.id.clone(),
                });
            }

            // Check length
            let char_count = desc.char_count();
            if char_count > max_length {
                return Err(ValidationError::TooLong {
                    index,
                    id: desc.id.clone(),
                    length: char_count,
                    max_length,
                });
            }

            // Check duration
            if desc.duration_secs == 0 {
                return Err(ValidationError::InvalidDuration {
                    index,
                    id: desc.id.clone(),
                    duration_secs: desc.duration_secs,
                });
            }
        }

        Ok(())
    }

    /// Returns detailed validation results for all descriptions.
    #[must_use]
    pub fn validate_all(&self) -> Vec<Result<(), ValidationError>> {
        let max_length = if self.is_premium {
            MAX_BIO_LENGTH_PREMIUM
        } else {
            MAX_BIO_LENGTH_FREE
        };

        let mut results = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        if self.descriptions.is_empty() {
            results.push(Err(ValidationError::NoDescriptions));
            return results;
        }

        for (index, desc) in self.descriptions.iter().enumerate() {
            // Check for duplicate IDs
            if !seen_ids.insert(&desc.id) {
                results.push(Err(ValidationError::DuplicateId { id: desc.id.clone() }));
                continue;
            }

            // Check for empty text
            if desc.text.is_empty() {
                results.push(Err(ValidationError::Empty {
                    index,
                    id: desc.id.clone(),
                }));
                continue;
            }

            // Check length
            let char_count = desc.char_count();
            if char_count > max_length {
                results.push(Err(ValidationError::TooLong {
                    index,
                    id: desc.id.clone(),
                    length: char_count,
                    max_length,
                }));
                continue;
            }

            // Check duration
            if desc.duration_secs == 0 {
                results.push(Err(ValidationError::InvalidDuration {
                    index,
                    id: desc.id.clone(),
                    duration_secs: desc.duration_secs,
                }));
                continue;
            }

            results.push(Ok(()));
        }

        results
    }

    /// Gets a description by its index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Description> {
        self.descriptions.get(index)
    }

    /// Returns the number of descriptions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.descriptions.len()
    }

    /// Checks if there are no descriptions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.descriptions.is_empty()
    }

    /// Creates an example configuration for users to reference.
    #[must_use]
    pub fn example() -> Self {
        Self {
            descriptions: vec![
                Description::new(
                    "morning".to_owned(),
                    "☀️ Good morning! Ready for a new day".to_owned(),
                    3600, // 1 hour
                ),
                Description::new(
                    "working".to_owned(),
                    "💻 Currently working...".to_owned(),
                    7200, // 2 hours
                ),
                Description::new(
                    "evening".to_owned(),
                    "🌙 Relaxing in the evening".to_owned(),
                    3600, // 1 hour
                ),
            ],
            is_premium: false,
            auto_detect_premium: true,
        }
    }

    /// Updates the premium status (used after auto-detection).
    pub fn set_premium(&mut self, is_premium: bool) {
        self.is_premium = is_premium;
    }

    /// Returns the maximum bio length based on premium status.
    #[must_use]
    pub fn max_bio_length(&self) -> usize {
        if self.is_premium {
            MAX_BIO_LENGTH_PREMIUM
        } else {
            MAX_BIO_LENGTH_FREE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_description_char_count() {
        let desc = Description::new("test".to_owned(), "Hello, World!".to_owned(), 60);
        assert_eq!(desc.char_count(), 13);
    }

    #[test]
    fn test_description_char_count_unicode() {
        // Emoji characters should count as 1 character each
        let desc = Description::new("test".to_owned(), "Hello 👋🌍".to_owned(), 60);
        assert_eq!(desc.char_count(), 8); // "Hello " (6) + 2 emoji = 8
    }

    #[test]
    fn test_validation_empty_descriptions() {
        let config = DescriptionConfig {
            descriptions: vec![],
            is_premium: false,
            auto_detect_premium: false,
        };
        assert!(matches!(config.validate(), Err(ValidationError::NoDescriptions)));
    }

    #[test]
    fn test_validation_too_long() {
        let config = DescriptionConfig {
            descriptions: vec![Description::new(
                "test".to_owned(),
                "a".repeat(71),
                60,
            )],
            is_premium: false,
            auto_detect_premium: false,
        };
        assert!(matches!(config.validate(), Err(ValidationError::TooLong { .. })));
    }

    #[test]
    fn test_validation_premium_allows_longer() {
        let config = DescriptionConfig {
            descriptions: vec![Description::new(
                "test".to_owned(),
                "a".repeat(100),
                60,
            )],
            is_premium: true,
            auto_detect_premium: false,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_duplicate_id() {
        let config = DescriptionConfig {
            descriptions: vec![
                Description::new("same".to_owned(), "First".to_owned(), 60),
                Description::new("same".to_owned(), "Second".to_owned(), 60),
            ],
            is_premium: false,
            auto_detect_premium: false,
        };
        assert!(matches!(config.validate(), Err(ValidationError::DuplicateId { .. })));
    }

    #[test]
    fn test_validation_zero_duration() {
        let config = DescriptionConfig {
            descriptions: vec![Description::new(
                "test".to_owned(),
                "Hello".to_owned(),
                0,
            )],
            is_premium: false,
            auto_detect_premium: false,
        };
        assert!(matches!(config.validate(), Err(ValidationError::InvalidDuration { .. })));
    }
}
