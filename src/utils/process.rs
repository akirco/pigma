//! Process-level introspection helpers.

/// Read the current RSS in KB from /proc/self/status.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn mem_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}
