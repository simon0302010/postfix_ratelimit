use std::ffi::CString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::{env, u64};

use indymilter::{Callbacks, Context, EomContext, SocketInfo, Status};
use log::{debug, error, info, warn};
use regex::Regex;
use rusqlite::params;
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::mpsc::Receiver;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::{LimiterSignals, save_db};

#[derive(Clone)]
pub struct Limiter {
    conn: Connection,
    config: Config,
    mail_regex: Regex,
}

#[derive(Default, Debug)]
struct ConnectionData {
    sender: Option<String>,
    recipients: Vec<String>,
    host: String,
}

impl Limiter {
    pub fn new(db: Connection, config: Config) -> Self {
        let mail_regex = Regex::new(r#"(?:[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*|"(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21\x23-\x5b\x5d-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])*")@(?:(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?|\[(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?|[a-z0-9-]*[a-z0-9]:(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21-\x5a\x53-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])+)\])"#).unwrap();

        Self {
            conn: db,
            config,
            mail_regex,
        }
    }

    pub async fn run(&self, socket: String, mut stop_rec: Receiver<LimiterSignals>) {
        let listener = create_listener(&socket).await;

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

        // stops when it recieves LimiterSignals::STOP and reload on LimiterSignals::RELOAD
        let limiter_shutdown = self.clone();
        let shutdown_signal = async move {
            while let Some(signal) = stop_rec.recv().await {
                match signal {
                    LimiterSignals::STOP => {
                        break;
                    }
                    LimiterSignals::RELOAD => {
                        info!("SIGHUP received. Reloading...");

                        save_db(&self.conn, PathBuf::from(&self.config.db_file))
                            .await
                            .unwrap_or_else(|e| {
                                error!("Failed to save DB: {}", e);
                                exit(1);
                            });

                        let args: Vec<String> = env::args().collect();
                        let err = Command::new(&args[0]).args(&args[1..]).exec();
                        error!("Failed to reload: {}", err);
                    }
                    LimiterSignals::CONFIG => {
                        info!("Showing current configuration:");
                        println!("{}", limiter_shutdown.config);
                    }
                    LimiterSignals::CLEAR_DB => {
                        info!("Clearing database");
                        let _ = limiter_shutdown
                            .conn
                            .call(move |conn| conn.execute("DELETE FROM emails", params![]))
                            .await
                            .unwrap_or_else(|e| {
                                error!("Failed to clear database: {}", e);
                                0
                            });
                    }
                }
            }
        };

        match listener {
            ListenerType::Tcp(l) => indymilter::run(l, callbacks, config, shutdown_signal)
                .await
                .unwrap_or_else(|e| {
                    error!("Execution of milter failed: {}", e);
                    exit(1);
                }),
            ListenerType::Unix(l) => indymilter::run(l, callbacks, config, shutdown_signal)
                .await
                .unwrap_or_else(|e| {
                    error!("Execution of milter failed: {}", e);
                    exit(1);
                }),
        }
    }

    async fn handle_connect(
        &self,
        cx: &mut Context<ConnectionData>,
        socket_info: SocketInfo,
    ) -> Status {
        let host = match socket_info {
            SocketInfo::Inet(addr) => addr.ip().to_string(),
            SocketInfo::Unix(sock) => sock.to_string_lossy().to_string(),
            _ => "Unknown".to_string(),
        };

        let _ = cx.data.replace(ConnectionData {
            sender: None,
            recipients: Vec::new(),
            host,
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

            if self.config.max_recipients != 0
                && data.recipients.len() as u64 > self.config.max_recipients
            {
                warn!(
                    "Rejected email from {} due to exceeding max recipients ({})",
                    data.sender.clone().unwrap_or_else(|| "Unknown".to_string()),
                    self.config.max_recipients
                );
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
            debug!(
                "Received email from {:?} to {:?} from server {}",
                data.sender.clone().unwrap_or_default(),
                data.recipients,
                data.host
            );

            let Some(sender) = data.sender.clone() else {
                if self.config.reject_error {
                    return Status::Reject;
                } else {
                    return Status::Continue;
                }
            };

            let count = if data.recipients.len() > 1 && self.config.count_recipients {
                data.recipients.len() as u64
            } else {
                1
            };

            if data.recipients.len() == 0 {
                warn!(
                    "Cannot find recipients for email from {} ({})",
                    sender, data.host
                )
            }

            let (allowed, emails) = self.allowed(sender.clone(), data.host.clone(), count).await;

            if emails == 0 {
                warn!("Email count from {} ({}) is zero.", sender, data.host);
                if self.config.reject_error {
                    return Status::Reject;
                }
            }

            if allowed {
                Status::Accept
            } else {
                warn!(
                    "Rejected email from {} ({}) due to rate limit being reached ({} over limit)",
                    sender,
                    data.host,
                    emails - self.config.limit
                );
                Status::Reject
            }
        } else {
            warn!("No connection data found in EOM context. Cannot process email.");
            if self.config.reject_error {
                Status::Reject
            } else {
                Status::Continue
            }
        }
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

    /// check whether to allow email
    /// returns (allowed: bool, email count: u64)
    async fn allowed(&self, email: String, host: String, count: u64) -> (bool, u64) {
        let interval_seconds = self.config.interval * 60;
        let limit = self.config.limit;

        let db_host = if self.config.per_host {
            host.clone()
        } else {
            "global".to_string()
        };

        let current_count = self
            .conn
            .call(move |conn| {
                let tx = conn.transaction()?;

                tx.execute(
                    "INSERT INTO emails (address, host, count, time)
                     VALUES (?1, ?2, 0, unixepoch('now'))
                     ON CONFLICT(address, host) DO UPDATE SET
                        count = 0, time = unixepoch('now')
                     WHERE (unixepoch('now') - emails.time) > ?3",
                    params![email, db_host, interval_seconds],
                )?;

                tx.execute(
                    "INSERT INTO emails (address, host, count, time)
                      VALUES (?1, ?2, ?3, unixepoch('now'))
                      ON CONFLICT(address, host) DO UPDATE SET
                         count = emails.count + ?3",
                    params![email, db_host, count],
                )?;

                let new_count: u64 = {
                    let mut stmt =
                        tx.prepare("SELECT count FROM emails WHERE address = ?1 AND host = ?2")?;

                    stmt.query_row(params![email, db_host], |row| row.get(0))
                        .unwrap_or(0)
                };

                tx.commit()?;

                if new_count != 0 && new_count <= limit {
                    debug!(
                        "Allowing Mail sent by {} from host {} with a current count of {}",
                        email, host, new_count
                    );
                }

                Ok::<u64, tokio_rusqlite::Error>(new_count)
            })
            .await
            .unwrap_or_else(|e| {
                error!("Database error: {}", e);
                0
            });

        (current_count <= limit, current_count)
    }
}

async fn create_listener(socket: &str) -> ListenerType {
    if socket.starts_with("unix:") {
        let socket_trimmed = socket.trim_start_matches("unix:");
        let _ = std::fs::remove_file(socket_trimmed);
        let listener = UnixListener::bind(socket_trimmed).unwrap_or_else(|e| {
            error!("Cannot bind Unix socket: {}", e);
            exit(1);
        });
        return ListenerType::Unix(listener);
    } else if socket.starts_with("inet:") {
        let socket_trimmed = socket.trim_start_matches("inet:");
        let listener = TcpListener::bind(&socket_trimmed)
            .await
            .unwrap_or_else(|e| {
                error!("Cannot bind TCP socket: {}", e);
                exit(1);
            });
        return ListenerType::Tcp(listener);
    } else {
        error!(
            "Unknown socket type: {}. Please specify with unix: or inet:",
            socket
        );
        exit(1);
    }
}

enum ListenerType {
    Tcp(TcpListener),
    Unix(UnixListener),
}
