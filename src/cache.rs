//! Reclaim expired documents even when an HTTP or stdio client goes idle.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(crate) fn start_expiry_worker() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    // Started only when a document is cached. One small sleeping thread covers
    // both stores and both transports; live entries keep their existing TTLs.
    if std::thread::Builder::new()
        .name("cache-expiry".into())
        .stack_size(128 * 1024)
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            let now = Instant::now();
            crate::share::expire(now);
            crate::moodle::download::expire(now);
        })
        .is_err()
    {
        // Access-time expiry still works, and the next insertion can retry.
        STARTED.store(false, Ordering::Relaxed);
    }
}
