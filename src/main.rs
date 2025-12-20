use signal_hook::{consts::TERM_SIGNALS, iterator::Signals};
use std::{error::Error, sync::mpsc, thread};

fn main() -> Result<(), Box<dyn Error>> {
    let (stop_send, stop_rec) = mpsc::channel();

    let mut signals = Signals::new(TERM_SIGNALS)?;

    // thread that receives signals
    thread::spawn(move || {
        for sig in signals.forever() {
            println!("Received signal {:?}", sig);
            if stop_send.send(()).is_err() {
                eprintln!("failed to send stop signal");
                break;
            }
        }
    });

    stop_rec.recv().expect("failed to receive stop signal");

    Ok(())
}