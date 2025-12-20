mod config;
mod limiter;

use crate::{config::Config, limiter::Limiter};

use std::{
    error::Error,
    process::exit,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use log::{error, info};
use rusqlite::Connection;
use signal_hook::{consts::TERM_SIGNALS, iterator::Signals};
use simple_logger::SimpleLogger;

const CONFIG_PATH: &str = "config.toml";

fn main() -> Result<(), Box<dyn Error>> {
    // init logger
    SimpleLogger::new().init().unwrap_or_else(|_| {
        eprintln!("Failed to initialize logger");
    });

    // channel for stop signal
    let (stop_send, stop_rec): (Sender<()>, Receiver<()>) = mpsc::channel();

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
    let mut db_disk = Connection::open(&config.db_file).unwrap_or_else(|e| {
        error!("Failed to create or open database: {}", e);
        exit(1);
    });
    let mut db_mem = Connection::open_in_memory().unwrap_or_else(|e| {
        error!("Failed to create in-memory database: {}", e);
        exit(1);
    });

    // creates backup
    backup_db(&db_disk, &mut db_mem).unwrap_or_else(|e| {
        error!("Failed to load database into memory: {}", e);
        exit(1);
    });

    // start limiter and get the db connection back after it received the stop signal
    let limiter = Limiter::new(db_mem, stop_rec);
    let db_mem = limiter.run();

    // write db back to disk
    backup_db(&db_mem, &mut db_disk).unwrap_or_else(|e| {
        error!("Failed to write database to disk: {}", e);
        exit(1);
    });

    Ok(())
}

/// spawns a thread that receives termination signals and sends them through a channel
fn spawn_signal_thread(sender: Sender<()>) -> Result<(), Box<dyn Error>> {
    let mut signals = Signals::new(TERM_SIGNALS)?;

    thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received signal {:?}", sig);
            if sender.send(()).is_err() {
                error!("Failed to send stop signal");
                break;
            }
        }
    });

    Ok(())
}

/// backs-up a database
fn backup_db(from: &Connection, to: &mut Connection) -> Result<(), rusqlite::Error> {
    rusqlite::backup::Backup::new(from, to)
        .unwrap_or_else(|e| {
            error!("Failed to create backup for database: {}", e);
            exit(1);
        })
        .run_to_completion(5, Duration::from_millis(250), None)
}
