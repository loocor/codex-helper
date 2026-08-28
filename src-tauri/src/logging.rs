use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

const LATEST_LOG_MAX_BYTES: usize = 128 * 1024;
const LATEST_LOG_MAX_LINES: usize = 50;
const LATEST_LOG_LINE_MAX_CHARS: usize = 2048;

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

    pub fn read_latest(&self) -> anyhow::Result<LatestLog> {
        match read_file_tail(&self.log_path, LATEST_LOG_MAX_BYTES) {
            Ok(contents) => Ok(LatestLog {
                path: self.log_path.clone(),
                records: parse_latest_records(&contents),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LatestLog {
                path: self.log_path.clone(),
                records: Vec::new(),
            }),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LatestLog {
    pub path: PathBuf,
    pub records: Vec<LatestLogRecord>,
}

#[derive(Debug, Clone)]
pub struct LatestLogRecord {
    pub timestamp: String,
    pub event: String,
    pub summary: String,
}

fn read_file_tail(path: &std::path::Path, max_bytes: usize) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len > max_bytes as u64 {
        file.seek(SeekFrom::End(-(max_bytes as i64)))?;
    }
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    if len > max_bytes as u64 {
        if let Some((_, rest)) = buf.split_once('\n') {
            return Ok(rest.to_string());
        }
    }
    Ok(buf)
}

fn parse_latest_records(contents: &str) -> Vec<LatestLogRecord> {
    let lines: Vec<&str> = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines
        .into_iter()
        .rev()
        .take(LATEST_LOG_MAX_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(summarize_log_line)
        .collect()
}

fn summarize_log_line(line: &str) -> LatestLogRecord {
    let clipped: String = line.chars().take(LATEST_LOG_LINE_MAX_CHARS).collect();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&clipped) {
        let event = value
            .get("event")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("event")
            .to_string();
        let timestamp = value
            .get("timestamp")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let summary = value
            .get("detail")
            .and_then(|detail| {
                detail
                    .get("message")
                    .or_else(|| detail.get("path"))
                    .or_else(|| detail.get("status"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        return LatestLogRecord {
            timestamp,
            event,
            summary,
        };
    }
    LatestLogRecord {
        timestamp: String::new(),
        event: "log".to_string(),
        summary: clipped,
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

    #[test]
    fn read_latest_tails_without_loading_the_whole_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        let mut body = String::new();
        for index in 0..200 {
            body.push_str(&format!(
                "{{\"timestamp\":\"t{index}\",\"event\":\"bridge.request\",\"detail\":{{\"path\":\"/x{index}\",\"status\":\"ok\"}}}}\n"
            ));
        }
        std::fs::create_dir_all(temp_dir.path().join("logs")).expect("dir");
        std::fs::write(logger.log_path(), body).expect("write");
        let latest = logger.read_latest().expect("latest");
        assert!(!latest.records.is_empty());
        assert!(latest.records.len() <= 50);
        assert_eq!(latest.records.last().unwrap().event, "bridge.request");
        assert!(latest.records.last().unwrap().summary.contains("/x199"));
    }
}
