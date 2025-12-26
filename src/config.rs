use std::{error::Error, fs, process::exit};

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
    /// Address on which the milter will listen, specified as either "inet:IP:PORT" for a TCP socket or "unix:/path/to/socket" for a Unix socket.
    pub socket: String,
    /// Maximum number of recipients allowed per individual email message. 0 for no limit.
    pub max_recipients: u64,
    /// If true, each recipient counts separately towards the rate limit, causing the limit to be reached faster with emails sent to multiple recipients.
    pub count_recipients: bool,
    /// If true, rate limiting is tracked separately per sender and per connecting host; if false, only the sender's email address is considered.
    pub per_host: bool,
    /// Frequency, in minutes, at which expired entries are removed from the database. Does not affect ratelimiting.
    pub clean_interval: u64,
    /// Enables Debug mode which prints extra messages to the terminal
    pub debug: bool,
    /// Rejects Emails that encountered some kind of issue during processing. False by default.
    pub reject_error: bool,
    /// In which file to write the logs. Leave empty for no logging to file.
    pub log_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_file: String::new(),
            interval: 60, // 1h
            limit: 20,
            socket: "inet:127.0.0.1:11847".to_string(),
            max_recipients: 20,
            count_recipients: true,
            per_host: false,
            clean_interval: 120,
            debug: false,
            reject_error: false,
            log_file: String::new(),
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        match fs::read_to_string(path) {
            Ok(s) => {
                let cfg: Config = toml::from_str(&s)?;
                cfg.validate()?;
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

    fn validate(&self) -> Result<(), String> {
        let mut failed = false;
        let mut errors: Vec<&str> = Vec::new();

        if self.db_file.is_empty() {
            errors.push("Required field \"db_file\" is missing from config or empty");
            failed = true;
        }

        if failed {
            Err(errors.join("\n"))
        } else {
            Ok(())
        }
    }
}
