use super::identity::{verify_host, Identity};
use super::{retry_delay, State};
use russh::client::{self, ChannelOpenHandle, Msg};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::{sleep, timeout};

enum Failure {
    Retry(&'static str),
    Fatal(String),
}

pub(super) fn supervise(
    port: u16,
    config_path: PathBuf,
    stop: Receiver<()>,
    events: &Sender<State>,
) -> Result<(), String> {
    supervise_at(port, config_path, stop, events, ("srv.us", 22))
}

// Kept private to the tunnel module: tests use a local SSH peer, never an
// environment-selected relay or a production fallback.
pub(super) fn supervise_at(
    port: u16,
    config_path: PathBuf,
    stop: Receiver<()>,
    events: &Sender<State>,
    peer: (&str, u16),
) -> Result<(), String> {
    let identity = Identity::load(&config_path)?;
    let mut attempt = 0;
    loop {
        if stop.try_recv() != Err(TryRecvError::Empty) {
            return Ok(());
        }
        // A runtime per connection ensures no old SSH/forwarding task survives
        // reconnect, user stop or shutdown, including during DNS/authentication.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "Cannot start the public link network worker.".to_string())?;
        let result = runtime.block_on(async {
            tokio::select! {
                _ = stopped(&stop) => None,
                failure = connection(port, &identity, events, peer) => Some(failure),
            }
        });
        runtime.shutdown_timeout(Duration::from_millis(250));
        match result {
            None => return Ok(()),
            Some(Failure::Fatal(message)) => return Err(message),
            Some(Failure::Retry(message)) => {
                let _ = events.send(State::Reconnecting(message.into()));
                // Backoff is interruptible, bounded, and never replaces the key.
                match stop.recv_timeout(retry_delay(attempt)) {
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    _ => return Ok(()),
                }
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

async fn stopped(stop: &Receiver<()>) {
    loop {
        if stop.try_recv() != Err(TryRecvError::Empty) {
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
}

struct Client {
    port: u16,
    host_file: PathBuf,
    host_error: Arc<Mutex<Option<String>>>,
    slots: Arc<Semaphore>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        offered: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let checked = offered
            .public_key()
            .to_bytes()
            .map_err(|_| "Cannot verify the srv.us server identity.".to_string())
            .and_then(|bytes| verify_host(&self.host_file, &bytes));
        if let Err(message) = checked {
            *self.host_error.lock().unwrap() = Some(message);
            return Ok(false);
        }
        Ok(true)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Never let relay-supplied addresses choose an arbitrary LAN target.
        if connected_address != "localhost" || connected_port != 1 {
            return Ok(()); // dropping reply rejects the channel
        }
        let Ok(permit) = Arc::clone(&self.slots).try_acquire_owned() else {
            return Ok(());
        };
        reply.accept().await;
        let port = self.port;
        tokio::spawn(async move {
            let _permit = permit;
            if let Ok(Ok(mut local)) = timeout(
                Duration::from_secs(5),
                TcpStream::connect(("127.0.0.1", port)),
            )
            .await
            {
                let mut remote = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
            }
        });
        Ok(())
    }
}

async fn connection(
    port: u16,
    identity: &Identity,
    events: &Sender<State>,
    peer: (&str, u16),
) -> Failure {
    let host_error = Arc::new(Mutex::new(None));
    let handler = Client {
        port,
        host_file: identity.host_file.clone(),
        host_error: Arc::clone(&host_error),
        slots: Arc::new(Semaphore::new(64)),
    };
    let setup = async {
        let config = client::Config {
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        };
        let mut handle = client::connect(Arc::new(config), peer, handler).await?;
        // Dedicated app key only, never a system SSH agent or school credential.
        let auth = handle
            .authenticate_publickey(
                "nomatch",
                PrivateKeyWithHashAlg::new(Arc::new(identity.key.clone()), None),
            )
            .await?;
        if !auth.success() {
            return Err(russh::Error::RequestDenied);
        }
        let channel = handle.channel_open_session().await?;
        channel.request_shell(false).await?;
        handle.tcpip_forward("localhost", 1).await?;
        Ok::<_, russh::Error>((handle, channel))
    };
    let (mut handle, mut channel) = match timeout(Duration::from_secs(25), setup).await {
        Ok(Ok(connected)) => connected,
        _ => {
            if let Some(message) = host_error.lock().unwrap().take() {
                return Failure::Fatal(message);
            }
            return Failure::Retry("Cannot reach srv.us. Retrying automatically with the same link. Check your connection; some networks block SSH port 22.");
        }
    };
    // Wait for the relay's acknowledgement, not just a derived hostname.
    // Bounded buffer handles fragmented SSH packets without retaining logs.
    let announcement = format!("1: {}/", identity.origin);
    let acknowledged = timeout(Duration::from_secs(10), async {
        let mut buffer = Vec::new();
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    buffer.extend_from_slice(&data);
                    if buffer.len() > 4096 {
                        return false;
                    }
                    if String::from_utf8_lossy(&buffer)
                        .lines()
                        .any(|line| line.trim() == announcement)
                    {
                        return true;
                    }
                }
                ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. } => {
                    return false
                }
                _ => {}
            }
        }
        false
    })
    .await;
    if acknowledged != Ok(true) {
        return Failure::Retry("srv.us did not confirm the public link. Retrying automatically.");
    }
    let _ = events.send(State::Ready(identity.origin.clone()));
    loop {
        tokio::select! {
            _ = &mut handle => break,
            message = channel.wait() => {
                if matches!(message, None | Some(ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. })) {
                    break;
                }
            }
        }
    }
    Failure::Retry(
        "The public link was interrupted. Reconnecting automatically; its address stays the same.",
    )
}
