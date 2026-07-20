use std::path::Path;

use super::{
    BrowserParentImageDecision, BrowserParentImageRejection, classify_browser_parent_image_path,
    trusted_browser_parent_fingerprint, trusted_browser_parent_image_path_decision,
};

#[test]
fn accepts_known_browser_under_program_files_root() {
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Accepted
    );
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
            [Path::new(r"C:\Program Files (x86)")],
        ),
        BrowserParentImageDecision::Accepted
    );
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files\Microsoft\Edge\Application\150.0.4078.48\msedge.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Accepted
    );
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files\Chromium\Application\chrome.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Accepted
    );
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files\Chromium\Application\chromium.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Accepted
    );
}

#[test]
fn rejects_direct_launch_parents() {
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnsupportedExecutable)
    );
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Windows\System32\cmd.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnsupportedExecutable)
    );
}

#[test]
fn rejects_browser_named_parent_outside_trusted_roots() {
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Users\alice\AppData\Local\Temp\chrome.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
}

#[test]
fn rejects_browser_named_parent_under_unexpected_vendor_path() {
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files\Evil\Chrome\Application\chrome.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
}

#[test]
fn rejects_browser_named_parent_under_nested_vendor_path() {
    for path in [
        r"C:\Program Files\Evil\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Evil\Chromium\Application\chrome.exe",
        r"C:\Program Files\Evil\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Evil\Microsoft\Edge\Application\150.0.4078.48\msedge.exe",
    ] {
        assert_eq!(
            trusted_browser_parent_image_path_decision(
                Path::new(path),
                [Path::new(r"C:\Program Files")],
            ),
            BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
        );
    }
}

#[test]
fn rejects_edge_runtime_paths_outside_browser_application_layout() {
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files (x86)\Microsoft\EdgeCore\150.0.4078.48\msedge.exe"),
            [Path::new(r"C:\Program Files (x86)")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(
                r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\150.0.4078.48\msedge.exe",
            ),
            [Path::new(r"C:\Program Files (x86)")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
}

#[test]
fn rejects_edge_versioned_layout_with_malformed_version_segment() {
    for path in [
        r"C:\Program Files\Microsoft\Edge\Application\Stable\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\150\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\150.\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\150..4078\msedge.exe",
    ] {
        assert_eq!(
            trusted_browser_parent_image_path_decision(
                Path::new(path),
                [Path::new(r"C:\Program Files")],
            ),
            BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
        );
    }
}

#[test]
fn rejects_prefix_spoofed_program_files_root() {
    assert_eq!(
        trusted_browser_parent_image_path_decision(
            Path::new(r"C:\Program Files Evil\Google\Chrome\Application\chrome.exe"),
            [Path::new(r"C:\Program Files")],
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UntrustedRoot)
    );
}

#[test]
fn accepts_user_local_chromium_only_with_dev_local_trust() {
    let path = Path::new(r"C:\Users\alice\AppData\Local\Chromium\Application\chrome.exe");
    let trusted_roots = [Path::new(r"C:\Program Files")];
    let local_roots = [Path::new(r"C:\Users\alice\AppData\Local")];

    assert_eq!(
        classify_browser_parent_image_path(path, trusted_roots, local_roots, false),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UntrustedRoot)
    );
    assert_eq!(
        classify_browser_parent_image_path(path, trusted_roots, local_roots, true),
        BrowserParentImageDecision::Accepted
    );
}

#[test]
fn rejects_dev_local_browser_named_parent_under_unexpected_local_path() {
    assert_eq!(
        classify_browser_parent_image_path(
            Path::new(r"C:\Users\alice\AppData\Local\Temp\chrome.exe"),
            [Path::new(r"C:\Program Files")],
            [Path::new(r"C:\Users\alice\AppData\Local")],
            true,
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
    assert_eq!(
        classify_browser_parent_image_path(
            Path::new(r"C:\Users\alice\AppData\Local\Evil\Chromium\Application\chrome.exe"),
            [Path::new(r"C:\Program Files")],
            [Path::new(r"C:\Users\alice\AppData\Local")],
            true,
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
}

#[test]
fn rejects_playwright_chromium_even_with_dev_local_trust() {
    assert_eq!(
        classify_browser_parent_image_path(
            Path::new(
                r"C:\Users\alice\AppData\Local\ms-playwright\chromium-1194\chrome-win\chrome.exe",
            ),
            [Path::new(r"C:\Program Files")],
            [Path::new(r"C:\Users\alice\AppData\Local")],
            true,
        ),
        BrowserParentImageDecision::Rejected(BrowserParentImageRejection::UnexpectedLayout)
    );
}

#[test]
fn rejection_log_fields_are_reason_only() {
    for rejection in [
        BrowserParentImageRejection::UnsupportedExecutable,
        BrowserParentImageRejection::UnexpectedLayout,
        BrowserParentImageRejection::UntrustedRoot,
    ] {
        let code = rejection_code(rejection);
        let message = rejection_message(rejection);
        assert!(code.starts_with("resident_pipe.peer_identity_rejected."));
        assert!(!code.contains('\\'));
        assert!(!message.contains('\\'));
        assert!(!message.contains(":\\"));
        assert!(!message.contains("chrome-extension://"));
        assert!(!message.contains("pipe-auth-v2"));
    }
}

#[test]
fn parent_fingerprint_uses_redacted_leaf_and_stable_category_without_pid() {
    let first = trusted_browser_parent_fingerprint(
        Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        [Path::new(r"C:\Program Files")],
    );
    let second = trusted_browser_parent_fingerprint(
        Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        [Path::new(r"C:\Program Files")],
    );

    assert_eq!(first, second);
    assert_eq!(first.label, "chrome.exe");
    assert_eq!(first.category, "recognized-browser-parent");
    assert_eq!(first.fingerprint.len(), 64);
    assert!(!first.fingerprint.contains("Program Files"));
    assert!(!first.label.contains('\\'));
    assert!(!first.label.contains(":\\"));
}

#[test]
fn parent_fingerprint_changes_for_category_or_path_without_exposing_path() {
    let chrome = trusted_browser_parent_fingerprint(
        Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        [Path::new(r"C:\Program Files")],
    );
    let powershell = trusted_browser_parent_fingerprint(
        Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
        [Path::new(r"C:\Program Files")],
    );

    assert_ne!(chrome.fingerprint, powershell.fingerprint);
    assert_eq!(powershell.label, "powershell.exe");
    assert_eq!(powershell.category, "shell-launcher-parent");
    assert!(!powershell.fingerprint.contains("WindowsPowerShell"));
}

const fn rejection_code(rejection: BrowserParentImageRejection) -> &'static str {
    match rejection {
        BrowserParentImageRejection::UnsupportedExecutable => {
            "resident_pipe.peer_identity_rejected.unsupported_browser_parent"
        }
        BrowserParentImageRejection::UnexpectedLayout => {
            "resident_pipe.peer_identity_rejected.unexpected_browser_parent_layout"
        }
        BrowserParentImageRejection::UntrustedRoot => {
            "resident_pipe.peer_identity_rejected.untrusted_browser_parent_root"
        }
    }
}

const fn rejection_message(rejection: BrowserParentImageRejection) -> &'static str {
    match rejection {
        BrowserParentImageRejection::UnsupportedExecutable => {
            "resident pipe rejected native-host session: unsupported browser parent"
        }
        BrowserParentImageRejection::UnexpectedLayout => {
            "resident pipe rejected native-host session: unexpected browser parent layout"
        }
        BrowserParentImageRejection::UntrustedRoot => {
            "resident pipe rejected native-host session: untrusted browser parent root"
        }
    }
}
