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
    /// Rejects Emails that encountered some kind of issue during processing like the sender missing. False by default.
    pub reject_error: bool,
    /// In which file to write the logs. Leave empty for no logging to file.
    pub log_file: String,
    /// Enables rate limiting based on the SASL user. This requires the server to provide the {auth_authen} macro.
    pub use_sasl: bool,
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
            use_sasl: false,
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
                    "[ERROR] Config file not found at '{}'. Use --config=<path> to specify.",
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

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut to_write: Vec<String> = Vec::new();
        if !self.db_file.is_empty() {
            to_write.push(format!("Database: {}", self.db_file));
        }
        to_write.push(format!("Interval: {}m", self.interval));
        to_write.push(format!("Limit: {}", self.limit));
        to_write.push(format!("Socket: {}", self.socket));
        to_write.push(format!("Max recipients: {}", self.max_recipients));
        to_write.push(format!(
            "Count recipients: {}",
            to_yes_or_no(self.count_recipients)
        ));
        to_write.push(format!("Per host: {}", to_yes_or_no(self.per_host)));
        to_write.push(format!("Clean interval: {}m", self.clean_interval));
        to_write.push(format!("Debug mode: {}", to_yes_or_no(self.debug)));
        to_write.push(format!(
            "Reject errors: {}",
            to_yes_or_no(self.reject_error)
        ));
        to_write.push(format!("Log file: {}", {
            if self.log_file == String::new() {
                "No".to_string()
            } else {
                self.log_file.clone()
            }
        }));

        write!(f, "{}", to_write.join("\n"))
    }
}

fn to_yes_or_no(boolean: bool) -> String {
    if boolean {
        "Yes".to_string()
    } else {
        "No".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_valid() {
        let toml_str = r#"
           db_file = "/var/db/ratelimit.db"
           interval = 20
           limit = 10
           socket = "inet:127.0.0.1:11847"
        "#;

        let config: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.db_file, "/var/db/ratelimit.db".to_string());
        assert_eq!(config.interval, 20);
        assert_eq!(config.limit, 10);
        assert_eq!(config.socket, "inet:127.0.0.1:11847".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_db_missing() {
        let toml_str = r#"
            interval = 20
            limit = 10
        "#;

        let config: Config = toml::from_str(&toml_str).unwrap();

        assert!(config.validate().is_err());
    }
}
