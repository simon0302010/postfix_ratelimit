use std::borrow::Borrow;
use std::ffi::{CStr, CString};
use std::process::exit;

use indymilter::{Callbacks, Context, EomContext, Macros, SocketInfo, Status};
use log::{error, info, warn};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Receiver;
use tokio_rusqlite::Connection;

#[derive(Clone)]
pub struct Limiter {
    conn: Connection,
    interval: u64,
    limit: u64,
    max_recipients: u64,
}

#[derive(Default)]
struct ConnectionData {
    sender: Option<String>,
    recipients: Vec<String>,
    ip: String,
}

impl Limiter {
    pub fn new(db: Connection, interval: u64, limit: u64, max_recipients: u64) -> Self {
        Self {
            conn: db,
            interval,
            limit,
            max_recipients,
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
        let limiter_eom = self.clone();

        // define callbacks
        let callbacks = Callbacks::new()
            .on_connect(move |cx, _, socket_info| {
                let limiter = limiter_connect.clone();
                Box::pin(async move { limiter.handle_connect(cx, socket_info).await })
            })
            .on_mail(move |cx, args| {
                let limiter = limiter_mail.clone();
                Box::pin(async move { limiter.handle_mail(cx, args).await })
            })
            .on_rcpt(move |cx, args| {
                let limiter = limiter_rcpt.clone();
                Box::pin(async move { limiter.handle_rcpt(cx, args).await })
            })
            .on_eom(move |cx| {
                let limiter = limiter_eom.clone();
                Box::pin(async move { limiter.handle_eom(cx).await })
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

    async fn handle_connect(
        &self,
        cx: &mut Context<ConnectionData>,
        socket_info: SocketInfo,
    ) -> Status {
        let ip = match socket_info {
            SocketInfo::Inet(addr) => addr.ip().to_string(),
            SocketInfo::Unix(sock) => sock.to_string_lossy().to_string(),
            _ => "Unknown".to_string(),
        };

        let _ = cx.data.replace(ConnectionData {
            sender: None,
            recipients: Vec::new(),
            ip,
        });

        Status::Continue
    }

    /// handles rcpt
    async fn handle_rcpt(&self, cx: &mut Context<ConnectionData>, args: Vec<CString>) -> Status {
        let current_recipient = args
            .first()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(data) = cx.data.as_mut() {
            data.recipients.push(current_recipient);

            if self.max_recipients != 0 && data.recipients.len() as u64 > self.max_recipients {
                return Status::Reject;
            }
        }

        Status::Continue
    }

    /// handles mail
    async fn handle_mail(&self, cx: &mut Context<ConnectionData>, args: Vec<CString>) -> Status {
        let sender = args.first().map(|s| s.to_string_lossy().to_string());

        if let Some(data) = cx.data.as_mut() {
            data.sender = sender;
        }

        Status::Continue
    }

    async fn handle_eom(&self, cx: &mut EomContext<ConnectionData>) -> Status {
        if let Some(data) = cx.data.as_ref() {
            info!(
                "Received Email from {:?} to {:?} from server {}",
                data.sender.clone().unwrap_or("Unknown".to_string()),
                data.recipients,
                data.ip
            );

            return Status::Accept;
        } else {
            warn!("No connection data found in EOM context. Cannot process email.")
        }

        Status::Continue
    }
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
