use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExportManifest {
    pub exported_at: String,
    pub app_version: String,
    pub includes: Vec<String>,
}

/// Export user data as a JSON manifest (without video files or auth tokens).
pub fn collect_export_data(_app_data_dir: &std::path::Path) -> ExportManifest {
    // Use chrono for RFC3339 timestamp (chrono is already in Cargo.toml)
    let exported_at = chrono::Utc::now().to_rfc3339();

    ExportManifest {
        exported_at,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        includes: vec![
            "settings".to_string(),
            "clip_metadata".to_string(),
            "game_history".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_export_data() {
        let path = std::path::Path::new(".");
        let manifest = collect_export_data(path);
        assert!(!manifest.exported_at.is_empty());
        assert!(!manifest.app_version.is_empty());
        assert_eq!(manifest.includes.len(), 3);
    }
}
