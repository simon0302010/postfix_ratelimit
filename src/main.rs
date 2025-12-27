mod config;
mod limiter;

use crate::{config::Config, limiter::Limiter};

use std::{error::Error, path::PathBuf, process::exit, time::Duration};

use log::{debug, error, info};
use rusqlite::params;
use signal_hook::{
    consts::{SIGHUP, SIGUSR1, TERM_SIGNALS},
    iterator::Signals,
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_rusqlite::Connection;

const CONFIG_NAME: &str = "postfix_ratelimit.conf";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // --help commandline option
    let args = std::env::args().collect::<Vec<String>>();
    if args.contains(&"--help".to_string()) {
        show_help(args[0].clone());
        exit(0);
    }

    // channel for stop signal
    let (stop_send, stop_rec): (Sender<LimiterSignals>, Receiver<LimiterSignals>) =
        mpsc::channel(1);

    // spawns the thread
    spawn_signal_thread(stop_send)?;

    // load config
    let config_path = match find_config().await {
        Ok(path) => path,
        Err(()) => {
            eprintln!(
                "Error: Config file not found. Use --config=<path> to specify or see the documentation for possible locations.",
            );
            exit(1);
        }
    };

    let config = match Config::from_file(
        config_path
            .to_str()
            .expect("Failed to convert harcoded path to string. This should be impossible..."),
    ) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to parse configuration file:\n{}", e);
            exit(1);
        }
    };

    setup_logger(&config).await.unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
        exit(1);
    });

    // load db from disk into memory
    debug!("Starting to load database");
    let db_mem = load_db(PathBuf::from(&config.db_file))
        .await
        .unwrap_or_else(|e| {
            error!("Failed to load DB: {}", e);
            exit(1);
        });
    debug!("Loaded database into memory");

    db_mem
        .call(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS emails (
                address TEXT NOT NULL,
                host TEXT NOT NULL,
                count INTEGER DEFAULT 0,
                time INTEGER,
                UNIQUE(address, host)
            )",
                [],
            )
        })
        .await?;

    // spawn clean up thread
    if config.clean_interval > 0 {
        spawn_clean_thread(db_mem.clone(), config.clone()).await;
    }

    // start limiter and get the db connection back after it received the stop signal
    let limiter = Limiter::new(db_mem.clone(), config.clone());
    limiter.run(config.socket, stop_rec).await;

    // write db back to disk
    debug!("Starting to save database");
    save_db(&db_mem, PathBuf::from(&config.db_file))
        .await
        .unwrap_or_else(|e| {
            error!("Failed to save DB: {}", e);
            exit(1);
        });
    debug!("Successfully saved database to hard drive");

    Ok(())
}

/// spawns a thread that receives termination signals and sends them through a channel
fn spawn_signal_thread(sender: Sender<LimiterSignals>) -> Result<(), Box<dyn Error>> {
    let mut signals = Signals::new(
        TERM_SIGNALS
            .iter()
            .copied()
            .chain([SIGHUP, SIGUSR1])
            .collect::<Vec<_>>(),
    )?;

    std::thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received signal {:?}", sig);
            if sig == SIGHUP {
                if sender.blocking_send(LimiterSignals::RELOAD).is_err() {
                    break;
                }
            } else if sig == SIGUSR1 {
                if sender.blocking_send(LimiterSignals::CONFIG).is_err() {
                    break;
                }
            } else if TERM_SIGNALS.contains(&sig) {
                if sender.blocking_send(LimiterSignals::STOP).is_err() {
                    break;
                }
            }
        }
    });
    Ok(())
}

/// cleans the database on a set inverval
async fn spawn_clean_thread(conn: Connection, config: Config) {
    tokio::spawn(async move {
        loop {
            if conn
                .call(move |conn| {
                    conn.execute(
                        "DELETE FROM emails WHERE (unixepoch('now') - time) > ?1",
                        params![config.interval * 60],
                    )
                })
                .await
                .is_err()
            {
                error!("Failed to clean database");
                break;
            } else {
                debug!("Cleaned database")
            }
            // sleep
            tokio::time::sleep(Duration::from_mins(config.clean_interval)).await;
        }
    });
}

/// loads the database into ram
async fn load_db(disk_path: PathBuf) -> Result<Connection, Box<dyn Error>> {
    let db_mem = Connection::open_in_memory().await?;

    db_mem
        .call(move |conn_mem| {
            let conn_disk = rusqlite::Connection::open(&disk_path)?;
            let backup = rusqlite::backup::Backup::new(&conn_disk, conn_mem)?;
            backup.run_to_completion(1_000, Duration::from_millis(0), None)
        })
        .await
        .unwrap_or_else(|e| {
            error!("Failed to load database: {}", e);
            exit(1);
        });

    Ok(db_mem)
}

/// saves the database onto the hard drive
async fn save_db(db_mem: &Connection, disk_path: PathBuf) -> Result<(), Box<dyn Error>> {
    db_mem
        .call(move |conn_mem| {
            let mut conn_disk = rusqlite::Connection::open(&disk_path)?;
            let backup = rusqlite::backup::Backup::new(conn_mem, &mut conn_disk)?;
            backup.run_to_completion(1_000, Duration::from_millis(0), None)
        })
        .await
        .unwrap_or_else(|e| {
            error!("Failed to save database to disk: {}", e);
            exit(1);
        });
    Ok(())
}

async fn setup_logger(config: &Config) -> Result<(), log::SetLoggerError> {
    let log_level = if config.debug
        || std::env::args()
            .collect::<Vec<String>>()
            .contains(&"--debug".to_string())
    {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let mut logger = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                message
            ))
        })
        .level(log_level)
        .chain(std::io::stdout());

    if !config.log_file.trim().is_empty() {
        match fern::log_file(config.log_file.clone()) {
            Ok(file) => {
                logger = logger.chain(file);
            }
            Err(e) => {
                eprintln!(
                    "Failed to create log file: ({}). Only logging to console.",
                    e
                );
            }
        }
    } else {
        println!("Not logging to file")
    }

    logger.apply()
}

/// finds the configuration file
async fn find_config() -> Result<PathBuf, ()> {
    for argument in std::env::args().collect::<Vec<String>>() {
        if argument.starts_with("--config=") {
            return Ok(
                PathBuf::try_from(argument.trim_start_matches("--config=").trim()).unwrap_or_else(
                    |e| {
                        eprintln!("Failed to parse config path: {}", e);
                        exit(1);
                    },
                ),
            );
        }
    }

    let paths = vec![
        format!("/etc/{}", CONFIG_NAME),
        format!("/usr/local/etc/{}", CONFIG_NAME),
    ];

    for config_path in paths {
        match PathBuf::try_from(config_path) {
            Ok(path) if path.exists() => {
                return Ok(path);
            }
            _ => {}
        }
    }

    Err(())
}

fn show_help(executable: String) {
    println!("postfix_ratelimit  Copyright (C) 2025  simon0302010");
    println!("This program comes with ABSOLUTELY NO WARRANTY.");
    println!("This is free software, and you are welcome to redistribute it");
    println!("under certain conditions.");
    println!();
    println!("Usage: {} [OPTIONS]", executable);
    println!();
    println!("Options:");
    println!("  --config=<path>   Specify the path to the configuration file.");
    println!("  --debug           Enable debug logging.");
    println!("  --help            Show this help message and exit.");
    exit(0);
}

pub enum LimiterSignals {
    RELOAD,
    STOP,
    CONFIG,
}
