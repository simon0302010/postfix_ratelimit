use std::{error::Error, fs};

use log::warn;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// Path to the SQLite database file used for storing rate limit data.
    pub db_file: String,
    /// Time window for rate limiting, specified in minutes.
    pub interval: u64,
    /// Maximum number of emails allowed to be sent within each interval.
    pub limit: u64,
    /// Address (IP:PORT) on which the milter will listen, or a Unix socket path (must start with '/').
    pub socket: String,
    /// Maximum number of recipients allowed per individual email message. 0 for no limit.
    pub max_recipients: u64,
    /// If true, each recipient counts separately towards the rate limit, causing the limit to be reached faster with emails sent to multiple recipients.
    pub count_recipients: bool,
    /// If true, rate limiting is tracked separately per sender and per connecting host; if false, only the sender's email address is considered.
    pub per_host: bool,
    /// Frequency, in minutes, at which expired entries are removed from the database. Does not affect ratelimiting.
    pub clean_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        // temporary
        Self {
            db_file: "ratelimit.sqlite".to_string(),
            interval: 60, // 1h
            limit: 20,
            socket: "127.0.0.1:11847".to_string(),
            max_recipients: 20,
            count_recipients: true,
            per_host: false,
            clean_interval: 120,
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
                warn!("Could not find configuration file. Using default values.");
                Ok(Config::default())
            }
        }
    }
}
