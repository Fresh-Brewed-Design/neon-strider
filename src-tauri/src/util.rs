//! Small shared helpers: the wall clock, duration text, and running blocking
//! work off the async runtime.

/// Wall-clock time as epoch milliseconds.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// `now_ms` for the state machines, which count time in `u64`.
pub fn now_ms_u64() -> u64 {
    now_ms().max(0) as u64
}

/// H:MM:SS, or M:SS under an hour.
pub fn format_duration(total_s: i64) -> String {
    let t = total_s.max(0);
    let (h, m, s) = (t / 3600, (t % 3600) / 60, t % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

/// Run blocking work (SQLite, files, a listener) off the async runtime.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}
