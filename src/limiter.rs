use std::ffi::CString;
use std::process::exit;

use indymilter::{Callbacks, Context, EomContext, SocketInfo, Status};
use log::{error, info, warn};
use regex::Regex;
use rusqlite::{Params, params};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Receiver;
use tokio_rusqlite::Connection;

#[derive(Clone)]
pub struct Limiter {
    conn: Connection,
    interval: u64,
    limit: u64,
    max_recipients: u64,
    mail_regex: Regex,
}

#[derive(Default)]
struct ConnectionData {
    sender: Option<String>,
    recipients: Vec<String>,
    ip: String,
}

#[derive(Clone)]
struct LimitData {
    count: u64,
    time: u64,
}

impl Limiter {
    pub fn new(db: Connection, interval: u64, limit: u64, max_recipients: u64) -> Self {
        let mail_regex = Regex::new(r#"(?:[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*|"(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21\x23-\x5b\x5d-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])*")@(?:(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?|\[(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?|[a-z0-9-]*[a-z0-9]:(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21-\x5a\x53-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])+)\])"#).unwrap();

        Self {
            conn: db,
            interval,
            limit,
            max_recipients,
            mail_regex,
        }
    }

    pub async fn run(&self, socket: String, mut stop_rec: Receiver<()>) {
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
        let current_recipient = self
            .email_regex(args.first().map(|s| s.to_string_lossy().to_string()))
            .await;

        if let Some(data) = cx.data.as_mut() {
            match current_recipient {
                Some(rec) => {
                    data.recipients.push(rec);
                }
                None => {}
            }

            if self.max_recipients != 0 && data.recipients.len() as u64 > self.max_recipients {
                return Status::Reject;
            }
        }

        Status::Continue
    }

    /// handles mail
    async fn handle_mail(&self, cx: &mut Context<ConnectionData>, args: Vec<CString>) -> Status {
        let sender = self
            .email_regex(args.first().map(|s| s.to_string_lossy().to_string()))
            .await;

        if let Some(data) = cx.data.as_mut() {
            data.sender = sender;
        }

        Status::Continue
    }

    async fn handle_eom(&self, cx: &mut EomContext<ConnectionData>) -> Status {
        if let Some(data) = cx.data.as_ref() {
            info!(
                "Received Email from {:?} to {:?} from server {}",
                data.sender.clone().unwrap_or_default(),
                data.recipients,
                data.ip
            );

            return Status::Accept;
        } else {
            warn!("No connection data found in EOM context. Cannot process email.")
        }

        // first check if time is over for address and reset if needed
        // then update_count and check if it went over the budget
        // log it and reject

        Status::Continue
    }

    async fn email_regex(&self, email: Option<String>) -> Option<String> {
        let email = match email {
            Some(e) => e,
            None => return None,
        };

        if let Some(captures) = self.mail_regex.captures(&email)
            && let Some(matched) = captures.get(0)
        {
            return Some(matched.as_str().to_string());
        }
        None
    }
}

/// resets the time and count field in db
async fn reset_row(conn: Connection, address: String, interval: u64) {
    // converting from minutes to seconds
    let interval = interval * 60;

    if conn
        .call(move |c| {
            c.execute(
                "INSERT INTO emails (address, count, time)
            VALUES (?1, 0, strftime('%s','now'))
            ON CONFLICT(address)
            DO UPDATE SET
                time  = excluded.time,
                count = 0
            WHERE excluded.time - emails.time > ?2;",
                params![address, interval],
            )
        })
        .await
        .is_err()
    {
        warn!("Failed to reset row ")
    }
}

/// updates count in database
async fn update_count(conn: Connection, address: String, count: u64) {
    if conn
        .call(move |c| {
            c.execute(
                "INSERT INTO emails (address, count)
                VALUES (?1, ?2)
                ON CONFLICT(address)
                DO UPDATE SET count = count + ?2;",
                rusqlite::params![address, count],
            )
        })
        .await
        .is_err()
    {
        warn!("Failed to insert values into database");
    }
}

async fn find_email(conn: Connection, address: &str) -> Option<LimitData> {
    let address = address.to_string();

    conn.call(move |c| {
        let result = c.query_row(
            "SELECT count, time FROM emails WHERE address = ?1",
            rusqlite::params![address],
            |row| {
                let count: u64 = row.get(0)?;
                let time: u64 = row.get(1)?;
                Ok(LimitData { count, time })
            },
        );

        Ok::<std::option::Option<LimitData>, tokio_rusqlite::Error>(result.ok())
    })
    .await
    .unwrap_or(None)
}
