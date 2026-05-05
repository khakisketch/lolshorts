use std::io::Write;
use std::path::Path;

/// Log an audit event (user action) to the audit log file.
/// Format: ISO8601 | ACTION | DETAIL
pub fn log_audit_event(audit_dir: &Path, action: &str, detail: &str) {
    let log_path = audit_dir.join("audit.log");
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    let line = format!("{} | {} | {}\n", timestamp, action, detail);

    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut f| f.write_all(line.as_bytes()))
    {
        tracing::warn!("Failed to write audit log: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        log_audit_event(dir.path(), "SETTINGS_SAVE", "video_bitrate=20000");
        let log_path = dir.path().join("audit.log");
        assert!(log_path.exists());
    }

    #[test]
    fn test_audit_log_contains_action() {
        let dir = tempfile::tempdir().unwrap();
        log_audit_event(dir.path(), "UPLOAD_START", "clip_id=abc123");
        let contents = std::fs::read_to_string(dir.path().join("audit.log")).unwrap();
        assert!(contents.contains("UPLOAD_START"));
        assert!(contents.contains("clip_id=abc123"));
    }

    #[test]
    fn test_audit_log_appends_multiple_events() {
        let dir = tempfile::tempdir().unwrap();
        log_audit_event(dir.path(), "ACTION_ONE", "detail_one");
        log_audit_event(dir.path(), "ACTION_TWO", "detail_two");
        let contents = std::fs::read_to_string(dir.path().join("audit.log")).unwrap();
        assert!(contents.contains("ACTION_ONE"));
        assert!(contents.contains("ACTION_TWO"));
        // Two lines (plus possible trailing newline)
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_audit_log_format_has_pipe_separators() {
        let dir = tempfile::tempdir().unwrap();
        log_audit_event(dir.path(), "TEST_ACTION", "test_detail");
        let contents = std::fs::read_to_string(dir.path().join("audit.log")).unwrap();
        // Format: ISO8601 | ACTION | DETAIL
        let parts: Vec<&str> = contents.trim().splitn(3, " | ").collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], "TEST_ACTION");
        assert_eq!(parts[2], "test_detail");
    }

    #[test]
    fn test_audit_log_invalid_dir_does_not_panic() {
        // Writing to a non-existent directory should warn but not panic
        let bad_path = Path::new("/nonexistent/path/that/does/not/exist");
        // This must not panic
        log_audit_event(bad_path, "ACTION", "detail");
    }
}
