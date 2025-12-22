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
}

impl Default for Config {
    fn default() -> Self {
        // temporary
        Self {
            db_file: "db.sqlite".to_string(),
            interval: 1440, // 24h
            limit: 500,
            socket: "inet:3000@localhost".to_string(),
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
