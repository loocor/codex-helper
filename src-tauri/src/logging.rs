use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DiagnosticLogger {
    log_path: PathBuf,
}

impl DiagnosticLogger {
    pub fn new(logs_dir: PathBuf) -> Self {
        Self {
            log_path: logs_dir.join("codex-helper.jsonl"),
        }
    }

    pub fn append(&self, event: &str, detail: serde_json::Value) -> anyhow::Result<()> {
        if event.trim().is_empty() {
            anyhow::bail!("Diagnostic event name is empty");
        }
        let parent = self
            .log_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Diagnostic log path has no parent"))?;
        fs::create_dir_all(parent)?;
        let record = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "event": event,
            "detail": detail,
        });
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        writeln!(file, "{}", serde_json::to_string(&record)?)?;
        Ok(())
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn logging_appends_jsonl_records() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));

        logger
            .append(
                "launcher.starting",
                serde_json::json!({ "debugPort": 9229 }),
            )
            .expect("append log");

        let contents = fs::read_to_string(logger.log_path()).expect("read log");
        let line = contents.lines().next().expect("first line");
        let record: serde_json::Value = serde_json::from_str(line).expect("json record");
        assert_eq!(record["event"], "launcher.starting");
        assert_eq!(record["detail"]["debugPort"], 9229);
        assert!(record["timestamp"].as_str().is_some());
    }

    #[test]
    fn logging_rejects_empty_event_names() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));

        let error = logger.append(" ", serde_json::json!({})).unwrap_err();

        assert_eq!(error.to_string(), "Diagnostic event name is empty");
    }
}
