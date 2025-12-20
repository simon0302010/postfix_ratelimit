use std::sync::mpsc::Receiver;

use rusqlite::Connection;

pub struct Limiter {
    conn: Connection,
    stop_rec: Receiver<()>,
}

impl Limiter {
    pub fn new(db: Connection, stop_rec: Receiver<()>) -> Self {
        Self { conn: db, stop_rec }
    }

    pub fn run(self) -> Connection {
        let _ = self.stop_rec.recv();
        self.conn
    }
}
