use std::{error::Error, fs};

use log::warn;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    /// filepath to database file
    pub db_file: String,
    /// maximum emails per minute
    pub limit: u64,
}

impl Default for Config {
    fn default() -> Self {
        // temporary
        Self {
            db_file: "db.sqlite".to_string(),
            limit: 5,
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
                warn!("could not find config file. using defaults.");
                Ok(Config::default())
            }
        }
    }
}
