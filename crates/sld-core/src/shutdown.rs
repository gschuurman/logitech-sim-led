//! Cooperative shutdown signal, shared by every source and output. One
//! `ShutdownHandle` lives in `sld-service::main`; every spawned task gets a
//! cloned `ShutdownSignal` to `select!` against in its run loop.

use tokio::sync::watch;

#[derive(Clone)]
pub struct ShutdownSignal(watch::Receiver<bool>);

impl ShutdownSignal {
    pub fn new(rx: watch::Receiver<bool>) -> Self {
        Self(rx)
    }

    /// Resolves once shutdown has been requested. Safe to call repeatedly
    /// (e.g. in a `tokio::select!` loop) -- it never resolves before
    /// shutdown, and resolves immediately on every call after.
    pub async fn cancelled(&mut self) {
        loop {
            if *self.0.borrow() {
                return;
            }
            if self.0.changed().await.is_err() {
                // Sender dropped without ever signaling shutdown; treat that
                // as "shutting down" too so tasks don't spin forever.
                return;
            }
        }
    }

    pub fn is_shutdown(&self) -> bool {
        *self.0.borrow()
    }
}

#[derive(Clone)]
pub struct ShutdownHandle(watch::Sender<bool>);

impl ShutdownHandle {
    pub fn new() -> (Self, ShutdownSignal) {
        let (tx, rx) = watch::channel(false);
        (Self(tx), ShutdownSignal::new(rx))
    }

    pub fn shutdown(&self) {
        let _ = self.0.send(true);
    }
}
