use std::net::SocketAddr;
use std::thread;
use std::{process::exit, sync::mpsc::Receiver, thread::sleep, time::Duration};

use log::{error, info};
use milter::{Context, Milter, Status, on_mail, on_rcpt};
use rusqlite::Connection;

pub struct Limiter {
    conn: Connection,
    stop_rec: Receiver<()>,
}

impl Limiter {
    pub fn new(db: Connection, stop_rec: Receiver<()>) -> Self {
        Self { conn: db, stop_rec }
    }

    pub fn run(self, socket: String) -> Connection {
        Milter::new(&socket)
            .name("Ratelimit")
            .on_mail(handle_mail)
            .on_rcpt(handle_rcpt)
            .run()
            .unwrap_or_else(|e| {
                error!("Milter execution failed: {}", e);
                exit(1);
            });

        self.conn
    }
}

#[on_mail(handle_mail)]
fn handle_email(_: Context<()>, from: Vec<&str>) -> Status {
    info!("Handling outgoing email from {:?}", from.first());
    Status::Continue
}

#[on_rcpt(handle_rcpt)]
fn handle_rec(_: Context<()>, recipient: Vec<&str>) -> Status {
    info!(
        "Handling recipient {:?}",
        recipient
            .iter()
            .filter(|r| !r.trim().is_empty())
            .collect::<Vec<&&str>>()
    );
    Status::Continue
}
