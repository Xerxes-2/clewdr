use std::sync::LazyLock;
use std::sync::{atomic::{AtomicI64, AtomicU64, Ordering}, Mutex};

// metrics
pub static LAST_WRITE_TS: LazyLock<AtomicI64> = LazyLock::new(|| AtomicI64::new(0));
pub static WRITE_ERROR_COUNT: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
pub static TOTAL_WRITES: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
pub static TOTAL_WRITE_NANOS: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
pub static LAST_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub fn mark_write_ok() {
    let now = chrono::Utc::now().timestamp();
    LAST_WRITE_TS.store(now, Ordering::Relaxed);
}
pub fn mark_write_err() { WRITE_ERROR_COUNT.fetch_add(1, Ordering::Relaxed); }
pub fn record_duration(start: std::time::Instant) {
    let dur = start.elapsed();
    TOTAL_WRITES.fetch_add(1, Ordering::Relaxed);
    TOTAL_WRITE_NANOS.fetch_add(dur.as_nanos() as u64, Ordering::Relaxed);
}
pub fn record_error_msg(e: &dyn std::error::Error) {
    if let Ok(mut g) = LAST_ERROR.lock() { *g = Some(e.to_string()); }
}

