//! Reads cookies from local browsers on macOS.
//!
//! The reader returns domain cookies and a per-profile note. It does not
//! decide which names make a provider session. Callers assemble headers and
//! choose the user-facing error. Undecryptable Chromium cookies are skipped
//! so one bad row does not reject the browser.

use std::path::{Path, PathBuf};
use std::process::Command;

use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use sha2::{Digest, Sha256};

const KEY_LEN: usize = 16;
const HOST_HASH_LEN: usize = 32;
/// Microseconds between 1601-01-01 (Chrome cookie epoch) and Unix epoch.
const CHROME_EPOCH_UNIX_MICROS: i64 = 11_644_473_600 * 1_000_000;
/// Seconds between 1970-01-01 and 2001-01-01 (Safari cookie epoch).
const SAFARI_EPOCH_UNIX_SECONDS: f64 = 978_307_200.0;
const IV: [u8; 16] = [0x20; 16];

type Aes128CbcDec = cbc::Decryptor<Aes128>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedCookie {
    pub name: String,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCookieBatch {
    pub browser: &'static str,
    pub profile: String,
    pub cookies: Vec<ImportedCookie>,
    pub note: String,
    pub undecryptable: Vec<String>,
}

pub struct CookieQuery<'a> {
    pub domain_suffix: &'a str,
    pub diagnostic_needles: &'a [&'a str],
}

struct ChromiumBrowser {
    name: &'static str,
    app_support_dir: &'static str,
    keychain_services: &'static [&'static str],
}

/// Safari, Chrome, Chrome Beta, Chrome Canary, Firefox, Edge, then Arc.
/// A missing browser is a note, not a failure of the browsers that follow.
pub fn read_browser_cookie_batches(query: &CookieQuery<'_>) -> Vec<BrowserCookieBatch> {
    let Some(home) = dirs::home_dir() else {
        return vec![BrowserCookieBatch {
            browser: "browser",
            profile: String::new(),
            cookies: Vec::new(),
            note: "Could not locate the home directory".to_string(),
            undecryptable: Vec::new(),
        }];
    };
    let support = home.join("Library/Application Support");
    let mut batches = Vec::new();
    batches.extend(read_safari(&home, query));
    batches.extend(read_chromium(
        &ChromiumBrowser {
            name: "Chrome",
            app_support_dir: "Google/Chrome",
            keychain_services: &["Chrome Safe Storage"],
        },
        &support,
        query,
    ));
    batches.extend(read_chromium(
        &ChromiumBrowser {
            name: "Chrome Beta",
            app_support_dir: "Google/Chrome Beta",
            keychain_services: &["Chrome Safe Storage", "Chrome Beta Safe Storage"],
        },
        &support,
        query,
    ));
    batches.extend(read_chromium(
        &ChromiumBrowser {
            name: "Chrome Canary",
            app_support_dir: "Google/Chrome Canary",
            keychain_services: &["Chrome Safe Storage", "Chrome Canary Safe Storage"],
        },
        &support,
        query,
    ));
    batches.extend(read_firefox(&support, query));
    batches.extend(read_chromium(
        &ChromiumBrowser {
            name: "Microsoft Edge",
            app_support_dir: "Microsoft Edge",
            keychain_services: &["Microsoft Edge Safe Storage"],
        },
        &support,
        query,
    ));
    batches.extend(read_chromium(
        &ChromiumBrowser {
            name: "Arc",
            app_support_dir: "Arc/User Data",
            keychain_services: &["Arc Safe Storage"],
        },
        &support,
        query,
    ));
    batches
}

pub fn parse_cookie_header(raw: &str) -> Result<Vec<ImportedCookie>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Cookie header is empty".to_string());
    }
    let body = trimmed
        .strip_prefix("Cookie:")
        .or_else(|| trimmed.strip_prefix("cookie:"))
        .unwrap_or(trimmed)
        .trim();
    if body.is_empty() {
        return Err("Cookie header is empty".to_string());
    }
    let mut cookies: Vec<ImportedCookie> = Vec::new();
    for part in body.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((name, value)) = part.split_once('=') else {
            return Err("Cookie header has an entry without a name".to_string());
        };
        let name = name.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("cookie") {
            return Err("Cookie header has an entry without a name".to_string());
        }
        let value = value.trim().as_bytes();
        if value.is_empty() || !is_valid_cookie_value(value) {
            return Err(format!(
                "cookie \"{name}\" is empty or contains invalid control bytes"
            ));
        }
        if let Some(existing) = cookies.iter_mut().find(|cookie| cookie.name == name) {
            existing.value = value.to_vec();
        } else {
            cookies.push(ImportedCookie {
                name: name.to_string(),
                value: value.to_vec(),
            });
        }
    }
    if cookies.is_empty() {
        return Err("Cookie header is empty".to_string());
    }
    Ok(cookies)
}

pub fn cookie_header_bytes(cookies: &[ImportedCookie]) -> Result<Vec<u8>, String> {
    if cookies.is_empty() {
        return Err("Cookie header is empty".to_string());
    }
    let mut header = Vec::new();
    for cookie in cookies {
        if cookie.name.is_empty()
            || cookie.value.is_empty()
            || !is_valid_cookie_value(&cookie.value)
        {
            return Err(format!(
                "cookie \"{}\" is empty or contains invalid control bytes",
                cookie.name
            ));
        }
        if !header.is_empty() {
            header.extend_from_slice(b"; ");
        }
        header.extend_from_slice(cookie.name.as_bytes());
        header.push(b'=');
        header.extend_from_slice(&cookie.value);
    }
    Ok(header)
}

fn read_safari(home: &Path, query: &CookieQuery<'_>) -> Vec<BrowserCookieBatch> {
    let mut files = Vec::new();
    let mut blocked = Vec::new();
    for relative in [
        "Library/Cookies/Cookies.binarycookies",
        "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
    ] {
        let path = home.join(relative);
        match path.metadata() {
            Ok(metadata) if metadata.is_file() => files.push(path),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                blocked.push(path);
            }
            Err(_) => {}
        }
    }
    for root in [
        home.join("Library/Containers/com.apple.Safari/Data/Library/WebKit/WebsiteDataStore"),
        home.join("Library/WebKit/WebsiteDataStore"),
    ] {
        collect_named_files(
            &root,
            "Cookies.binarycookies",
            5,
            32,
            &mut files,
            &mut blocked,
        );
    }
    if files.is_empty() && blocked.is_empty() {
        return vec![note_batch("Safari", "", "no cookie file")];
    }
    let mut batches = Vec::new();
    for path in blocked {
        batches.push(note_batch(
            "Safari",
            &profile_label(&path),
            &format!("macOS blocked access to {}", path.display()),
        ));
    }
    let now = unix_time_seconds();
    for path in files {
        let profile = profile_label(&path);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                batches.push(note_batch(
                    "Safari",
                    &profile,
                    &format!("macOS blocked access to {}", path.display()),
                ));
                continue;
            }
            Err(error) => {
                batches.push(note_batch(
                    "Safari",
                    &profile,
                    &format!("failed to read cookie file: {error}"),
                ));
                continue;
            }
        };
        match parse_safari_cookies(&bytes, query, now) {
            Ok(read) => batches.push(batch_from_read("Safari", profile, read)),
            Err(error) => batches.push(note_batch("Safari", &profile, &error)),
        }
    }
    batches
}

fn read_firefox(support: &Path, query: &CookieQuery<'_>) -> Vec<BrowserCookieBatch> {
    let root = support.join("Firefox");
    if !root.exists() {
        return vec![note_batch("Firefox", "", "not installed")];
    }
    let databases = firefox_cookie_databases(&root);
    if databases.is_empty() {
        return vec![note_batch("Firefox", "", "no cookie database")];
    }
    databases
        .into_iter()
        .map(|path| {
            let profile = path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("profile")
                .to_string();
            match read_firefox_database(&path, query) {
                Ok(read) => batch_from_read("Firefox", profile, read),
                Err(error) => note_batch("Firefox", &profile, &error),
            }
        })
        .collect()
}

fn read_chromium(
    browser: &ChromiumBrowser,
    support: &Path,
    query: &CookieQuery<'_>,
) -> Vec<BrowserCookieBatch> {
    let user_data = support.join(browser.app_support_dir);
    if !user_data.is_dir() {
        return vec![note_batch(browser.name, "", "not installed")];
    }
    let databases = match find_cookie_databases(&user_data) {
        Ok(databases) if !databases.is_empty() => databases,
        Ok(_) => return vec![note_batch(browser.name, "", "no cookie database")],
        Err(error) => return vec![note_batch(browser.name, "", &error)],
    };
    let secret = match keychain_secret(browser.keychain_services) {
        Ok(secret) => secret,
        Err(error) => return vec![note_batch(browser.name, "", &error)],
    };
    let keys = candidate_keys(&secret);
    databases
        .into_iter()
        .map(|path| {
            let profile = chromium_profile_label(&path);
            match read_chromium_database(&path, &keys, query) {
                Ok(read) => batch_from_read(browser.name, profile, read),
                Err(error) => note_batch(browser.name, &profile, &error),
            }
        })
        .collect()
}

struct ProfileRead {
    cookies: Vec<ImportedCookie>,
    note: String,
    undecryptable: Vec<String>,
}

fn batch_from_read(
    browser: &'static str,
    profile: String,
    read: ProfileRead,
) -> BrowserCookieBatch {
    BrowserCookieBatch {
        browser,
        profile,
        cookies: read.cookies,
        note: read.note,
        undecryptable: read.undecryptable,
    }
}

fn note_batch(browser: &'static str, profile: &str, note: &str) -> BrowserCookieBatch {
    BrowserCookieBatch {
        browser,
        profile: profile.to_string(),
        cookies: Vec::new(),
        note: note.to_string(),
        undecryptable: Vec::new(),
    }
}

fn read_chromium_database(
    db_path: &Path,
    keys: &[[u8; KEY_LEN]],
    query: &CookieQuery<'_>,
) -> Result<ProfileRead, String> {
    let temp = tempfile::Builder::new()
        .prefix("codex-helper-cookies-")
        .tempdir()
        .map_err(|error| format!("failed to create temp dir: {error}"))?;
    let copy_path = temp.path().join("Cookies");
    copy_database(db_path, &copy_path)?;
    let connection = rusqlite::Connection::open_with_flags(
        &copy_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("failed to open cookies db: {error}"))?;
    let db_version = cookie_db_version(&connection);
    let mut statement = connection
        .prepare(
            "SELECT host_key, name, encrypted_value, expires_utc, is_persistent
             FROM cookies",
        )
        .map_err(|error| format!("failed to query cookies db: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| format!("failed to query cookies db: {error}"))?;
    let now = unix_time_micros();
    let mut cookies = Vec::new();
    let mut undecryptable = Vec::new();
    let mut matched = false;
    for row in rows {
        let (host, name, encrypted, expires_utc, is_persistent) =
            row.map_err(|error| format!("failed to read cookie row: {error}"))?;
        if name.is_empty() || !host_matches(&host, query.domain_suffix) {
            continue;
        }
        matched = true;
        if cookie_is_expired_at(is_persistent, expires_utc, now) {
            continue;
        }
        if encrypted.is_empty() {
            undecryptable.push(name);
            continue;
        }
        match decrypt_cookie_value(&encrypted, &host, db_version, keys) {
            Ok(value) => cookies.push(ImportedCookie { name, value }),
            Err(_) => undecryptable.push(name),
        }
    }
    let note = if matched {
        if undecryptable.is_empty() {
            String::new()
        } else {
            format!(
                "skipped undecryptable cookies: {}",
                undecryptable.join(", ")
            )
        }
    } else {
        diagnose_cookie_db(&connection, "host_key", query).unwrap_or_else(|error| error)
    };
    Ok(ProfileRead {
        cookies,
        note,
        undecryptable,
    })
}

fn read_firefox_database(db_path: &Path, query: &CookieQuery<'_>) -> Result<ProfileRead, String> {
    let temp = tempfile::Builder::new()
        .prefix("codex-helper-cookies-")
        .tempdir()
        .map_err(|error| format!("failed to create temp dir: {error}"))?;
    let copy_path = temp.path().join("cookies.sqlite");
    copy_database(db_path, &copy_path)?;
    let connection = rusqlite::Connection::open_with_flags(
        &copy_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("failed to open cookies db: {error}"))?;
    let has_origin = sqlite_column_exists(&connection, "moz_cookies", "originAttributes");
    let sql = if has_origin {
        "SELECT host, name, value, expiry, originAttributes FROM moz_cookies"
    } else {
        "SELECT host, name, value, expiry, '' FROM moz_cookies"
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("failed to query cookies db: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| format!("failed to query cookies db: {error}"))?;
    let now = unix_time_seconds();
    let mut cookies = Vec::new();
    let mut matched = false;
    for row in rows {
        let (host, name, value, expiry, origin) =
            row.map_err(|error| format!("failed to read cookie row: {error}"))?;
        if name.is_empty() || !host_matches(&host, query.domain_suffix) {
            continue;
        }
        matched = true;
        if origin.contains("partitionKey") {
            continue;
        }
        if expiry > 0 && expiry < now {
            continue;
        }
        let value = value.into_bytes();
        if value.is_empty() || !is_valid_cookie_value(&value) {
            continue;
        }
        cookies.push(ImportedCookie { name, value });
    }
    let note = if matched {
        String::new()
    } else {
        diagnose_cookie_db(&connection, "host", query).unwrap_or_else(|error| error)
    };
    Ok(ProfileRead {
        cookies,
        note,
        undecryptable: Vec::new(),
    })
}

fn parse_safari_cookies(
    bytes: &[u8],
    query: &CookieQuery<'_>,
    now_unix: i64,
) -> Result<ProfileRead, String> {
    let records = parse_binary_cookies(bytes)?;
    let mut cookies = Vec::new();
    let mut related = Vec::new();
    let mut total = 0i64;
    for record in records {
        total += 1;
        if host_is_related(&record.host, query.diagnostic_needles)
            && !related.iter().any(|host: &String| host == &record.host)
        {
            related.push(record.host.clone());
        }
        if !host_matches(&record.host, query.domain_suffix) {
            continue;
        }
        if safari_cookie_expired(record.expires, now_unix) {
            continue;
        }
        if record.name.is_empty()
            || record.value.is_empty()
            || !is_valid_cookie_value(record.value.as_bytes())
        {
            continue;
        }
        cookies.push(ImportedCookie {
            name: record.name,
            value: record.value.into_bytes(),
        });
    }
    let note = if cookies.is_empty() {
        diagnosis_text(total, &related, query.domain_suffix)
    } else {
        String::new()
    };
    Ok(ProfileRead {
        cookies,
        note,
        undecryptable: Vec::new(),
    })
}

struct SafariCookie {
    host: String,
    name: String,
    value: String,
    expires: f64,
}

fn parse_binary_cookies(bytes: &[u8]) -> Result<Vec<SafariCookie>, String> {
    if bytes.len() < 8 || &bytes[..4] != b"cook" {
        return Err("Safari cookie file is not a binarycookies file".to_string());
    }
    let page_count = read_u32_be(bytes, 4)? as usize;
    let sizes_end = 8 + page_count
        .checked_mul(4)
        .ok_or("Safari cookie file is invalid")?;
    if page_count > 10_000 || sizes_end > bytes.len() {
        return Err("Safari cookie file is invalid".to_string());
    }
    let mut offset = sizes_end;
    let mut records = Vec::new();
    for index in 0..page_count {
        let size = read_u32_be(bytes, 8 + index * 4)? as usize;
        if size < 8
            || offset
                .checked_add(size)
                .map(|end| end > bytes.len())
                .unwrap_or(true)
        {
            return Err("Safari cookie file is invalid".to_string());
        }
        records.extend(parse_safari_page(&bytes[offset..offset + size])?);
        offset += size;
    }
    Ok(records)
}

fn parse_safari_page(page: &[u8]) -> Result<Vec<SafariCookie>, String> {
    if page.len() < 8 {
        return Err("Safari cookie file is invalid".to_string());
    }
    let count = read_u32_le(page, 4)? as usize;
    if count > 100_000 || 8 + count * 4 > page.len() {
        return Err("Safari cookie file is invalid".to_string());
    }
    let mut records = Vec::new();
    for index in 0..count {
        let cookie_offset = read_u32_le(page, 8 + index * 4)? as usize;
        if let Some(record) = parse_safari_record(page, cookie_offset) {
            records.push(record);
        }
    }
    Ok(records)
}

fn parse_safari_record(page: &[u8], offset: usize) -> Option<SafariCookie> {
    if offset.checked_add(56)? > page.len() {
        return None;
    }
    let size = read_u32_le(page, offset).ok()? as usize;
    if size < 56 || offset.checked_add(size)? > page.len() {
        return None;
    }
    let limit = offset + size;
    let host = read_c_string(
        page,
        offset,
        read_u32_le(page, offset + 16).ok()? as usize,
        limit,
    )?;
    let name = read_c_string(
        page,
        offset,
        read_u32_le(page, offset + 20).ok()? as usize,
        limit,
    )?;
    let value = read_c_string(
        page,
        offset,
        read_u32_le(page, offset + 28).ok()? as usize,
        limit,
    )?;
    let expires = read_f64_le(page, offset + 40).ok()?;
    if host.is_empty() || name.is_empty() {
        return None;
    }
    Some(SafariCookie {
        host,
        name,
        value,
        expires,
    })
}

fn read_c_string(bytes: &[u8], base: usize, relative: usize, limit: usize) -> Option<String> {
    let start = base.checked_add(relative)?;
    if start >= limit || limit > bytes.len() {
        return None;
    }
    let end = bytes[start..limit]
        .iter()
        .position(|byte| *byte == 0)
        .map(|index| start + index)
        .unwrap_or(limit);
    if end <= start {
        return None;
    }
    String::from_utf8(bytes[start..end].to_vec()).ok()
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or("Safari cookie file is invalid")?;
    Ok(u32::from_be_bytes(slice.try_into().unwrap()))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or("Safari cookie file is invalid")?;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn read_f64_le(bytes: &[u8], offset: usize) -> Result<f64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or("Safari cookie file is invalid")?;
    Ok(f64::from_le_bytes(slice.try_into().unwrap()))
}

fn firefox_cookie_databases(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("Profiles")) {
        for entry in entries.flatten() {
            let path = entry.path().join("cookies.sqlite");
            if path.is_file() {
                found.push(path);
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("profiles.ini")) {
        let mut relative = true;
        let mut path: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                if let Some(profile_path) = path.take() {
                    push_firefox_profile(&mut found, root, &profile_path, relative);
                }
                relative = true;
                continue;
            }
            if let Some(value) = line.strip_prefix("IsRelative=") {
                relative = value.trim() != "0";
            } else if let Some(value) = line.strip_prefix("Path=") {
                path = Some(value.trim().to_string());
            }
        }
        if let Some(profile_path) = path {
            push_firefox_profile(&mut found, root, &profile_path, relative);
        }
    }
    found.sort();
    found.dedup();
    found
}

fn push_firefox_profile(found: &mut Vec<PathBuf>, root: &Path, profile_path: &str, relative: bool) {
    if profile_path.is_empty() {
        return;
    }
    let profile = if relative {
        root.join(profile_path)
    } else {
        PathBuf::from(profile_path)
    };
    let database = profile.join("cookies.sqlite");
    if database.is_file() {
        found.push(database);
    }
}

fn find_cookie_databases(user_data_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(user_data_dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            format!("macOS blocked access to {}", user_data_dir.display())
        } else {
            format!("failed to list {}: {error}", user_data_dir.display())
        }
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        for candidate in [path.join("Network/Cookies"), path.join("Cookies")] {
            if candidate.is_file() {
                found.push(candidate);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn collect_named_files(
    root: &Path,
    name: &str,
    depth: usize,
    limit: usize,
    found: &mut Vec<PathBuf>,
    blocked: &mut Vec<PathBuf>,
) {
    if depth == 0 || found.len() >= limit || !root.exists() {
        return;
    }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            blocked.push(root.to_path_buf());
            return;
        }
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if found.len() >= limit {
            return;
        }
        let path = entry.path();
        if path.file_name().and_then(|item| item.to_str()) == Some(name) && path.is_file() {
            if !found.contains(&path) {
                found.push(path);
            }
            continue;
        }
        if path.is_dir() && !path.is_symlink() {
            collect_named_files(&path, name, depth - 1, limit, found, blocked);
        }
    }
}

fn copy_database(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::copy(source, destination).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            format!("macOS blocked access to {}", source.display())
        } else {
            format!("failed to copy cookies db: {error}")
        }
    })?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", source.display()));
        if sidecar.is_file() {
            let _ = std::fs::copy(
                &sidecar,
                destination.with_file_name(format!(
                    "{}{suffix}",
                    destination
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Cookies")
                )),
            );
        }
    }
    Ok(())
}

fn keychain_secret(services: &[&str]) -> Result<Vec<u8>, String> {
    let mut last = String::from("Keychain item not found");
    for service in services {
        match keychain_secret_one(service) {
            Ok(secret) => return Ok(secret),
            Err(error) if error.contains("not found") => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

fn keychain_secret_one(service: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()
        .map_err(|error| format!("failed to run security find-generic-password: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("could not be found") || stderr.contains("SecKeychainSearch") {
            return Err(format!("Keychain item \"{service}\" not found"));
        }
        return Err(format!(
            "Keychain rejected access to \"{service}\"{}",
            if stderr.is_empty() {
                String::new()
            } else {
                format!(" ({stderr})")
            }
        ));
    }
    let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if secret.is_empty() {
        return Err(format!("Keychain item \"{service}\" is empty"));
    }
    Ok(secret.into_bytes())
}

fn candidate_keys(secret: &[u8]) -> Vec<[u8; KEY_LEN]> {
    let mut keys = Vec::new();
    let mut pbkdf2_key = [0u8; KEY_LEN];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(secret, b"saltysalt", 1003, &mut pbkdf2_key);
    keys.push(pbkdf2_key);
    if secret.len() == KEY_LEN * 2 && secret.iter().all(u8::is_ascii_hexdigit) {
        if let Ok(raw) = hex_decode(secret) {
            let mut key = [0u8; KEY_LEN];
            key.copy_from_slice(&raw);
            keys.push(key);
        }
    }
    keys
}

fn hex_decode(input: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(input).map_err(|_| "secret is not UTF-8".to_string())?;
    (0..text.len() / 2)
        .map(|index| {
            u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                .map_err(|error| format!("invalid hex secret: {error}"))
        })
        .collect()
}

fn cookie_db_version(connection: &rusqlite::Connection) -> Option<i64> {
    connection
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'version'",
            [],
            |row| row.get(0),
        )
        .ok()
}

fn sqlite_column_exists(connection: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let sql = format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1");
    connection.query_row(&sql, [column], |_| Ok(())).is_ok()
}

fn diagnose_cookie_db(
    connection: &rusqlite::Connection,
    host_column: &str,
    query: &CookieQuery<'_>,
) -> Result<String, String> {
    let total: i64 = connection
        .query_row("SELECT COUNT(*) FROM cookies", [], |row| row.get(0))
        .or_else(|_| connection.query_row("SELECT COUNT(*) FROM moz_cookies", [], |row| row.get(0)))
        .map_err(|error| format!("failed to count cookies: {error}"))?;
    let sql = format!(
        "SELECT DISTINCT {host_column} FROM {} ORDER BY {host_column}",
        if host_column == "host" {
            "moz_cookies"
        } else {
            "cookies"
        }
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("failed to list related hosts: {error}"))?;
    let hosts = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to list related hosts: {error}"))?
        .filter_map(Result::ok)
        .filter(|host| host_is_related(host, query.diagnostic_needles))
        .collect::<Vec<_>>();
    Ok(diagnosis_text(total, &hosts, query.domain_suffix))
}

fn diagnosis_text(total: i64, hosts: &[String], suffix: &str) -> String {
    if hosts.is_empty() {
        format!("{total} cookies stored, none for {suffix}")
    } else {
        format!(
            "{total} cookies stored, related hosts: {}",
            hosts
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn host_matches(host: &str, suffix: &str) -> bool {
    let host = host.trim_start_matches('.').to_ascii_lowercase();
    let suffix = suffix.trim_start_matches('.').to_ascii_lowercase();
    !suffix.is_empty() && (host == suffix || host.ends_with(&format!(".{suffix}")))
}

fn host_is_related(host: &str, needles: &[&str]) -> bool {
    let host = host.to_ascii_lowercase();
    needles.iter().any(|needle| {
        let needle = needle.trim().to_ascii_lowercase();
        !needle.is_empty() && host.contains(&needle)
    })
}

fn unix_time_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_micros()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn unix_time_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn cookie_is_expired_at(is_persistent: i64, expires_utc: i64, now_unix_micros: i64) -> bool {
    if is_persistent == 0 || expires_utc <= 0 {
        return false;
    }
    expires_utc < now_unix_micros.saturating_add(CHROME_EPOCH_UNIX_MICROS)
}

fn safari_cookie_expired(expires: f64, now_unix: i64) -> bool {
    expires > 0.0 && expires + SAFARI_EPOCH_UNIX_SECONDS < now_unix as f64
}

fn chromium_profile_label(path: &Path) -> String {
    let mut cursor = path.parent();
    if path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("Network")
    {
        cursor = path.parent().and_then(|parent| parent.parent());
    }
    cursor
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("profile")
        .to_string()
}

fn profile_label(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("profile")
        .to_string()
}

fn is_valid_cookie_value(value: &[u8]) -> bool {
    value
        .iter()
        .all(|byte| matches!(byte, 0x09 | 0x20..=0x7e | 0x80..=0xff))
}

fn decrypt_cookie_value(
    encrypted: &[u8],
    host_key: &str,
    db_version: Option<i64>,
    keys: &[[u8; KEY_LEN]],
) -> Result<Vec<u8>, String> {
    let (version, ciphertext) = match encrypted.first() {
        Some(b'v') if encrypted.len() > 3 => (encrypted[..3].to_vec(), &encrypted[3..]),
        _ => return Err("unexpected cookie encryption format".to_string()),
    };
    if version != b"v10" && version != b"v11" {
        return Err(format!(
            "unsupported cookie encryption version {}",
            String::from_utf8_lossy(&version)
        ));
    }
    let mut last_error = String::from("no key could decrypt this cookie");
    for key in keys {
        match Aes128CbcDec::new(key.into(), &IV.into()).decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        {
            Ok(plain) => match cookie_plaintext(&plain, host_key, db_version) {
                Ok(value) => return Ok(value),
                Err(error) => last_error = error,
            },
            Err(error) => last_error = format!("decryption failed: {error}"),
        }
    }
    Err(last_error)
}

fn cookie_plaintext(
    plain: &[u8],
    host_key: &str,
    db_version: Option<i64>,
) -> Result<Vec<u8>, String> {
    let digest = Sha256::digest(host_key.as_bytes());
    let hash_matches =
        plain.len() >= HOST_HASH_LEN && plain[..HOST_HASH_LEN] == digest.as_slice()[..];
    if hash_matches {
        let value = &plain[HOST_HASH_LEN..];
        if value.is_empty() || !is_valid_cookie_value(value) {
            return Err(
                "decrypted cookie was empty or contained invalid control bytes after removing SHA256(host_key)"
                    .to_string(),
            );
        }
        return Ok(value.to_vec());
    }
    if db_version.is_some_and(|version| version >= 24) {
        return Err("decrypted cookie did not start with SHA256(host_key)".to_string());
    }
    if plain.is_empty() || !is_valid_cookie_value(plain) {
        return Err("decrypted value was empty or contained invalid control bytes".to_string());
    }
    Ok(plain.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_keys_accept_pbkdf2_and_hex_secrets() {
        let secret = b"peanuts";
        let keys = candidate_keys(secret);
        assert!(!keys.is_empty());
        let hex_secret = b"0123456789abcdef0123456789abcdef";
        let keys = candidate_keys(hex_secret);
        assert_eq!(keys.len(), 2);
        let mut expected = [0u8; 16];
        expected.copy_from_slice(&hex_decode(hex_secret).unwrap());
        assert!(keys.contains(&expected));
    }

    #[test]
    fn rejects_unknown_encryption_version() {
        let keys = candidate_keys(b"peanuts");
        let error =
            decrypt_cookie_value(b"v20-not-really", "example.com", Some(24), &keys).unwrap_err();
        assert!(error.contains("v20"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_short_or_plain_values() {
        let keys = candidate_keys(b"peanuts");
        assert!(decrypt_cookie_value(b"plain", "example.com", Some(24), &keys).is_err());
    }

    #[test]
    fn strips_verified_host_hash_prefix() {
        let keys = candidate_keys(b"peanuts");
        let host = ".platform.xiaomimimo.com";
        let value = b"session-token";
        let encrypted = encrypt_cookie(&keys[0], &with_host_hash(host, value));
        let decrypted = decrypt_cookie_value(&encrypted, host, Some(24), &keys).unwrap();
        assert_eq!(decrypted, value);
    }

    #[test]
    fn keeps_legacy_plaintext_without_hash() {
        let keys = candidate_keys(b"peanuts");
        let host = ".platform.xiaomimimo.com";
        let value = b"legacy-cookie-value-longer-than-thirty-two-bytes";
        let encrypted = encrypt_cookie(&keys[0], value);
        let decrypted = decrypt_cookie_value(&encrypted, host, Some(23), &keys).unwrap();
        assert_eq!(decrypted, value);
    }

    #[test]
    fn rejects_mismatched_host_hash_on_modern_db() {
        let keys = candidate_keys(b"peanuts");
        let host = ".platform.xiaomimimo.com";
        let encrypted = encrypt_cookie(
            &keys[0],
            &with_host_hash("wrong.example", b"api-platform-session"),
        );
        let error = decrypt_cookie_value(&encrypted, host, Some(24), &keys).unwrap_err();
        assert!(
            error.contains("did not start with SHA256(host_key)"),
            "{error}"
        );
        assert!(!error.contains("api-platform-session"), "{error}");
    }

    #[test]
    fn does_not_slice_unverified_prefix_on_legacy_db() {
        let keys = candidate_keys(b"peanuts");
        let host = ".platform.xiaomimimo.com";
        let encrypted = encrypt_cookie(
            &keys[0],
            &with_host_hash("wrong.example", b"api-platform-session"),
        );
        let error = decrypt_cookie_value(&encrypted, host, Some(23), &keys).unwrap_err();
        assert!(error.contains("invalid control bytes"), "{error}");
        assert!(
            !error.contains("after removing SHA256(host_key)"),
            "{error}"
        );
    }

    #[test]
    fn modern_profile_skips_bad_cookie_and_keeps_valid_session() {
        let keys = candidate_keys(b"peanuts");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cookies");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO meta (key, value) VALUES ('version', '24');
                 CREATE TABLE cookies (
                   host_key TEXT, name TEXT, encrypted_value BLOB,
                   expires_utc INTEGER, is_persistent INTEGER
                 );",
            )
            .unwrap();
        let host = ".platform.xiaomimimo.com";
        let good = encrypt_cookie(&keys[0], &with_host_hash(host, b"session-token"));
        let bad = encrypt_cookie(&keys[0], &with_host_hash("wrong.example", b"secret-value"));
        connection
            .execute(
                "INSERT INTO cookies VALUES (?1, 'api-platform_serviceToken', ?2, 0, 0)",
                rusqlite::params![host, good],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO cookies VALUES (?1, 'api-platform_ph', ?2, 0, 0)",
                rusqlite::params![host, bad],
            )
            .unwrap();
        drop(connection);
        let read = read_chromium_database(
            &path,
            &keys,
            &CookieQuery {
                domain_suffix: "xiaomimimo.com",
                diagnostic_needles: &["xiaomi", "mimo"],
            },
        )
        .unwrap();
        assert_eq!(read.cookies.len(), 1);
        assert_eq!(read.cookies[0].name, "api-platform_serviceToken");
        assert_eq!(read.cookies[0].value, b"session-token");
        assert_eq!(read.undecryptable, vec!["api-platform_ph".to_string()]);
        assert!(!read.note.contains("secret-value"), "{}", read.note);
    }

    #[test]
    fn manual_header_parser_requires_named_values_and_hides_them() {
        let cookies = parse_cookie_header(
            "Cookie: api-platform_serviceToken=session-token; userId=42; api-platform_ph=optional",
        )
        .unwrap();
        assert_eq!(cookies.len(), 3);
        let header = cookie_header_bytes(&cookies).unwrap();
        assert!(header
            .windows(b"session-token".len())
            .any(|window| window == b"session-token"));
        let error = parse_cookie_header("Cookie: api-platform_serviceToken=").unwrap_err();
        assert!(error.contains("api-platform_serviceToken"), "{error}");
        assert!(!error.contains("session"), "{error}");
    }

    #[test]
    fn safari_parser_reads_domain_cookie_and_skips_unrelated_values() {
        let bytes = sample_binary_cookies(".platform.xiaomimimo.com", "userId", "42");
        let read = parse_safari_cookies(
            &bytes,
            &CookieQuery {
                domain_suffix: "xiaomimimo.com",
                diagnostic_needles: &["xiaomi"],
            },
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(read.cookies.len(), 1);
        assert_eq!(read.cookies[0].value, b"42");
        let missed = parse_safari_cookies(
            &bytes,
            &CookieQuery {
                domain_suffix: "example.com",
                diagnostic_needles: &["xiaomi"],
            },
            1_700_000_000,
        )
        .unwrap();
        assert!(missed.cookies.is_empty());
        assert!(
            missed.note.contains(".platform.xiaomimimo.com"),
            "{}",
            missed.note
        );
        assert!(!missed.note.contains("=42"), "{}", missed.note);
    }

    #[test]
    fn host_match_rejects_lookalike_suffix() {
        assert!(host_matches(".platform.xiaomimimo.com", "xiaomimimo.com"));
        assert!(!host_matches(
            "notxiaomimimo.com.evil.com",
            "xiaomimimo.com"
        ));
    }

    #[test]
    fn expired_session_cookie_is_not_live() {
        let now = 1_700_000_000_000_000;
        let expired = now + CHROME_EPOCH_UNIX_MICROS - 1;
        let fresh = now + CHROME_EPOCH_UNIX_MICROS + 1_000;
        assert!(cookie_is_expired_at(1, expired, now));
        assert!(!cookie_is_expired_at(1, fresh, now));
        assert!(!cookie_is_expired_at(0, expired, now));
    }

    fn encrypt_cookie(key: &[u8; KEY_LEN], plain: &[u8]) -> Vec<u8> {
        use cbc::cipher::BlockEncryptMut;
        let mut packet = b"v10".to_vec();
        packet.extend(
            cbc::Encryptor::<Aes128>::new(key.into(), &IV.into())
                .encrypt_padded_vec_mut::<Pkcs7>(plain),
        );
        packet
    }

    fn with_host_hash(host_key: &str, value: &[u8]) -> Vec<u8> {
        let mut plain = Sha256::digest(host_key.as_bytes()).to_vec();
        plain.extend_from_slice(value);
        plain
    }

    fn sample_binary_cookies(host: &str, name: &str, value: &str) -> Vec<u8> {
        let record = vec![0u8; 56];
        let host_at = 56;
        let name_at = host_at + host.len() + 1;
        let value_at = name_at + name.len() + 1;
        let mut body = record;
        body.extend(host.as_bytes());
        body.push(0);
        body.extend(name.as_bytes());
        body.push(0);
        body.extend(value.as_bytes());
        body.push(0);
        let size = body.len() as u32;
        body[..4].copy_from_slice(&size.to_le_bytes());
        body[16..20].copy_from_slice(&(host_at as u32).to_le_bytes());
        body[20..24].copy_from_slice(&(name_at as u32).to_le_bytes());
        body[24..28].copy_from_slice(&56u32.to_le_bytes());
        body[28..32].copy_from_slice(&(value_at as u32).to_le_bytes());
        let mut page = Vec::new();
        page.extend_from_slice(&0x00000100u32.to_le_bytes());
        page.extend_from_slice(&1u32.to_le_bytes());
        page.extend_from_slice(&12u32.to_le_bytes());
        page.extend(&body);
        let mut file = b"cook".to_vec();
        file.extend_from_slice(&1u32.to_be_bytes());
        file.extend_from_slice(&(page.len() as u32).to_be_bytes());
        file.extend(page);
        file
    }
}
