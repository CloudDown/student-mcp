//! Local SSH fixture: no public network or school credentials.
use super::*;
use russh::keys::ssh_key::{private::Ed25519Keypair, PrivateKey};
use russh::server::{self, Server};
use russh::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Peer {
    origin: String,
    connections: Sender<server::Handle>,
    channel: Option<Channel<server::Msg>>,
}

impl Server for Peer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self {
            origin: self.origin.clone(),
            connections: self.connections.clone(),
            channel: None,
        }
    }
}

impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        user: &str,
        _: server::VerifiedPublicKey<'_>,
    ) -> Result<server::Auth, Self::Error> {
        assert_eq!(user, "nomatch");
        Ok(server::Auth::Accept)
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.channel = Some(channel);
        reply.accept().await;
        Ok(())
    }
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut server::Session,
    ) -> Result<bool, Self::Error> {
        assert_eq!(address, "localhost");
        assert_eq!(*port, 1);
        // Deliberately fragment the announcement over separate SSH data frames.
        let text = format!("1: {}/\r\n", self.origin);
        let id = self.channel.as_ref().unwrap().id();
        session.data(id, text[..4].as_bytes().to_vec())?;
        session.data(id, text[4..].as_bytes().to_vec())?;
        let _ = self.connections.send(session.handle());
        Ok(true)
    }
}

struct Relay {
    port: u16,
    connections: Receiver<server::Handle>,
    stop: Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl Relay {
    fn start(origin: String) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop, stops) = channel();
        let (connections, receiver) = channel();
        let join = thread::spawn(move || {
            let rt = runtime();
            rt.block_on(async move {
                let socket = tokio::net::TcpListener::from_std(listener).unwrap();
                let config = Arc::new(server::Config {
                    keys: vec![PrivateKey::from(Ed25519Keypair::from_seed(&[9; 32]))],
                    ..Default::default()
                });
                let mut peer = Peer {
                    origin,
                    connections,
                    channel: None,
                };
                tokio::select! {
                    _ = peer.run_on_socket(config, &socket) => {},
                    _ = async {
                        while stops.try_recv() == Err(std::sync::mpsc::TryRecvError::Empty) {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    } => {},
                }
            });
            rt.shutdown_timeout(Duration::from_millis(100));
        });
        Self {
            port,
            connections: receiver,
            stop,
            join: Some(join),
        }
    }

    fn tunnel(&self, port: u16) -> Tunnel {
        let path = crate::config::config_path();
        let peer_port = self.port;
        let (stop, stops) = channel();
        let (events, receiver) = channel();
        let join = thread::spawn(move || {
            if let Err(message) = super::super::srvus::supervise_at(
                port,
                path,
                stops,
                &events,
                ("127.0.0.1", peer_port),
            ) {
                let _ = events.send(State::Failed(message));
            }
        });
        Tunnel {
            state: State::Starting,
            worker: Some(Worker {
                stop,
                events: receiver,
                join,
            }),
        }
    }

    fn accepted(&self) -> server::Handle {
        self.connections
            .recv_timeout(Duration::from_secs(5))
            .expect("SSH forward registered")
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        let _ = self.join.take().unwrap().join();
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn reconnects_with_the_same_identity_and_rejects_other_forward_targets() {
    crate::config::tests::with_temp_config(|| {
        crate::config::Settings::default().save().unwrap();
        let identity = Identity::load(&crate::config::config_path()).unwrap();
        let relay = Relay::start(identity.origin.clone());
        let listener = crate::http::bind(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let enabled = Arc::new(AtomicBool::new(true));
        let _server = LocalServer(Arc::clone(&enabled));
        thread::spawn(move || {
            let _ = crate::http::serve_controlled(listener, enabled);
        });
        let mut tunnel = relay.tunnel(port);
        let first = await_ready(&mut tunnel);
        let first_handle = relay.accepted();
        let rt = runtime();
        rt.block_on(async {
            for (host, port) in [("192.168.1.1", 1), ("localhost", 22)] {
                let result = tokio::time::timeout(Duration::from_secs(2),
                    first_handle.channel_open_forwarded_tcpip(host, port, "test", 1)).await;
                assert!(matches!(result, Ok(Err(_))), "unrequested targets must be rejected");
            }
            let forwarded = first_handle.channel_open_forwarded_tcpip("localhost", 1, "test", 1).await.unwrap();
            let mut stream = forwarded.into_stream();
            stream.write_all(b"POST /mcp/wrong-token HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response)).await.unwrap().unwrap();
            assert!(response.starts_with(b"HTTP/1.1 401"));
            first_handle.disconnect(russh::Disconnect::ByApplication, "fixture outage".into(), "".into()).await.unwrap();
        });
        // Observe the loss, then wait for automatic retry (without start()).
        let started = Instant::now();
        loop {
            tunnel.poll();
            if matches!(tunnel.state, State::Reconnecting(_)) {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(10));
        }
        assert!(tunnel.origin().is_none());
        assert_eq!(await_ready(&mut tunnel), first);
        let second_handle = relay.accepted();
        let mut pending = rt.block_on(async {
            second_handle
                .channel_open_forwarded_tcpip("localhost", 1, "test", 1)
                .await
                .unwrap()
                .into_stream()
        });
        let at = Instant::now();
        tunnel.stop();
        assert!(at.elapsed() < Duration::from_secs(1));
        // Stopping must also terminate established forwarding tasks, not only
        // clear the displayed URL. A remote EOF or error is acceptable.
        rt.block_on(async {
            let mut byte = [0];
            let result =
                tokio::time::timeout(Duration::from_secs(2), pending.read(&mut byte)).await;
            assert!(
                matches!(result, Ok(Ok(0) | Err(_))),
                "forward survived stop"
            );
            drop(pending);
        });
        assert!(
            relay
                .connections
                .recv_timeout(Duration::from_millis(1200))
                .is_err(),
            "stopped tunnel restarted"
        );
    });
}

#[test]
fn changed_host_key_is_fatal_and_stop_interrupts_the_handshake() {
    crate::config::tests::with_temp_config(|| {
        let path = crate::config::config_path();
        let identity = Identity::load(&path).unwrap();
        verify_host(&identity.host_file, b"wrong-pinned-key").unwrap();
        let relay = Relay::start(identity.origin);
        let mut tunnel = relay.tunnel(8420);
        let started = Instant::now();
        loop {
            tunnel.poll();
            if let State::Failed(message) = &tunnel.state {
                assert!(message.contains("identity changed"));
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(10));
        }
        assert!(relay
            .connections
            .recv_timeout(Duration::from_millis(100))
            .is_err());
        tunnel.stop();

        // An open TCP socket which never sends an SSH greeting must not make
        // window close wait for the 25-second handshake timeout.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let path = path.clone();
        let (stop, stops) = channel();
        let (events, _) = channel();
        let join = thread::spawn(move || {
            super::super::srvus::supervise_at(8420, path, stops, &events, ("127.0.0.1", port))
        });
        let (_socket, _) = listener.accept().unwrap();
        let at = Instant::now();
        stop.send(()).unwrap();
        assert!(join.join().unwrap().is_ok());
        assert!(at.elapsed() < Duration::from_secs(1));
    });
}
