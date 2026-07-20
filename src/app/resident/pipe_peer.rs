//! Resident pipe peer provenance policy.

mod consent;
#[cfg(test)]
mod consent_policy_tests;
#[cfg(test)]
mod consent_tests;
#[cfg(any(windows, test))]
mod executable;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

const ALLOWED_BROWSER_PARENT_EXECUTABLES: &[&str] = &["chrome.exe", "msedge.exe", "chromium.exe"];
const TRUSTED_CHROMIUM_PARENT_SUFFIXES: &[&str] = &[
    r"\google\chrome\application\chrome.exe",
    r"\chromium\application\chrome.exe",
    r"\chromium\application\chromium.exe",
];
const TRUSTED_EDGE_PARENT_SUFFIX: &str = r"\microsoft\edge\application\msedge.exe";
const TRUSTED_EDGE_VERSIONED_PARENT_MARKER: &str = r"\microsoft\edge\application\";
const TRUSTED_EDGE_VERSIONED_PARENT_SUFFIX: &str = r"\msedge.exe";
const DEV_LOCAL_CHROMIUM_PARENT_SUFFIX: &str = r"\chromium\application\chrome.exe";
#[cfg(windows)]
pub(super) use consent::ResidentConnectionConsent;
#[cfg(windows)]
pub(super) use executable::accepted_executable_identity;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParentProcessFingerprint {
    pub(super) fingerprint: String,
    pub(super) label: String,
    pub(super) category: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BrowserParentImageDecision {
    Accepted,
    Rejected(BrowserParentImageRejection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BrowserParentImageRejection {
    UnsupportedExecutable,
    UnexpectedLayout,
    UntrustedRoot,
}

impl BrowserParentImageDecision {
    const fn diagnostic_category(self) -> &'static str {
        match self {
            Self::Accepted => "recognized-browser-parent",
            Self::Rejected(BrowserParentImageRejection::UnsupportedExecutable) => {
                "unsupported-parent-executable"
            }
            Self::Rejected(BrowserParentImageRejection::UnexpectedLayout) => {
                "unexpected-parent-layout"
            }
            Self::Rejected(BrowserParentImageRejection::UntrustedRoot) => "untrusted-parent-root",
        }
    }
}

#[cfg(windows)]
pub(super) fn browser_parent_image_path_decision(path: &Path) -> BrowserParentImageDecision {
    let trusted_roots = crate::install::current_program_files_roots();
    let dev_local_roots = if crate::install::dev_local_browser_trust_enabled() {
        crate::install::current_user_local_app_data_roots()
    } else {
        Vec::new()
    };
    classify_browser_parent_image_path(
        path,
        trusted_roots.iter().map(PathBuf::as_path),
        dev_local_roots.iter().map(PathBuf::as_path),
        !dev_local_roots.is_empty(),
    )
}

#[cfg(windows)]
pub(super) fn browser_parent_fingerprint(path: &Path) -> ParentProcessFingerprint {
    let decision = browser_parent_image_path_decision(path);
    parent_process_fingerprint_from_decision(path, decision)
}

#[cfg(test)]
fn trusted_browser_parent_fingerprint<'a>(
    path: &Path,
    trusted_roots: impl IntoIterator<Item = &'a Path>,
) -> ParentProcessFingerprint {
    let decision = trusted_browser_parent_image_path_decision(path, trusted_roots);
    parent_process_fingerprint_from_decision(path, decision)
}

fn parent_process_fingerprint_from_decision(
    path: &Path,
    decision: BrowserParentImageDecision,
) -> ParentProcessFingerprint {
    let normalized_path = normalized_windows_path(path);
    let label = sanitized_leaf_label(&normalized_path);
    let category = parent_diagnostic_category(&label, decision);
    let fingerprint = crate::transfer::hashing::sha256_hex(
        format!("{category}|{label}|{normalized_path}").as_bytes(),
    );
    ParentProcessFingerprint {
        fingerprint,
        label,
        category,
    }
}

fn parent_diagnostic_category(label: &str, decision: BrowserParentImageDecision) -> &'static str {
    if matches!(
        label,
        "cmd.exe" | "powershell.exe" | "pwsh.exe" | "conhost.exe"
    ) {
        "shell-launcher-parent"
    } else {
        decision.diagnostic_category()
    }
}

fn classify_browser_parent_image_path<'a, 'b>(
    path: &Path,
    trusted_roots: impl IntoIterator<Item = &'a Path>,
    dev_local_roots: impl IntoIterator<Item = &'b Path>,
    dev_local_trust_enabled: bool,
) -> BrowserParentImageDecision {
    let normalized_path = normalized_windows_path(path);
    let trusted_roots = normalized_roots(trusted_roots);
    let dev_local_roots = if dev_local_trust_enabled {
        normalized_roots(dev_local_roots)
    } else {
        Vec::new()
    };

    if !has_allowed_browser_executable(&normalized_path) {
        return BrowserParentImageDecision::Rejected(
            BrowserParentImageRejection::UnsupportedExecutable,
        );
    }

    if is_trusted_browser_parent_path(&normalized_path, &trusted_roots)
        || is_dev_local_browser_parent_path(&normalized_path, &dev_local_roots)
    {
        return BrowserParentImageDecision::Accepted;
    }

    let has_trusted_layout = has_trusted_browser_layout(&normalized_path);
    let has_dev_local_layout =
        dev_local_trust_enabled && normalized_path.ends_with(DEV_LOCAL_CHROMIUM_PARENT_SUFFIX);
    if !has_trusted_layout && !has_dev_local_layout {
        return BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout);
    }

    if is_under_any_normalized_root(&normalized_path, &trusted_roots)
        || is_under_any_normalized_root(&normalized_path, &dev_local_roots)
    {
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    } else {
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UntrustedRoot)
    }
}

#[cfg(test)]
fn trusted_browser_parent_image_path_decision<'a>(
    path: &Path,
    trusted_roots: impl IntoIterator<Item = &'a Path>,
) -> BrowserParentImageDecision {
    classify_browser_parent_image_path(path, trusted_roots, [], false)
}

fn has_allowed_browser_executable(path: &str) -> bool {
    path.rsplit('\\')
        .next()
        .is_some_and(|file_name| ALLOWED_BROWSER_PARENT_EXECUTABLES.contains(&file_name))
}

fn sanitized_leaf_label(normalized_path: &str) -> String {
    let leaf = normalized_path
        .rsplit('\\')
        .next()
        .unwrap_or("unknown-parent");
    let mut label = String::with_capacity(leaf.len().min(80));
    for byte in leaf.bytes().take(80) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            label.push(char::from(byte));
        } else {
            label.push('_');
        }
    }
    if label.is_empty() {
        "unknown-parent".to_owned()
    } else {
        label
    }
}

fn has_trusted_browser_layout(path: &str) -> bool {
    TRUSTED_CHROMIUM_PARENT_SUFFIXES
        .iter()
        .any(|suffix| path.ends_with(suffix))
        || path.ends_with(TRUSTED_EDGE_PARENT_SUFFIX)
        || has_trusted_edge_versioned_layout(path)
}

fn has_trusted_edge_versioned_layout(path: &str) -> bool {
    let Some((_, version_and_exe)) = path.rsplit_once(TRUSTED_EDGE_VERSIONED_PARENT_MARKER) else {
        return false;
    };
    let Some(version) = version_and_exe.strip_suffix(TRUSTED_EDGE_VERSIONED_PARENT_SUFFIX) else {
        return false;
    };
    is_valid_edge_version_segment(version)
}

fn is_trusted_browser_parent_path(path: &str, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        TRUSTED_CHROMIUM_PARENT_SUFFIXES
            .iter()
            .any(|suffix| path_equals_root_suffix(path, root, suffix))
            || path_equals_root_suffix(path, root, TRUSTED_EDGE_PARENT_SUFFIX)
            || has_trusted_edge_versioned_layout_under_root(path, root)
    })
}

fn is_dev_local_browser_parent_path(path: &str, roots: &[String]) -> bool {
    roots
        .iter()
        .any(|root| path_equals_root_suffix(path, root, DEV_LOCAL_CHROMIUM_PARENT_SUFFIX))
}

fn path_equals_root_suffix(path: &str, root: &str, suffix: &str) -> bool {
    !root.is_empty() && path.strip_prefix(root).is_some_and(|rest| rest == suffix)
}

fn has_trusted_edge_versioned_layout_under_root(path: &str, root: &str) -> bool {
    if root.is_empty() {
        return false;
    }
    let prefix = format!("{root}{TRUSTED_EDGE_VERSIONED_PARENT_MARKER}");
    let Some(version_and_exe) = path.strip_prefix(&prefix) else {
        return false;
    };
    let Some(version) = version_and_exe.strip_suffix(TRUSTED_EDGE_VERSIONED_PARENT_SUFFIX) else {
        return false;
    };
    is_valid_edge_version_segment(version)
}

fn is_valid_edge_version_segment(version: &str) -> bool {
    let mut segment_count = 0;
    for segment in version.split('.') {
        if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        segment_count += 1;
    }
    segment_count > 1
}

fn normalized_roots<'a>(roots: impl IntoIterator<Item = &'a Path>) -> Vec<String> {
    roots.into_iter().map(normalized_windows_path).collect()
}

fn is_under_any_normalized_root(path: &str, roots: &[String]) -> bool {
    roots.iter().any(|root| is_under_trusted_root(path, root))
}

fn is_under_trusted_root(path: &str, root: &str) -> bool {
    !root.is_empty()
        && path.len() > root.len()
        && path.starts_with(root)
        && path.as_bytes().get(root.len()) == Some(&b'\\')
}

fn normalized_windows_path(path: &Path) -> String {
    path.as_os_str()
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests;
