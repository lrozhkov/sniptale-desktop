//! Dev-local install trust marker owner.

use serde::Deserialize;

#[cfg(windows)]
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use sha2::{Digest, Sha256};

#[cfg(windows)]
const DEV_LOCAL_BROWSER_TRUST_FILE: &str = "dev-local-browser-trust.json";
const DEV_LOCAL_BROWSER_TRUST_KIND: &str = "sniptale-dev-local-browser-trust";
const DEV_LOCAL_BROWSER_TRUST_SCOPE: &str = "current-user-browser-parent";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevLocalBrowserTrustFile {
    kind: String,
    enabled: bool,
    scope: String,
    executable_sha256: String,
}

#[cfg(windows)]
pub(crate) fn current_user_local_app_data_roots() -> Vec<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

#[cfg(windows)]
pub(crate) fn dev_local_browser_trust_enabled() -> bool {
    let compiled_enabled = dev_local_browser_trust_compiled_enabled();
    if !compiled_enabled {
        return false;
    }
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let Some(directory) = executable.parent() else {
        return false;
    };
    let marker_path = directory.join(DEV_LOCAL_BROWSER_TRUST_FILE);
    let Ok(payload) = std::fs::read_to_string(marker_path) else {
        return false;
    };
    let Some(executable_sha256) = file_sha256_hex(&executable) else {
        return false;
    };
    dev_local_browser_trust_enabled_for_marker(compiled_enabled, &payload, &executable_sha256)
}

#[cfg(windows)]
fn dev_local_browser_trust_compiled_enabled() -> bool {
    option_env!("SNIPTALE_DEV_LOCAL_BROWSER_TRUST").is_some_and(|value| value == "1")
}

fn dev_local_browser_trust_enabled_for_marker(
    compiled_enabled: bool,
    payload: &str,
    executable_sha256: &str,
) -> bool {
    compiled_enabled && dev_local_browser_trust_matches(payload, executable_sha256)
}

fn dev_local_browser_trust_matches(payload: &str, executable_sha256: &str) -> bool {
    let Ok(marker) = serde_json::from_str::<DevLocalBrowserTrustFile>(payload) else {
        return false;
    };
    marker.kind == DEV_LOCAL_BROWSER_TRUST_KIND
        && marker.scope == DEV_LOCAL_BROWSER_TRUST_SCOPE
        && marker.enabled
        && marker
            .executable_sha256
            .eq_ignore_ascii_case(executable_sha256)
}

#[cfg(windows)]
fn file_sha256_hex(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hex_lower(&hasher.finalize()))
}

#[cfg(windows)]
fn hex_lower(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::dev_local_browser_trust_enabled_for_marker;

    const EXE_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn marker(enabled: bool, executable_sha256: &str) -> String {
        format!(
            r#"{{
                "kind":"sniptale-dev-local-browser-trust",
                "enabled":{enabled},
                "scope":"current-user-browser-parent",
                "executableSha256":"{executable_sha256}"
            }}"#
        )
    }

    #[test]
    fn accepts_enabled_marker_bound_to_current_executable_hash() {
        assert!(dev_local_browser_trust_enabled_for_marker(
            true,
            &marker(true, EXE_HASH),
            EXE_HASH
        ));
        assert!(dev_local_browser_trust_enabled_for_marker(
            true,
            &marker(true, &EXE_HASH.to_ascii_uppercase()),
            EXE_HASH,
        ));
    }

    #[test]
    fn rejects_disabled_malformed_or_stale_marker() {
        assert!(!dev_local_browser_trust_enabled_for_marker(
            true,
            &marker(false, EXE_HASH),
            EXE_HASH,
        ));
        assert!(!dev_local_browser_trust_enabled_for_marker(
            true,
            &marker(
                true,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            EXE_HASH,
        ));
        assert!(!dev_local_browser_trust_enabled_for_marker(
            true,
            r#"{"kind":"sniptale-dev-local-browser-trust"}"#,
            EXE_HASH,
        ));
    }

    #[test]
    fn rejects_marker_when_binary_was_not_compiled_for_dev_local_trust() {
        assert!(!dev_local_browser_trust_enabled_for_marker(
            false,
            &marker(true, EXE_HASH),
            EXE_HASH,
        ));
    }
}
