use std::{error::Error, fs};

use log::warn;
use serde::Deserialize;
use signal_hook::low_level::exit;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Path to the SQLite database file used for storing rate limit data.
    pub db_file: String,
    /// Time window for rate limiting, specified in minutes.
    #[serde(default)]
    pub interval: u64,
    /// Maximum number of emails allowed to be sent within each interval.
    #[serde(default)]
    pub limit: u64,
    /// Address (IP:PORT) on which the milter will listen, or a Unix socket path (must start with '/').
    #[serde(default)]
    pub socket: String,
    /// Maximum number of recipients allowed per individual email message. 0 for no limit.
    #[serde(default)]
    pub max_recipients: u64,
    /// If true, each recipient counts separately towards the rate limit, causing the limit to be reached faster with emails sent to multiple recipients.
    #[serde(default)]
    pub count_recipients: bool,
    /// If true, rate limiting is tracked separately per sender and per connecting host; if false, only the sender's email address is considered.
    #[serde(default)]
    pub per_host: bool,
    /// Frequency, in minutes, at which expired entries are removed from the database. Does not affect ratelimiting.
    #[serde(default)]
    pub clean_interval: u64,
    /// Enables Debug mode which prints extra messages to the terminal
    #[serde(default)]
    pub debug: bool,
    /// Rejects Emails that encountered some kind of issue during processing. False by default.
    #[serde(default)]
    pub reject_error: bool,
    /// In which file to write the logs. Leave empty for no logging to file.
    pub log_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_file: "postfix_ratelimit.sqlite".to_string(),
            interval: 60, // 1h
            limit: 20,
            socket: "127.0.0.1:11847".to_string(),
            max_recipients: 20,
            count_recipients: true,
            per_host: false,
            clean_interval: 120,
            debug: false,
            reject_error: false,
            log_file: "postfix_ratelimit.log".to_string(),
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        match fs::read_to_string(path) {
            Ok(s) => {
                let cfg: Config = toml::from_str(&s)?;
                Ok(cfg)
            }
            Err(_) => {
                eprintln!(
                    "Error: Config file not found at '{}'. Use --config=<path> to specify.",
                    path
                );
                exit(1);
            }
        }
    }
}
