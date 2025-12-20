mod config;
use crate::config::Config;

use std::{
    error::Error,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use log::{error, info};
use signal_hook::{consts::TERM_SIGNALS, iterator::Signals};
use simple_logger::SimpleLogger;

const CONFIG_PATH: &str = "config.toml";

fn main() -> Result<(), Box<dyn Error>> {
    // init logger
    if SimpleLogger::new().init().is_err() {
        eprintln!("failed to initialize logger.")
    }

    // channel for stop signal
    let (stop_send, stop_rec): (Sender<()>, Receiver<()>) = mpsc::channel();

    // spawns the thread
    spawn_signal_thread(stop_send)?;

    // load config
    let config = Config::from_file(CONFIG_PATH)?;

    // waits for a message from the thread
    stop_rec.recv().expect("failed to receive stop signal");

    Ok(())
}

/// spawns a thread that receives termination signals and sends them through a channel
fn spawn_signal_thread(sender: Sender<()>) -> Result<(), Box<dyn Error>> {
    let mut signals = Signals::new(TERM_SIGNALS)?;

    thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received signal {:?}", sig);
            if sender.send(()).is_err() {
                error!("failed to send stop signal");
                break;
            }
        }
    });

    Ok(())
}
