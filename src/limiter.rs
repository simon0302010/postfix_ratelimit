use std::ffi::CString;
use std::process::exit;

use indymilter::{Callbacks, Context, Macros, SocketInfo, Status};
use log::{error, info, warn};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Receiver;
use tokio_rusqlite::Connection;

#[derive(Clone)]
pub struct Limiter {
    conn: Connection,
    interval: u64,
    limit: u64,
}

impl Limiter {
    pub fn new(db: Connection, interval: u64, limit: u64) -> Self {
        Self {
            conn: db,
            interval,
            limit,
        }
    }

    pub async fn run(&self, socket: String, mut stop_rec: Receiver<()>) {
        for i in 0..10 {
            tokio::spawn(test_entry(self.conn.clone(), i));
        }

        let listener = TcpListener::bind(&socket).await.unwrap_or_else(|e| {
            error!("Cannot open milter socket: {}", e);
            exit(1);
        });

        let limiter_connect = self.clone();
        let limiter_mail = self.clone();
        let limiter_rcpt = self.clone();

        let callbacks = Callbacks::new()
            .on_connect(move |cx, hostname, socket_info| {
                let limiter = limiter_connect.clone();
                Box::pin(async move { limiter.handle_connect(cx, hostname, socket_info).await })
            })
            .on_mail(move |cx, args| {
                let limiter = limiter_mail.clone();
                Box::pin(async move { limiter.handle_mail(cx, args).await })
            })
            .on_rcpt(move |cx, args| {
                let limiter = limiter_rcpt.clone();
                Box::pin(async move { limiter.handle_rcpt(cx, args).await })
            });

        let config = Default::default();

        info!("Milter listening on {}", socket);

        let shutdown_signal = async move {
            let _ = stop_rec.recv().await;
        };

        indymilter::run(listener, callbacks, config, shutdown_signal)
            .await
            .unwrap_or_else(|e| {
                error!("Execution of milter failed: {}", e);
                exit(1);
            })
    }

    /// on new connection
    async fn handle_connect(
        &self,
        cx: &mut Context<()>,
        hostname: CString,
        socket_info: SocketInfo,
    ) -> Status {
        println!("CONNECT");
        println!("  hostname: {hostname:?}");
        println!("  socket_info: {socket_info:?}");
        print_macros(&cx.macros);

        Status::Continue
    }

    /// handles rcpt
    async fn handle_rcpt(&self, cx: &mut Context<()>, args: Vec<CString>) -> Status {
        println!("RCPT");
        println!("  args: {args:?}");
        print_macros(&cx.macros);

        Status::Continue
    }

    /// handles mail
    async fn handle_mail(&self, cx: &mut Context<()>, args: Vec<CString>) -> Status {
        println!("MAIL");
        println!("  args: {args:?}");
        print_macros(&cx.macros);

        Status::Continue
    }
}

fn print_macros(macros: &Macros) {
    println!("  macros: {:?}", macros.to_hash_map());
}

/// test entry into db
async fn test_entry(conn: Connection, i: u64) {
    let email = format!("example{}@example.com", i);
    let count = 1;

    if conn
        .call(move |c| {
            c.execute(
                "INSERT INTO emails (address, count) VALUES (?1, ?2)",
                rusqlite::params![email, count],
            )
        })
        .await
        .is_err()
    {
        warn!("failed to insert values into database");
    }
}
