use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use chrono::{DateTime, Local};
use regex::RegexBuilder;
use serde_json::{json, Value};

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 200;
const SUMMARY_MAX_CHARS: usize = 2048;
const STRING_MAX_CHARS: usize = 2048;
const DATA_URL_VISIBLE_CHARS: usize = 96;
const READ_CHUNK_BYTES: usize = 64 * 1024;
const LINE_MAX_BYTES: usize = 256 * 1024;
const PATTERN_MAX_CHARS: usize = 1024;
const REGEX_SIZE_LIMIT: usize = 1024 * 1024;
const LEGACY_LOG_NAME: &str = "codex-helper.jsonl";
const LEGACY_MIGRATING_NAME: &str = "codex-helper.jsonl.migrating";
const UNPARSED_LOG_NAME: &str = "codex-helper-unparsed.jsonl";
const DATED_LOG_PREFIX: &str = "codex-helper-";
const DATED_LOG_SUFFIX: &str = ".jsonl";

#[derive(Debug)]
pub struct DiagnosticLogger {
    logs_dir: PathBuf,
    state: Mutex<LoggerState>,
}

#[derive(Debug, Default)]
struct LoggerState {
    migrated: bool,
}

impl DiagnosticLogger {
    pub fn new(logs_dir: PathBuf) -> Self {
        Self {
            logs_dir,
            state: Mutex::new(LoggerState::default()),
        }
    }

    pub fn log_path(&self) -> PathBuf {
        dated_log_path(&self.logs_dir, &local_date_today())
    }

    pub fn append(&self, event: &str, detail: Value) -> anyhow::Result<()> {
        if event.trim().is_empty() {
            anyhow::bail!("Diagnostic event name is empty");
        }
        self.ensure_ready()?;
        let record = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "event": event,
            "detail": sanitize_value(detail),
        });
        let path = self.log_path();
        append_jsonl_line(&path, &serde_json::to_string(&record)?)
    }

    pub fn list_records(
        &self,
        date: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
        event: Option<&str>,
    ) -> anyhow::Result<LogPage> {
        let limit = normalize_limit(limit)?;
        self.ensure_ready()?;
        let dates = self.list_dates()?;
        let selected_dates = filter_dates(&dates, date)?;
        if selected_dates.is_empty() {
            return Ok(LogPage {
                path: self.page_path(date, &dates),
                date: date.map(str::to_string),
                dates,
                records: Vec::new(),
                cursor: None,
                has_more: false,
            });
        }
        let (mut date_index, mut end_exclusive) =
            list_start_position(&selected_dates, cursor, |value| {
                file_len(&dated_log_path(&self.logs_dir, value))
            })?;
        let mut records = Vec::new();
        let mut page_path = dated_log_path(&self.logs_dir, &selected_dates[date_index]);
        while records.len() < limit && date_index < selected_dates.len() {
            let current_date = &selected_dates[date_index];
            let path = dated_log_path(&self.logs_dir, current_date);
            if records.is_empty() {
                page_path = path.clone();
            }
            let mut scan_end = end_exclusive;
            loop {
                if records.len() == limit {
                    break;
                }
                let chunk = if event.is_some() {
                    200
                } else {
                    limit - records.len()
                };
                let (lines, next_end) = read_lines_backwards(&path, scan_end, chunk)?;
                if lines.is_empty() {
                    break;
                }
                for line in lines {
                    let record = parse_log_line(&line.text);
                    if !matches_event_filter(&record.event, event) {
                        continue;
                    }
                    records.push(record);
                    if records.len() == limit {
                        let cursor = older_cursor_after_file(
                            &selected_dates,
                            date_index,
                            line.start,
                            |value| file_len(&dated_log_path(&self.logs_dir, value)),
                        )?;
                        return Ok(LogPage {
                            path: page_path,
                            date: date.map(str::to_string),
                            dates,
                            records,
                            cursor: cursor.clone(),
                            has_more: cursor.is_some(),
                        });
                    }
                }
                if event.is_none() || next_end == 0 || next_end >= scan_end {
                    break;
                }
                scan_end = next_end;
            }
            date_index += 1;
            end_exclusive = if date_index < selected_dates.len() {
                file_len(&dated_log_path(&self.logs_dir, &selected_dates[date_index]))?
            } else {
                0
            };
        }
        Ok(LogPage {
            path: page_path,
            date: date.map(str::to_string),
            dates,
            records,
            cursor: None,
            has_more: false,
        })
    }

    pub fn search_records(
        &self,
        pattern: &str,
        regex: bool,
        date: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
        event: Option<&str>,
    ) -> anyhow::Result<LogSearchPage> {
        let limit = normalize_limit(limit)?;
        let pattern = pattern.trim();
        if pattern.is_empty() {
            anyhow::bail!("Search pattern is empty");
        }
        if pattern.chars().count() > PATTERN_MAX_CHARS {
            anyhow::bail!("Search pattern is too long");
        }
        self.ensure_ready()?;
        let matcher = SearchMatcher::new(pattern, regex)?;
        let dates = self.list_dates()?;
        let selected_dates = filter_dates(&dates, date)?;
        if selected_dates.is_empty() {
            return Ok(LogSearchPage {
                path: self.page_path(date, &dates),
                dates,
                matches: Vec::new(),
                cursor: None,
                has_more: false,
            });
        }
        let (mut date_index, mut end_exclusive) =
            list_start_position(&selected_dates, cursor, |value| {
                file_len(&self.search_path(value))
            })?;
        let mut matches = Vec::new();
        let mut page_path = self.search_path(&selected_dates[date_index]);
        while matches.len() < limit && date_index < selected_dates.len() {
            let current_date = &selected_dates[date_index];
            let path = self.search_path(current_date);
            if matches.is_empty() {
                page_path = path.clone();
            }
            let mut scan_end = end_exclusive;
            while matches.len() < limit {
                let (lines, next_end) = read_lines_backwards(&path, scan_end, 200)?;
                if lines.is_empty() {
                    break;
                }
                for line in lines {
                    let record = parse_log_line(&line.text);
                    if !matches_event_filter(&record.event, event) {
                        continue;
                    }
                    if let Some(found) = matcher.preview(&line.text) {
                        matches.push(LogSearchMatch {
                            date: current_date.clone(),
                            path: path.clone(),
                            timestamp: record.timestamp,
                            event: record.event,
                            summary: record.summary,
                            preview: found,
                            detail: record.detail,
                        });
                        if matches.len() == limit {
                            let cursor = older_cursor_after_file(
                                &selected_dates,
                                date_index,
                                line.start,
                                |value| file_len(&self.search_path(value)),
                            )?;
                            return Ok(LogSearchPage {
                                path: page_path,
                                dates,
                                matches,
                                cursor: cursor.clone(),
                                has_more: cursor.is_some(),
                            });
                        }
                    }
                }
                if next_end == 0 || next_end >= scan_end {
                    break;
                }
                scan_end = next_end;
            }
            date_index += 1;
            end_exclusive = if date_index < selected_dates.len() {
                file_len(&self.search_path(&selected_dates[date_index]))?
            } else {
                0
            };
        }
        Ok(LogSearchPage {
            path: page_path,
            dates,
            matches,
            cursor: None,
            has_more: false,
        })
    }

    pub fn read_latest(&self) -> anyhow::Result<LogPage> {
        self.list_records(None, None, DEFAULT_PAGE_LIMIT, None)
    }

    fn ensure_ready(&self) -> anyhow::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Diagnostic logger lock poisoned"))?;
        fs::create_dir_all(&self.logs_dir)?;
        if !state.migrated {
            migrate_legacy_logs(&self.logs_dir)?;
            state.migrated = true;
        }
        Ok(())
    }

    fn list_dates(&self) -> anyhow::Result<Vec<String>> {
        let mut dates = Vec::new();
        let entries = match fs::read_dir(&self.logs_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(dates),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let name = entry?.file_name();
            if let Some(date) = parse_dated_log_name(&name.to_string_lossy()) {
                dates.push(date);
            }
        }
        dates.sort();
        dates.reverse();
        if unparsed_log_path(&self.logs_dir).is_file() {
            dates.push("unparsed".to_string());
        }
        Ok(dates)
    }

    fn page_path(&self, date: Option<&str>, dates: &[String]) -> PathBuf {
        if let Some(date) = date {
            return dated_log_path(&self.logs_dir, date);
        }
        if let Some(latest) = dates.first() {
            return dated_log_path(&self.logs_dir, latest);
        }
        self.logs_dir.clone()
    }

    fn search_path(&self, date: &str) -> PathBuf {
        if date == "unparsed" {
            unparsed_log_path(&self.logs_dir)
        } else {
            dated_log_path(&self.logs_dir, date)
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogPage {
    pub path: PathBuf,
    pub date: Option<String>,
    pub dates: Vec<String>,
    pub records: Vec<LogRecord>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct LogSearchPage {
    pub path: PathBuf,
    pub dates: Vec<String>,
    pub matches: Vec<LogSearchMatch>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct LogRecord {
    pub timestamp: String,
    pub event: String,
    pub summary: String,
    pub detail: Value,
}

#[derive(Debug, Clone)]
pub struct LogSearchMatch {
    pub date: String,
    pub path: PathBuf,
    pub timestamp: String,
    pub event: String,
    pub summary: String,
    pub preview: String,
    pub detail: Value,
}

#[derive(Debug, Clone)]
struct CollectedLine {
    start: u64,
    text: String,
}

enum SearchMatcher {
    Literal(String),
    Regex(regex::Regex),
}

impl SearchMatcher {
    fn new(pattern: &str, regex: bool) -> anyhow::Result<Self> {
        if regex {
            let compiled = RegexBuilder::new(pattern)
                .size_limit(REGEX_SIZE_LIMIT)
                .dfa_size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|error| anyhow::anyhow!("Search regex is invalid: {error}"))?;
            Ok(Self::Regex(compiled))
        } else {
            Ok(Self::Literal(pattern.to_string()))
        }
    }

    fn preview(&self, line: &str) -> Option<String> {
        match self {
            Self::Literal(pattern) => line
                .find(pattern.as_str())
                .map(|index| preview_at(line, index, pattern.chars().count())),
            Self::Regex(regex) => regex
                .find(line)
                .map(|found| preview_at(line, found.start(), found.as_str().chars().count())),
        }
    }
}

fn matches_event_filter(event: &str, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(filter) if filter.ends_with('.') => event.starts_with(filter),
        Some(filter) => event == filter,
    }
}

fn normalize_limit(limit: usize) -> anyhow::Result<usize> {
    if limit == 0 {
        anyhow::bail!("Log page limit must be greater than 0");
    }
    if limit > MAX_PAGE_LIMIT {
        anyhow::bail!("Log page limit must be at most {MAX_PAGE_LIMIT}");
    }
    Ok(limit)
}

fn local_date_today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn local_date_from_timestamp(timestamp: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|value| value.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

fn is_log_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes.iter().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                true
            } else {
                byte.is_ascii_digit()
            }
        })
}

fn parse_dated_log_name(name: &str) -> Option<String> {
    let date = name
        .strip_prefix(DATED_LOG_PREFIX)?
        .strip_suffix(DATED_LOG_SUFFIX)?;
    is_log_date(date).then(|| date.to_string())
}

fn dated_log_path(logs_dir: &Path, date: &str) -> PathBuf {
    logs_dir.join(format!("{DATED_LOG_PREFIX}{date}{DATED_LOG_SUFFIX}"))
}

fn unparsed_log_path(logs_dir: &Path) -> PathBuf {
    logs_dir.join(UNPARSED_LOG_NAME)
}

fn parse_cursor(cursor: &str) -> anyhow::Result<(String, u64)> {
    let (date, offset) = cursor
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("Log cursor is invalid"))?;
    if date != "unparsed" && !is_log_date(date) {
        anyhow::bail!("Log cursor date is invalid");
    }
    let offset = offset
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("Log cursor offset is invalid"))?;
    Ok((date.to_string(), offset))
}

fn filter_dates(dates: &[String], date: Option<&str>) -> anyhow::Result<Vec<String>> {
    match date {
        None => Ok(dates.to_vec()),
        Some("unparsed") => Ok(dates
            .iter()
            .filter(|value| value.as_str() == "unparsed")
            .cloned()
            .collect()),
        Some(date) if is_log_date(date) => Ok(dates
            .iter()
            .filter(|value| value.as_str() == date)
            .cloned()
            .collect()),
        Some(_) => anyhow::bail!("Log date is invalid"),
    }
}

fn file_len(path: &Path) -> anyhow::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn list_start_position(
    dates: &[String],
    cursor: Option<&str>,
    len_for_date: impl Fn(&str) -> anyhow::Result<u64>,
) -> anyhow::Result<(usize, u64)> {
    if dates.is_empty() {
        anyhow::bail!("Log cursor date is not available");
    }
    match cursor {
        None => Ok((0, len_for_date(&dates[0])?)),
        Some(cursor) => {
            let (date, offset) = parse_cursor(cursor)?;
            let index = dates
                .iter()
                .position(|value| value == &date)
                .ok_or_else(|| anyhow::anyhow!("Log cursor date is not available"))?;
            let len = len_for_date(&date)?;
            Ok((index, offset.min(len)))
        }
    }
}

fn older_cursor_after_file(
    dates: &[String],
    date_index: usize,
    next_end: u64,
    len_for_date: impl Fn(&str) -> anyhow::Result<u64>,
) -> anyhow::Result<Option<String>> {
    let mut index = date_index;
    let mut end = next_end;
    while index < dates.len() {
        if end > 0 {
            return Ok(Some(format!("{}:{end}", dates[index])));
        }
        index += 1;
        if index >= dates.len() {
            break;
        }
        end = len_for_date(&dates[index])?;
    }
    Ok(None)
}

fn append_jsonl_line(path: &Path, line: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn migrate_legacy_logs(logs_dir: &Path) -> anyhow::Result<()> {
    let migrating = logs_dir.join(LEGACY_MIGRATING_NAME);
    let legacy = logs_dir.join(LEGACY_LOG_NAME);
    if migrating.is_file() {
        split_legacy_file(logs_dir, &migrating)?;
        remove_optional_file(&migrating_offset_path(logs_dir))?;
        fs::remove_file(&migrating)?;
    }
    if !legacy.is_file() {
        return Ok(());
    }
    match fs::rename(&legacy, &migrating) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    split_legacy_file(logs_dir, &migrating)?;
    remove_optional_file(&migrating_offset_path(logs_dir))?;
    fs::remove_file(&migrating)?;
    Ok(())
}

fn migrating_offset_path(logs_dir: &Path) -> PathBuf {
    logs_dir.join(format!("{LEGACY_MIGRATING_NAME}.offset"))
}

fn remove_optional_file(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn split_legacy_file(logs_dir: &Path, source: &Path) -> anyhow::Result<()> {
    let offset_path = migrating_offset_path(logs_dir);
    let start = match fs::read_to_string(&offset_path) {
        Ok(text) => text
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Legacy log migration offset is invalid"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    let mut file = File::open(source)?;
    let len = file.metadata()?.len();
    if start > len {
        anyhow::bail!("Legacy log migration offset is invalid");
    }
    file.seek(SeekFrom::Start(start))?;
    let mut reader = BufReader::new(file);
    while let Some(line) = read_line_capped(&mut reader, LINE_MAX_BYTES)? {
        if !line.trim().is_empty() {
            match serde_json::from_str::<Value>(&line) {
                Ok(value) => {
                    let sanitized = sanitize_value(value);
                    let date = sanitized
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .and_then(local_date_from_timestamp);
                    match date {
                        Some(date) => {
                            append_jsonl_line(
                                &dated_log_path(logs_dir, &date),
                                &serde_json::to_string(&sanitized)?,
                            )?;
                        }
                        None => append_jsonl_line(&unparsed_log_path(logs_dir), &line)?,
                    }
                }
                Err(_) => append_jsonl_line(&unparsed_log_path(logs_dir), &line)?,
            }
        }
        let pos = reader.stream_position()?;
        fs::write(&offset_path, pos.to_string())?;
    }
    Ok(())
}

fn read_line_capped<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    loop {
        let consumed = {
            let data = reader.fill_buf()?;
            if data.is_empty() {
                return Ok(if buf.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&buf).into_owned())
                });
            }
            if let Some(index) = data.iter().position(|byte| *byte == b'\n') {
                if buf.len() < max_bytes {
                    let take = (max_bytes - buf.len()).min(index);
                    buf.extend_from_slice(&data[..take]);
                }
                reader.consume(index + 1);
                if buf.ends_with(&[b'\r']) {
                    buf.pop();
                }
                return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
            }
            if buf.len() < max_bytes {
                let take = (max_bytes - buf.len()).min(data.len());
                buf.extend_from_slice(&data[..take]);
            }
            data.len()
        };
        reader.consume(consumed);
    }
}

fn read_lines_backwards(
    path: &Path,
    end_exclusive: u64,
    max_lines: usize,
) -> anyhow::Result<(Vec<CollectedLine>, u64)> {
    if max_lines == 0 {
        return Ok((Vec::new(), end_exclusive));
    }
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), 0));
        }
        Err(error) => return Err(error.into()),
    };
    let file_len = file.metadata()?.len();
    let mut pos = end_exclusive.min(file_len);
    let mut buffer = Vec::new();
    let mut lines = Vec::new();
    while lines.len() < max_lines {
        if pos == 0 && buffer.is_empty() {
            break;
        }
        if pos > 0 && !buffer.contains(&b'\n') {
            let take = (READ_CHUNK_BYTES as u64).min(pos);
            pos -= take;
            file.seek(SeekFrom::Start(pos))?;
            let mut chunk = vec![0_u8; take as usize];
            file.read_exact(&mut chunk)?;
            chunk.append(&mut buffer);
            buffer = chunk;
            if buffer.len() > LINE_MAX_BYTES && !buffer.contains(&b'\n') {
                buffer.clear();
                if pos == 0 {
                    break;
                }
                continue;
            }
        }
        if let Some(newline) = buffer.iter().rposition(|byte| *byte == b'\n') {
            let suffix = buffer.split_off(newline + 1);
            buffer.pop();
            let start = pos + newline as u64 + 1;
            push_collected_line(&mut lines, start, suffix);
            continue;
        }
        if pos == 0 && !buffer.is_empty() {
            let text = std::mem::take(&mut buffer);
            push_collected_line(&mut lines, 0, text);
        }
    }
    let next_end = lines.last().map(|line| line.start).unwrap_or(pos);
    Ok((lines, next_end))
}

fn push_collected_line(lines: &mut Vec<CollectedLine>, start: u64, bytes: Vec<u8>) {
    if bytes.is_empty() {
        return;
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.ends_with('\r') {
        text.pop();
    }
    if text.trim().is_empty() {
        return;
    }
    lines.push(CollectedLine { start, text });
}

fn sanitize_value(value: Value) -> Value {
    match value {
        Value::String(value) => Value::String(sanitize_string(value)),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, sanitize_value(value)))
                .collect(),
        ),
        other => other,
    }
}

fn sanitize_string(value: String) -> String {
    if value.len() >= 5
        && value
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    {
        if value.len() <= DATA_URL_VISIBLE_CHARS {
            return value;
        }
        return format!(
            "{}…[truncated {} chars]",
            value
                .chars()
                .take(DATA_URL_VISIBLE_CHARS)
                .collect::<String>(),
            value.chars().count()
        );
    }
    let char_count = value.chars().count();
    if char_count <= STRING_MAX_CHARS {
        return value;
    }
    format!(
        "{}…[truncated {} chars]",
        value.chars().take(STRING_MAX_CHARS).collect::<String>(),
        char_count
    )
}

fn parse_log_line(line: &str) -> LogRecord {
    if let Ok(value) = serde_json::from_str::<Value>(line) {
        let event = value
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or("event")
            .to_string();
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let detail = value.get("detail").cloned().unwrap_or(Value::Null);
        let summary = match &detail {
            Value::Object(map) => map
                .get("userPreview")
                .or_else(|| map.get("message"))
                .or_else(|| map.get("path"))
                .or_else(|| map.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            Value::String(text) => text.clone(),
            _ => String::new(),
        };
        return LogRecord {
            timestamp,
            event,
            summary,
            detail,
        };
    }
    let clipped: String = line.chars().take(SUMMARY_MAX_CHARS).collect();
    LogRecord {
        timestamp: String::new(),
        event: "log".to_string(),
        summary: clipped.clone(),
        detail: json!({ "text": clipped }),
    }
}

fn preview_at(line: &str, byte_index: usize, match_chars: usize) -> String {
    let prefix_bytes = line.get(..byte_index).unwrap_or("");
    let prefix_chars = prefix_bytes.chars().count();
    let start = prefix_chars.saturating_sub(40);
    let end = (prefix_chars + match_chars + 80).min(line.chars().count());
    let snippet: String = line
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect();
    let mut preview = String::new();
    if start > 0 {
        preview.push('…');
    }
    preview.push_str(&snippet);
    if end < line.chars().count() {
        preview.push('…');
    }
    preview.chars().take(SUMMARY_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_legacy(path: &Path, lines: &[&str]) {
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, lines.join("\n") + "\n").expect("write");
    }

    #[test]
    fn logging_appends_jsonl_records_to_local_date_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));

        logger
            .append("launcher.starting", json!({ "debugPort": 9229 }))
            .expect("append log");

        let path = logger.log_path();
        let expected_name = format!("codex-helper-{}.jsonl", local_date_today());
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(expected_name.as_str())
        );
        let contents = fs::read_to_string(&path).expect("read log");
        let line = contents.lines().next().expect("first line");
        let record: Value = serde_json::from_str(line).expect("json record");
        assert_eq!(record["event"], "launcher.starting");
        assert_eq!(record["detail"]["debugPort"], 9229);
        assert!(record["timestamp"].as_str().is_some());
    }

    #[test]
    fn logging_rejects_empty_event_names() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        let error = logger.append(" ", json!({})).unwrap_err();
        assert_eq!(error.to_string(), "Diagnostic event name is empty");
    }

    #[test]
    fn append_truncates_data_urls_before_writing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        let href = format!("data:text/html;charset=utf-8,{}", "A".repeat(5000));
        logger
            .append(
                "runtime.ready",
                json!({ "href": href, "helperInstanceId": "helper-1" }),
            )
            .expect("append");
        let contents = fs::read_to_string(logger.log_path()).expect("read");
        let record: Value = serde_json::from_str(contents.lines().next().unwrap()).expect("json");
        let stored = record["detail"]["href"].as_str().expect("href");
        assert!(stored.len() < 200, "{stored}");
        assert!(stored.contains("[truncated"));
        assert!(!stored.contains(&"A".repeat(200)));
    }

    #[test]
    fn migrate_legacy_file_splits_by_local_date_and_sanitizes() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logs_dir = temp_dir.path().join("logs");
        let first = "2026-08-28T20:00:00+00:00";
        let second = "2026-08-27T01:00:00+00:00";
        let href = format!("data:text/html,{}", "B".repeat(4000));
        write_legacy(
            &logs_dir.join(LEGACY_LOG_NAME),
            &[
                &json!({"timestamp": first, "event": "one", "detail": {"href": href, "path": "/one"}}).to_string(),
                &json!({"timestamp": second, "event": "two", "detail": {"path": "/two"}}).to_string(),
                "not-json",
            ],
        );
        let logger = DiagnosticLogger::new(logs_dir.clone());
        let page = logger.list_records(None, None, 50, None).expect("list");
        assert!(!logs_dir.join(LEGACY_LOG_NAME).exists());
        let first_date = local_date_from_timestamp(first).expect("first date");
        let second_date = local_date_from_timestamp(second).expect("second date");
        assert!(page.dates.contains(&first_date));
        assert!(page.dates.contains(&second_date));
        let migrated = fs::read_to_string(dated_log_path(&logs_dir, &first_date)).expect("dated");
        assert!(migrated.contains("[truncated"));
        assert!(!migrated.contains(&"B".repeat(200)));
        let unparsed = fs::read_to_string(unparsed_log_path(&logs_dir)).expect("unparsed");
        assert!(unparsed.contains("not-json"));
        assert!(page.dates.contains(&"unparsed".to_string()));
        assert_eq!(page.records[0].event, "one");
        assert_eq!(page.records.last().unwrap().event, "log");
    }

    #[test]
    fn migrate_legacy_resume_does_not_duplicate_records() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logs_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&logs_dir).expect("dir");
        let first = json!({"timestamp": "2026-08-28T20:00:00+00:00", "event": "one", "detail": {"path": "/one"}}).to_string();
        let second = json!({"timestamp": "2026-08-28T21:00:00+00:00", "event": "two", "detail": {"path": "/two"}}).to_string();
        write_legacy(&logs_dir.join(LEGACY_MIGRATING_NAME), &[&first, &second]);
        let first_date = local_date_from_timestamp("2026-08-28T20:00:00+00:00").expect("date");
        append_jsonl_line(&dated_log_path(&logs_dir, &first_date), &first).expect("seed");
        let offset = (first.len() + 1) as u64;
        fs::write(migrating_offset_path(&logs_dir), offset.to_string()).expect("offset");
        let logger = DiagnosticLogger::new(logs_dir.clone());
        let page = logger.list_records(None, None, 50, None).expect("list");
        assert!(!logs_dir.join(LEGACY_MIGRATING_NAME).exists());
        assert!(!migrating_offset_path(&logs_dir).exists());
        let events: Vec<_> = page
            .records
            .iter()
            .map(|record| record.event.as_str())
            .collect();
        assert_eq!(events, vec!["two", "one"]);
    }

    #[test]
    fn list_records_skips_oversized_lines() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logs_dir = temp_dir.path().join("logs");
        let logger = DiagnosticLogger::new(logs_dir.clone());
        logger
            .append("bridge.request", json!({ "path": "/ok", "status": "ok" }))
            .expect("append");
        let path = logger.log_path();
        let existing = fs::read_to_string(&path).expect("existing");
        let huge = "x".repeat(LINE_MAX_BYTES + 8);
        fs::write(&path, format!("{huge}\n{existing}")).expect("write huge");
        let page = logger.list_records(None, None, 50, None).expect("list");
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].summary, "/ok");
    }

    #[test]
    fn list_records_pages_newest_first() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        for index in 0..7 {
            logger
                .append(
                    "bridge.request",
                    json!({ "path": format!("/x{index}"), "status": "ok" }),
                )
                .expect("append");
        }
        let first = logger.list_records(None, None, 3, None).expect("page 1");
        assert_eq!(first.records.len(), 3);
        assert_eq!(first.records[0].summary, "/x6");
        assert_eq!(first.records[0].detail["path"], "/x6");
        assert_eq!(first.records[2].summary, "/x4");
        assert!(first.has_more);
        let second = logger
            .list_records(None, first.cursor.as_deref(), 3, None)
            .expect("page 2");
        assert_eq!(second.records[0].summary, "/x3");
        assert_eq!(second.records[2].summary, "/x1");
        let third = logger
            .list_records(None, second.cursor.as_deref(), 3, None)
            .expect("page 3");
        assert_eq!(third.records.len(), 1);
        assert_eq!(third.records[0].summary, "/x0");
        assert!(!third.has_more);
    }

    #[test]
    fn search_records_matches_literal_and_regex() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        logger
            .append(
                "providers.saved",
                json!({ "id": "alpha-one", "status": "ok" }),
            )
            .expect("append");
        logger
            .append(
                "providers.saved",
                json!({ "id": "beta-two", "status": "ok" }),
            )
            .expect("append");
        let literal = logger
            .search_records("alpha-one", false, None, None, 50, None)
            .expect("literal");
        assert_eq!(literal.matches.len(), 1);
        assert!(literal.matches[0].preview.contains("alpha-one"));
        assert_eq!(literal.matches[0].detail["id"], "alpha-one");
        let regex = logger
            .search_records("beta-.*", true, None, None, 50, None)
            .expect("regex");
        assert_eq!(regex.matches.len(), 1);
        assert!(regex.matches[0].preview.contains("beta-two"));
        let error = logger
            .search_records("", false, None, None, 50, None)
            .unwrap_err();
        assert_eq!(error.to_string(), "Search pattern is empty");
    }

    #[test]
    fn list_records_filters_event_and_pages() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        for index in 0..5 {
            logger
                .append("launcher.starting", json!({ "n": index }))
                .expect("append launcher");
            logger
                .append("llm.request", json!({ "n": index }))
                .expect("append llm");
        }
        let first = logger
            .list_records(None, None, 3, Some("llm.request"))
            .expect("page 1");
        assert_eq!(first.records.len(), 3);
        assert!(first
            .records
            .iter()
            .all(|record| record.event == "llm.request"));
        assert_eq!(first.records[0].detail["n"], 4);
        assert_eq!(first.records[2].detail["n"], 2);
        assert!(first.has_more);
        let second = logger
            .list_records(None, first.cursor.as_deref(), 3, Some("llm.request"))
            .expect("page 2");
        assert_eq!(second.records.len(), 2);
        assert_eq!(second.records[0].detail["n"], 1);
        assert_eq!(second.records[1].detail["n"], 0);
        assert!(!second.has_more);
        let launcher = logger
            .list_records(None, None, 50, Some("launcher."))
            .expect("launcher");
        assert_eq!(launcher.records.len(), 5);
        assert!(launcher
            .records
            .iter()
            .all(|record| record.event.starts_with("launcher.")));
    }

    #[test]
    fn search_records_respects_event_filter() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        logger
            .append("llm.request", json!({ "id": "keep-me" }))
            .expect("append llm");
        logger
            .append("bridge.request", json!({ "id": "keep-me" }))
            .expect("append bridge");
        let filtered = logger
            .search_records("keep-me", false, None, None, 50, Some("llm.request"))
            .expect("filtered");
        assert_eq!(filtered.matches.len(), 1);
        assert_eq!(filtered.matches[0].event, "llm.request");
    }

    #[test]
    fn read_latest_returns_newest_page() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = DiagnosticLogger::new(temp_dir.path().join("logs"));
        for index in 0..60 {
            logger
                .append(
                    "bridge.request",
                    json!({ "path": format!("/x{index}"), "status": "ok" }),
                )
                .expect("append");
        }
        let latest = logger.read_latest().expect("latest");
        assert_eq!(latest.records.len(), 50);
        assert_eq!(latest.records[0].summary, "/x59");
        assert!(latest.has_more);
    }
}
