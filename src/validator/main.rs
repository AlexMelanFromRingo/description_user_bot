//! Standalone validator for description configuration files.
//!
//! This tool validates JSON configuration files for the description bot,
//! checking for proper structure, valid lengths, and other requirements.

use std::process::ExitCode;

use clap::Parser;

// Import from the main crate
use description_user_bot::config::{
    DescriptionConfig, MAX_BIO_LENGTH_FREE, MAX_BIO_LENGTH_PREMIUM,
};

/// Description configuration validator.
#[derive(Parser, Debug)]
#[command(name = "validate_descriptions")]
#[command(about = "Validates description configuration files for the Telegram userbot")]
#[command(version)]
struct Args {
    /// Path to the JSON configuration file to validate.
    #[arg(short, long, default_value = "descriptions.json")]
    file: String,

    /// Treat as Telegram Premium account (allows 140 chars instead of 70).
    #[arg(short, long)]
    premium: bool,

    /// Generate an example configuration file at the specified path.
    #[arg(long)]
    generate_example: Option<String>,

    /// Show detailed information for each description.
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // Handle example generation
    if let Some(output_path) = args.generate_example {
        return generate_example(&output_path);
    }

    // Validate the configuration file
    validate_config(&args.file, args.premium, args.verbose)
}

fn generate_example(output_path: &str) -> ExitCode {
    let example = DescriptionConfig::example();

    match example.save_to_file(output_path) {
        Ok(()) => {
            println!("✓ Example configuration written to: {output_path}");
            println!("\nThe file contains {} example descriptions.", example.len());
            println!("Edit this file and set 'is_premium' to true if you have Telegram Premium.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("✗ Failed to write example file: {e}");
            ExitCode::FAILURE
        }
    }
}

fn validate_config(path: &str, premium: bool, verbose: bool) -> ExitCode {
    println!("Validating: {path}");
    println!("Account type: {}\n", if premium { "Premium" } else { "Free" });

    // Load the configuration
    let mut config = match DescriptionConfig::load_from_file(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ Failed to load configuration: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Override premium setting from CLI
    config.is_premium = premium;

    let max_length = if premium {
        MAX_BIO_LENGTH_PREMIUM
    } else {
        MAX_BIO_LENGTH_FREE
    };

    // Validate all descriptions
    let results = config.validate_all();

    let mut errors = 0;
    let mut warnings = 0;

    for (i, result) in results.iter().enumerate() {
        let desc = &config.descriptions[i];
        let char_count = desc.char_count();

        if verbose {
            println!(
                "[{}] \"{}\" ({} chars, {}s)",
                desc.id,
                truncate(&desc.text, 40),
                char_count,
                desc.duration_secs
            );
        }

        match result {
            Ok(()) => {
                // Check for warnings (close to limit)
                let warn_threshold = max_length * 90 / 100; // 90% of max
                if char_count > warn_threshold {
                    warnings += 1;
                    if verbose {
                        println!(
                            "  ⚠ Warning: {char_count} chars is close to the {max_length} char limit"
                        );
                    }
                } else if verbose {
                    println!("  ✓ OK");
                }
            }
            Err(e) => {
                errors += 1;
                println!("  ✗ Error: {e}");
            }
        }
    }

    println!();

    // Summary
    let total = config.len();
    let valid = total - errors;

    if errors == 0 {
        println!("✓ All {total} descriptions are valid!");

        if warnings > 0 {
            println!("  ({warnings} warning(s) - descriptions close to character limit)");
        }

        // Show character limit info
        println!("\nCharacter limits:");
        println!("  Free account:    {MAX_BIO_LENGTH_FREE} chars");
        println!("  Premium account: {MAX_BIO_LENGTH_PREMIUM} chars");
        println!("  Your setting:    {max_length} chars ({})", if premium { "Premium" } else { "Free" });

        ExitCode::SUCCESS
    } else {
        println!("✗ Validation failed: {errors} error(s) in {total} descriptions");
        println!("  Valid: {valid}/{total}");

        ExitCode::FAILURE
    }
}

/// Truncates a string for display.
fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_owned()
    } else {
        format!("{}...", chars[..max_len].iter().collect::<String>())
    }
}
