use std::{sync::mpsc::Receiver, thread::sleep, time::Duration};

use log::warn;
use rusqlite::{Connection, params};

pub struct Limiter {
    conn: Connection,
    stop_rec: Receiver<()>,
}

impl Limiter {
    pub fn new(db: Connection, stop_rec: Receiver<()>) -> Self {
        Self { conn: db, stop_rec }
    }

    pub fn run(self) -> Connection {
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
            sleep(Duration::from_millis(16));
        }

        self.conn
    }
}
