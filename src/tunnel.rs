//! Managed srv.us reverse SSH tunnel. No account, external client or daemon.
#[path = "tunnel/identity.rs"]
mod identity;
#[path = "tunnel/srvus.rs"]
mod srvus;
#[cfg(test)]
#[path = "tunnel/tests.rs"]
mod tests;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Starting,
    Reconnecting(String),
    Ready(String),
    Failed(String),
    Stopped,
}

#[derive(Default)]
pub struct Tunnel {
    pub state: State,
    worker: Option<Worker>,
}

struct Worker {
    stop: Sender<()>,
    events: Receiver<State>,
    join: JoinHandle<()>,
}

impl Tunnel {
    pub fn start(&mut self, port: u16) {
        self.stop();
        self.state = State::Starting;
        let (stop, stops) = channel();
        let (events, receiver) = channel();
        let path = crate::config::config_path();
        let join = thread::spawn(move || {
            if let Err(message) = srvus::supervise(port, path, stops, &events) {
                let _ = events.send(State::Failed(message));
            }
        });
        self.worker = Some(Worker {
            stop,
            events: receiver,
            join,
        });
    }

    pub fn poll(&mut self) {
        if let Some(worker) = &self.worker {
            while let Ok(state) = worker.events.try_recv() {
                self.state = state;
            }
            if worker.join.is_finished() {
                let worker = self.worker.take().unwrap();
                if worker.join.join().is_err() {
                    self.state =
                        State::Failed("The public link was interrupted. Try again.".into());
                }
            }
        }
    }

    pub fn origin(&self) -> Option<&str> {
        match &self.state {
            State::Ready(url) => Some(url),
            _ => None,
        }
    }

    pub fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.stop.send(());
            let _ = worker.join.join();
        }
        self.state = State::Stopped;
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_secs((2u64.saturating_pow(attempt.min(5))).min(30))
}
