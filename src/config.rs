use std::{error::Error, fs};

use log::warn;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    /// filepath to database file
    pub db_file: String,
    /// interval in minutes
    pub interval: u64,
    /// how many emails are allowed per interval
    pub limit: u64,
    /// socket on which to run the milter
    pub socket: String,
    /// maximum amount of recipients allowed per email
    pub max_recipients: u64,
}

impl Default for Config {
    fn default() -> Self {
        // temporary
        Self {
            db_file: "db.sqlite".to_string(),
            interval: 1440, // 24h
            limit: 500,
            socket: "127.0.0.1:3000".to_string(),
            max_recipients: 50,
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
