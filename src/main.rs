mod config;
mod limiter;

use crate::{config::Config, limiter::Limiter};

use std::{error::Error, path::PathBuf, process::exit, time::Duration};

use log::{error, info};
use signal_hook::{
    consts::{SIGHUP, TERM_SIGNALS},
    iterator::Signals,
};
use simple_logger::SimpleLogger;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_rusqlite::Connection;

const CONFIG_PATH: &str = "config.toml";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // init logger
    SimpleLogger::new().init().unwrap_or_else(|_| {
        eprintln!("Failed to initialize logger");
    });

    // channel for stop signal
    let (stop_send, stop_rec): (Sender<LimiterSignals>, Receiver<LimiterSignals>) =
        mpsc::channel(1);

    // spawns the thread
    spawn_signal_thread(stop_send)?;

    // load config
    let config = match Config::from_file(CONFIG_PATH) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to parse configuration file\n{}", e);
            exit(1);
        }
    };

    // load db from disk into memory
    let db_mem = load_db(PathBuf::from(&config.db_file))
        .await
        .unwrap_or_else(|e| {
            error!("Failed to load DB: {}", e);
            exit(1);
        });

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

    // start limiter and get the db connection back after it received the stop signal
    let limiter = Limiter::new(db_mem.clone(), config.clone());
    limiter.run(config.socket, stop_rec).await;

    // write db back to disk
    save_db(&db_mem, PathBuf::from(&config.db_file))
        .await
        .unwrap_or_else(|e| {
            error!("Failed to save DB: {}", e);
            exit(1);
        });

    Ok(())
}

/// spawns a thread that receives termination signals and sends them through a channel
fn spawn_signal_thread(sender: Sender<LimiterSignals>) -> Result<(), Box<dyn Error>> {
    let mut signals = Signals::new(
        TERM_SIGNALS
            .iter()
            .copied()
            .chain([SIGHUP])
            .collect::<Vec<_>>(),
    )?;
    std::thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received signal {:?}", sig);
            if sig == SIGHUP {
                if sender.blocking_send(LimiterSignals::RELOAD).is_err() {
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

async fn load_db(disk_path: PathBuf) -> Result<Connection, Box<dyn Error>> {
    let db_mem = Connection::open_in_memory().await?;

    db_mem
        .call(move |conn_mem| {
            let conn_disk = rusqlite::Connection::open(&disk_path)?;
            let backup = rusqlite::backup::Backup::new(&conn_disk, conn_mem)?;
            backup.run_to_completion(5, Duration::from_millis(250), None)
        })
        .await
        .unwrap_or_else(|e| {
            error!("Failed to load database: {}", e);
            exit(1);
        });

    Ok(db_mem)
}

async fn save_db(db_mem: &Connection, disk_path: PathBuf) -> Result<(), Box<dyn Error>> {
    db_mem
        .call(move |conn_mem| {
            let mut conn_disk = rusqlite::Connection::open(&disk_path)?;
            let backup = rusqlite::backup::Backup::new(conn_mem, &mut conn_disk)?;
            backup.run_to_completion(5, Duration::from_millis(250), None)
        })
        .await
        .unwrap_or_else(|e| {
            error!("Failed to save database to disk: {}", e);
            exit(1);
        });
    Ok(())
}

pub enum LimiterSignals {
    RELOAD,
    STOP,
}
