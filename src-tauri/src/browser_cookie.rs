//! Reads cookies from local Chromium-family browsers (Chrome, Edge, Arc) on
//! macOS so MiMo usage queries can authenticate exactly like the web console.
//!
//! Flow per browser profile:
//! 1. Copy the profile's SQLite `Cookies` database (plus WAL/SHM) to a
//!    temporary directory to avoid lock contention with the running browser.
//! 2. Read `host_key` / `name` / `encrypted_value` rows for
//!    `xiaomimimo.com` hosts. `host_key` is required: Chrome cookie DB
//!    version >= 24 prefixes the plaintext with `SHA256(host_key)`.
//! 3. Fetch the browser's "Safe Storage" secret from the macOS Keychain and
//!    derive the AES-128 key (PBKDF2-HMAC-SHA1, salt `saltysalt`, 1003
//!    iterations; a 32-char hex secret is used directly as the raw key).
//! 4. Decrypt `v10`/`v11` cookies (AES-128-CBC, IV = 16 spaces, PKCS7),
//!    drop the host hash only when it matches that row's `host_key`, and
//!    assemble a `Cookie:` header value.
//!
//! Every failure path produces an explicit error; nothing falls back
//! silently.

use std::path::{Path, PathBuf};
use std::process::Command;

use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use sha2::{Digest, Sha256};

const MIMO_COOKIE_HOST: &str = "xiaomimimo.com";
const PLATFORM_SESSION_COOKIE: &str = "api-platform_serviceToken";
const PLATFORM_CONSOLE_URL: &str = "https://platform.xiaomimimo.com/#/console/balance";
const KEY_LEN: usize = 16;
const HOST_HASH_LEN: usize = 32;
/// Microseconds between 1601-01-01 (Chrome cookie epoch) and Unix epoch.
const CHROME_EPOCH_UNIX_MICROS: i64 = 11_644_473_600 * 1_000_000;
const IV: [u8; 16] = [0x20; 16];

type Aes128CbcDec = cbc::Decryptor<Aes128>;

struct BrowserSpec {
    /// Human-readable name used in error messages.
    name: &'static str,
    /// Directory under `~/Library/Application Support` that holds the
    /// Chromium user-data dir (profile directories live directly inside).
    app_support_dir: &'static str,
    /// macOS Keychain generic-password service for the Safe Storage secret.
    keychain_service: &'static str,
}

const BROWSERS: &[BrowserSpec] = &[
    BrowserSpec {
        name: "Chrome",
        app_support_dir: "Google/Chrome",
        keychain_service: "Chrome Safe Storage",
    },
    BrowserSpec {
        name: "Microsoft Edge",
        app_support_dir: "Microsoft Edge",
        keychain_service: "Microsoft Edge Safe Storage",
    },
    BrowserSpec {
        name: "Arc",
        app_support_dir: "Arc/User Data",
        keychain_service: "Arc Safe Storage",
    },
];

/// Returns a `name=value; name2=value2` Cookie header value (raw bytes so
/// non-ASCII cookie values survive verbatim) containing every decrypted
/// `xiaomimimo.com` cookie found in Chrome, Edge, or Arc.
pub fn fetch_mimo_cookie_header() -> Result<Vec<u8>, String> {
    let user_dirs = user_application_support_dir()?;

    let mut failures: Vec<String> = Vec::new();
    for browser in BROWSERS {
        match collect_browser_cookies(browser, &user_dirs) {
            Ok(header) if !header.is_empty() => return Ok(header),
            Ok(_) => failures.push(format!(
                "{}: no {} cookies found",
                browser.name, MIMO_COOKIE_HOST
            )),
            Err(error) => failures.push(format!("{}: {}", browser.name, error)),
        }
    }
    Err(format_mimo_cookie_failure(&failures.join("; ")))
}

fn format_mimo_cookie_failure(attempts: &str) -> String {
    if attempts.contains("Full Disk Access")
        || attempts.contains("Keychain rejected")
        || attempts.contains("Keychain item")
    {
        return format!("Could not read browser cookies. {attempts}");
    }
    if attempts.contains("could not decrypt") {
        return format!("MiMo console cookie decryption failed. {attempts}");
    }
    format!(
        "Could not read a MiMo console session ({PLATFORM_SESSION_COOKIE}) from Chrome, Edge, or Arc. Open {PLATFORM_CONSOLE_URL} and wait until the balance page loads /api/v1/balance, then retry. Opening https://mimo.org is not enough, and account.xiaomi.com cookies are not used. Attempts: {attempts}"
    )
}

fn user_application_support_dir() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join("Library/Application Support"))
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Could not locate ~/Library/Application Support".to_string())
}

fn collect_browser_cookies(browser: &BrowserSpec, user_dirs: &Path) -> Result<Vec<u8>, String> {
    let user_data_dir = user_dirs.join(browser.app_support_dir);
    if !user_data_dir.is_dir() {
        return Err(format!(
            "browser profile directory not found at {}",
            user_data_dir.display()
        ));
    }

    let mut db_paths = find_cookie_databases(&user_data_dir)?;
    if db_paths.is_empty() {
        return Err(format!(
            "no Cookies database found under {} (is the browser installed and signed in?)",
            user_data_dir.display()
        ));
    }
    db_paths.sort();

    let key = keychain_secret(browser.keychain_service)?;

    let mut failures: Vec<String> = Vec::new();
    let mut pairs: Vec<(String, Vec<u8>)> = Vec::new();
    for db_path in db_paths {
        match collect_profile_cookies(&db_path, &key) {
            Ok(mut profile_pairs) => pairs.append(&mut profile_pairs),
            Err(error) => failures.push(format!("{}: {}", db_path.display(), error)),
        }
    }

    if pairs.is_empty() {
        let detail = if failures.is_empty() {
            "no cookies matched".to_string()
        } else {
            failures.join("; ")
        };
        return Err(browser_cookie_failure(browser.name, &detail));
    }

    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs.dedup_by(|a, b| a.0 == b.0);
    let mut header: Vec<u8> = Vec::new();
    for (name, value) in pairs {
        if !header.is_empty() {
            header.extend_from_slice(b"; ");
        }
        header.extend_from_slice(name.as_bytes());
        header.push(b'=');
        header.extend_from_slice(&value);
    }
    Ok(header)
}

/// Finds every profile-level Cookies database (modern profiles store it under
/// `Network/`, older ones directly in the profile directory).
fn find_cookie_databases(user_data_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(user_data_dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            format!(
                "macOS blocked access to {} (grant CodexHelper Full Disk Access in System Settings, Privacy & Security, Full Disk Access, then retry)",
                user_data_dir.display()
            )
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
    Ok(found)
}

fn keychain_secret(service: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()
        .map_err(|error| format!("failed to run `security find-generic-password`: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("could not be found") || stderr.contains("SecKeychainSearch") {
            return Err(format!(
                "Keychain item \"{service}\" not found; open {} and sign in once to create it",
                MIMO_COOKIE_HOST
            ));
        }
        return Err(format!(
            "Keychain rejected access to \"{service}\" ({}). Approve the macOS Keychain prompt and retry",
            if stderr.is_empty() {
                "access denied"
            } else {
                &stderr
            }
        ));
    }
    let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if secret.is_empty() {
        return Err(format!("Keychain item \"{service}\" is empty"));
    }
    Ok(secret.into_bytes())
}

/// Derives candidate AES-128 keys for a Chromium Safe Storage secret.
fn candidate_keys(secret: &[u8]) -> Vec<[u8; KEY_LEN]> {
    let mut keys = Vec::new();
    // PBKDF2-HMAC-SHA1 with the well-known "saltysalt" salt is the standard
    // Chromium-on-macOS derivation.
    let mut pbkdf2_key = [0u8; KEY_LEN];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(secret, b"saltysalt", 1003, &mut pbkdf2_key);
    keys.push(pbkdf2_key);
    // Newer builds store a 32-hex-char secret whose bytes are the raw key.
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

fn collect_profile_cookies(
    db_path: &Path,
    keychain_secret: &[u8],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let temp = tempfile::Builder::new()
        .prefix("codex-helper-cookies-")
        .tempdir()
        .map_err(|error| format!("failed to create temp dir: {error}"))?;
    let copy_path = temp.path().join("Cookies");
    std::fs::copy(db_path, &copy_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            format!(
                "macOS blocked access to {} (grant CodexHelper Full Disk Access in System Settings, Privacy & Security, Full Disk Access, then retry)",
                db_path.display()
            )
        } else {
            format!("failed to copy cookies db: {error}")
        }
    })?;
    // Copy WAL/SHM sidecars so the snapshot includes recent writes.
    for suffix in ["-wal", "-shm"] {
        let source = PathBuf::from(format!("{}{suffix}", db_path.display()));
        if source.is_file() {
            let _ = std::fs::copy(&source, temp.path().join(format!("Cookies{suffix}")));
        }
    }

    let connection = rusqlite::Connection::open_with_flags(
        &copy_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("failed to open cookies db: {error}"))?;

    let db_version = cookie_db_version(&connection);
    let mut statement = connection
        .prepare(
            "SELECT host_key, name, encrypted_value, expires_utc, is_persistent
             FROM cookies WHERE host_key LIKE ?1",
        )
        .map_err(|error| format!("failed to query cookies db: {error}"))?;
    let pattern = format!("%{MIMO_COOKIE_HOST}%");
    let rows = statement
        .query_map([pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| format!("failed to query cookies db: {error}"))?;

    let keys = candidate_keys(keychain_secret);
    let now_unix_micros = unix_time_micros();
    let mut pairs = Vec::new();
    let mut matched_names = Vec::new();
    let mut decrypt_errors = Vec::new();
    let mut saw_expired_session = false;
    for row in rows {
        let (host_key, name, encrypted, expires_utc, is_persistent) =
            row.map_err(|error| format!("failed to read cookie row: {error}"))?;
        if name.is_empty() {
            continue;
        }
        matched_names.push(name.clone());
        if cookie_is_expired_at(is_persistent, expires_utc, now_unix_micros) {
            if name == PLATFORM_SESSION_COOKIE {
                saw_expired_session = true;
            }
            continue;
        }
        if encrypted.is_empty() {
            decrypt_errors.push(format!(
                "cookie \"{name}\" on {host_key} has an empty encrypted value"
            ));
            continue;
        }
        match decrypt_cookie_value(&encrypted, &host_key, db_version, &keys) {
            Ok(value) => pairs.push((name, value)),
            Err(error) => decrypt_errors.push(format!(
                "failed to decrypt cookie \"{name}\" on {host_key}: {error}"
            )),
        }
    }
    if pairs
        .iter()
        .any(|(name, _)| name == PLATFORM_SESSION_COOKIE)
    {
        // Optional cookies such as api-platform_ph must not hide a decrypted
        // session token. Their failures are omitted once the session exists.
        return Ok(pairs);
    }
    let session_decrypt_error = decrypt_errors
        .iter()
        .find(|error| error.contains(PLATFORM_SESSION_COOKIE));
    if let Some(error) = session_decrypt_error {
        return Err(format!(
            "Found {PLATFORM_SESSION_COOKIE} but could not decrypt it: {error}. Full Disk Access is not the problem"
        ));
    }
    let diagnosis = diagnose_cookie_db(&connection)
        .unwrap_or_else(|error| format!("diagnosis unavailable: {error}"));
    Err(missing_platform_session_error(
        &diagnosis,
        &matched_names,
        saw_expired_session,
        &decrypt_errors,
    ))
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

fn unix_time_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_micros()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn cookie_is_expired_at(is_persistent: i64, expires_utc: i64, now_unix_micros: i64) -> bool {
    if is_persistent == 0 || expires_utc <= 0 {
        return false;
    }
    expires_utc < now_unix_micros.saturating_add(CHROME_EPOCH_UNIX_MICROS)
}

fn browser_cookie_failure(browser_name: &str, detail: &str) -> String {
    if detail.contains(PLATFORM_SESSION_COOKIE) {
        return detail.to_string();
    }
    format!(
        "failed to read cookies ({detail}); sign in to {PLATFORM_CONSOLE_URL} in {browser_name} and approve Keychain access, then retry"
    )
}

fn missing_platform_session_error(
    diagnosis: &str,
    matched_names: &[String],
    expired_session: bool,
    decrypt_errors: &[String],
) -> String {
    let matched = if matched_names.is_empty() {
        format!("no {MIMO_COOKIE_HOST} rows")
    } else {
        format!(
            "{MIMO_COOKIE_HOST} cookies present: {}",
            matched_names.join(", ")
        )
    };
    let mut message = if expired_session {
        format!(
            "{PLATFORM_SESSION_COOKIE} is in the browser cookie database but has expired. Open {PLATFORM_CONSOLE_URL} and wait until the balance page loads /api/v1/balance, then retry. It lasts about 24 hours. {matched}. {diagnosis}. Full Disk Access is not the problem"
        )
    } else {
        format!(
            "No MiMo console session ({PLATFORM_SESSION_COOKIE} on .platform.xiaomimimo.com). {matched}. {diagnosis}. Open {PLATFORM_CONSOLE_URL} and wait until the balance page loads /api/v1/balance, then retry. That cookie lasts about 24 hours and is missing after expiry or a browser restart. Opening https://mimo.org does not create it, and account.xiaomi.com cookies cannot be exchanged for it. Full Disk Access is not the problem"
        )
    };
    if !decrypt_errors.is_empty() {
        message.push_str(&format!(". Decrypt errors: {}", decrypt_errors.join("; ")));
    }
    message
}

/// Summarizes the cookie database when no rows matched so the error message
/// distinguishes an empty or stale database from cookies stored under a
/// related Xiaomi SSO host. Only xiaomi/mimo host names are surfaced.
fn diagnose_cookie_db(connection: &rusqlite::Connection) -> Result<String, String> {
    let total: i64 = connection
        .query_row("SELECT COUNT(*) FROM cookies", [], |row| row.get(0))
        .map_err(|error| format!("failed to count cookies: {error}"))?;
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT host_key FROM cookies
             WHERE host_key LIKE '%mimo%' OR host_key LIKE '%xiaomi%'
             ORDER BY host_key",
        )
        .map_err(|error| format!("failed to list related hosts: {error}"))?;
    let hosts = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to list related hosts: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to list related hosts: {error}"))?;
    if hosts.is_empty() {
        Ok(format!(
            "{total} cookies stored, none for xiaomi/mimo hosts"
        ))
    } else {
        Ok(format!(
            "{total} cookies stored, related hosts: {}",
            hosts.join(", ")
        ))
    }
}

/// Per the http crate, header values may be tab plus any byte in
/// 0x20..=0x7e and the opaque range 0x80..=0xff; anything else (control
/// bytes, DEL) means the decryption produced garbage.
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
            "unsupported cookie encryption version {} (re-sign in to the browser to refresh cookies)",
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

/// Chrome cookie DB version >= 24 prefixes plaintext with SHA256(host_key)
/// before encryption. Remove that prefix only when it matches this row.
/// A mismatch is rejected on version >= 24 and left unstripped otherwise,
/// so an older database is not sliced unconditionally.
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
    // A wrong key can still pass the PKCS7 check by chance and produce
    // garbage bytes; such values would corrupt the Cookie header, so treat
    // them as decryption failures. Values with high bytes (>= 0x80) are
    // legitimate: some servers set UTF-8 cookie values, and Chromium sends
    // them verbatim as opaque header bytes.
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
        let value = b"api-platform-session";
        let encrypted = encrypt_cookie(&keys[0], &with_host_hash(host, value));
        let decrypted = decrypt_cookie_value(&encrypted, host, Some(24), &keys).unwrap();
        assert_eq!(decrypted, value);
        let without_version = decrypt_cookie_value(&encrypted, host, None, &keys).unwrap();
        assert_eq!(without_version, value);
    }

    #[test]
    fn keeps_legacy_cookie_without_host_hash() {
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
    fn missing_session_message_does_not_blame_keychain() {
        let message = missing_platform_session_error(
            "1815 cookies stored, related hosts: .account.xiaomi.com, .mimo.org",
            &[],
            false,
            &[],
        );
        assert!(message.contains(PLATFORM_SESSION_COOKIE), "{message}");
        assert!(message.contains(PLATFORM_CONSOLE_URL), "{message}");
        assert!(message.contains("mimo.org"), "{message}");
        assert!(
            message.contains("Full Disk Access is not the problem"),
            "{message}"
        );
        assert!(!message.contains("Keychain"), "{message}");
        let wrapped = browser_cookie_failure("Chrome", &message);
        assert_eq!(wrapped, message);
        let decrypt = format_mimo_cookie_failure(
            "Chrome: Found api-platform_serviceToken but could not decrypt it",
        );
        assert!(
            decrypt.starts_with("MiMo console cookie decryption failed"),
            "{decrypt}"
        );
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

    #[test]
    fn diagnosis_reports_related_hosts() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE cookies (host_key TEXT NOT NULL, name TEXT NOT NULL)",
                [],
            )
            .unwrap();
        for host in ["example.com", ".xiaomimimo.com", "platform.xiaomimimo.com"] {
            connection
                .execute(
                    "INSERT INTO cookies (host_key, name) VALUES (?1, 'session')",
                    [host],
                )
                .unwrap();
        }
        let diagnosis = diagnose_cookie_db(&connection).unwrap();
        assert!(diagnosis.contains("3 cookies stored"), "{diagnosis}");
        assert!(diagnosis.contains(".xiaomimimo.com"), "{diagnosis}");
        assert!(!diagnosis.contains("example.com"), "{diagnosis}");
    }

    #[test]
    fn diagnosis_reports_unrelated_database() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE cookies (host_key TEXT NOT NULL, name TEXT NOT NULL)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO cookies (host_key, name) VALUES ('example.com', 'a')",
                [],
            )
            .unwrap();
        let diagnosis = diagnose_cookie_db(&connection).unwrap();
        assert_eq!(diagnosis, "1 cookies stored, none for xiaomi/mimo hosts");
    }
}
