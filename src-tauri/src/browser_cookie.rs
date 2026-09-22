//! Reads cookies from local Chromium-family browsers (Chrome, Edge, Arc) on
//! macOS so MiMo usage queries can authenticate exactly like the web console.
//!
//! Flow per browser profile:
//! 1. Copy the profile's SQLite `Cookies` database (plus WAL/SHM) to a
//!    temporary directory to avoid lock contention with the running browser.
//! 2. Read `host_key` / `name` / `encrypted_value` rows for
//!    `xiaomimimo.com` hosts.
//! 3. Fetch the browser's "Safe Storage" secret from the macOS Keychain and
//!    derive the AES-128 key (PBKDF2-HMAC-SHA1, salt `saltysalt`, 1003
//!    iterations; a 32-char hex secret is used directly as the raw key).
//! 4. Decrypt `v10`/`v11` cookies (AES-128-CBC, IV = 16 spaces, PKCS7) and
//!    assemble a `Cookie:` header value.
//!
//! Every failure path produces an explicit error; nothing falls back
//! silently.

use std::path::{Path, PathBuf};
use std::process::Command;

use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

const MIMO_COOKIE_HOST: &str = "xiaomimimo.com";
const KEY_LEN: usize = 16;
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

/// Returns a `name=value; name2=value2` header value containing every
/// decrypted `xiaomimimo.com` cookie found in Chrome, Edge, or Arc.
pub fn fetch_mimo_cookie_header() -> Result<String, String> {
    let user_dirs = user_application_support_dir()?;

    let mut failures: Vec<String> = Vec::new();
    for browser in BROWSERS {
        match collect_browser_cookies(browser, &user_dirs) {
            Ok(header) if !header.is_empty() => return Ok(header),
            Ok(_) => failures.push(format!("{}: no {} cookies found", browser.name, MIMO_COOKIE_HOST)),
            Err(error) => failures.push(format!("{}: {}", browser.name, error)),
        }
    }
    Err(format!(
        "Could not read {} cookies from any supported browser. Sign in to https://platform.xiaomimimo.com in Chrome, Edge, or Arc, then retry. Attempts: {}",
        MIMO_COOKIE_HOST,
        failures.join("; ")
    ))
}

fn user_application_support_dir() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join("Library/Application Support"))
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Could not locate ~/Library/Application Support".to_string())
}

fn collect_browser_cookies(browser: &BrowserSpec, user_dirs: &Path) -> Result<String, String> {
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
    let mut pairs: Vec<(String, String)> = Vec::new();
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
        return Err(format!(
            "failed to read cookies ({detail}); sign in to https://platform.xiaomimimo.com in {} and approve Keychain access, then retry",
            browser.name
        ));
    }

    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs.dedup_by(|a, b| a.0 == b.0);
    Ok(pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; "))
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

fn collect_profile_cookies(db_path: &Path, keychain_secret: &[u8]) -> Result<Vec<(String, String)>, String> {
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
            let _ = std::fs::copy(
                &source,
                temp.path().join(format!("Cookies{suffix}")),
            );
        }
    }

    let connection = rusqlite::Connection::open_with_flags(
        &copy_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("failed to open cookies db: {error}"))?;

    let mut statement = connection
        .prepare("SELECT name, encrypted_value FROM cookies WHERE host_key LIKE ?1")
        .map_err(|error| format!("failed to query cookies db: {error}"))?;
    let pattern = format!("%{MIMO_COOKIE_HOST}%");
    let rows = statement
        .query_map([pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|error| format!("failed to query cookies db: {error}"))?;

    let keys = candidate_keys(keychain_secret);
    let mut pairs = Vec::new();
    for row in rows {
        let (_host, name, encrypted) = row.map_err(|error| format!("failed to read cookie row: {error}"))?;
        if name.is_empty() {
            continue;
        }
        if encrypted.is_empty() {
            continue;
        }
        let value = decrypt_cookie_value(&encrypted, &keys)
            .map_err(|error| format!("failed to decrypt cookie \"{name}\": {error}"))?;
        pairs.push((name, value));
    }
    Ok(pairs)
}

fn decrypt_cookie_value(encrypted: &[u8], keys: &[[u8; KEY_LEN]]) -> Result<String, String> {
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
        match Aes128CbcDec::new(key.into(), &IV.into())
            .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        {
            Ok(plain) => {
                let text = String::from_utf8_lossy(&plain).to_string();
                if !text.is_empty() {
                    return Ok(text);
                }
                last_error = "decrypted value was empty".to_string();
            }
            Err(error) => last_error = format!("decryption failed: {error}"),
        }
    }
    Err(last_error)
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
        let error = decrypt_cookie_value(b"v20-not-really", &keys).unwrap_err();
        assert!(error.contains("v20"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_short_or_plain_values() {
        let keys = candidate_keys(b"peanuts");
        assert!(decrypt_cookie_value(b"plain", &keys).is_err());
    }
}
