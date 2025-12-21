use std::time::Duration;

use indymilter::{Callbacks, Context, Status};
use log::warn;
use rusqlite::{Connection, params};
use tokio::sync::mpsc::Receiver;
use tokio::time::sleep;

pub struct Limiter {
    conn: Connection,
    stop_rec: Receiver<()>,
    interval: u64,
    limit: u64,
}

impl Limiter {
    pub fn new(db: Connection, stop_rec: Receiver<()>, interval: u64, limit: u64) -> Self {
        Self {
            conn: db,
            stop_rec,
            interval,
            limit,
        }
    }

    pub async fn run(mut self, socket: String) -> Connection {
        let email = "example@example.com";
        let count = 1;

        if self
            .conn
            .execute(
                "INSERT INTO emails (address, count) VALUES (?1, ?2)",
                params![email, count],
            )
            .is_err()
        {
            warn!("failed to insert values into database");
        }

        // main loop
        while self.stop_rec.try_recv().is_err() {
            sleep(Duration::from_millis(16)).await;
        }

        self.conn
    }
}
