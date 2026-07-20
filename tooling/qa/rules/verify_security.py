#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
import tempfile
from pathlib import Path

try:
    from .exception_registry import exact_paths
except ImportError:
    from exception_registry import exact_paths


ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / "src"

SENSITIVE_LOG_MARKERS = [
    "base64",
    "clipboard",
    "cookie",
    "headers",
    "secret",
    "typed_text",
    "auth_header",
    "authorization",
    "url",
    "token",
    "path",
    "raw_payload",
    "screenshot_pixels",
    "video_bytes",
    "audio_bytes",
]

FORBIDDEN_RUNTIME_PATTERNS = {
    r"\bserde_json::from_str\b": "boundary JSON must be parsed in owner parser modules",
    r"\bserde_json::from_value\b": "boundary JSON must be parsed in owner parser modules",
    r"\bstd::fs::read_to_string\b": "file boundary reads need owner parser/validator",
    r"\bstd::fs::read\b": "file boundary reads need owner parser/validator",
}

PARSER_ALLOWED = (
    "src/protocol/",
    "src/settings/",
    "src/install/",
    "src/diagnostics/",
    "src/ipc/",
    "src/app/resident/pipe_peer/",
)

REQUIRED_SECURITY_DOCS = {
    "docs/security/ipc-auth.md": [
        "per-install secret",
        "current-user",
        "bounded",
        "challenge",
        "DPAPI",
        "backpressure",
    ],
    "docs/security/install-trust.md": [
        "allowed_origins",
        "channel",
        "signature",
        "autostart",
        "--native-host",
        "Stale shadowing",
    ],
    "docs/security/data-handling.md": [
        "typed text",
        "clipboard",
        "raw Native Messaging payloads",
        "base64",
        "contracts/security/redaction-corpus.json",
    ],
    "docs/security/threat-model.md": [
        "Trust boundaries",
        "Accepted trade-offs and residual risk",
        "fully compromised Windows user session",
        "Manual output is plaintext",
        "No automatic updater",
    ],
}

REDACTION_CORPUS = ROOT / "contracts/security/redaction-corpus.json"
OPERATION_ERROR_CATALOG = ROOT / "contracts/native/operation-error-catalog.json"
UNSAFE_ADAPTER_PROOF = ROOT / "tooling/configs/qa/unsafe-ffi-adapter-proof.json"
PROOF_REFERENCE_PREFIXES = ("src/", "docs/", "contracts/", "tooling/")
APP_HOST = SRC / "app" / "host.rs"
APP_SUPPORT = SRC / "app" / "support.rs"
DIAGNOSTICS_BUNDLE = SRC / "diagnostics" / "bundle.rs"

UNSAFE_ADAPTER_ALLOWLIST = exact_paths("security.unsafe-adapter")

BUSINESS_CRITICAL_SILENT_FAILURE_PATTERNS = {
    "src/app/host.rs": {
        r"start_process_logger_for_mode_with_options[\s\S]{0,160}\.ok\s*\(": (
            "process logger startup failure must emit a redacted category warning"
        ),
    },
    "src/app/resident/transfer.rs": {
        r"open_default\s*\(\s*\)\.ok\s*\(": (
            "transfer store open failure must be logged by category, not collapsed with .ok()"
        ),
        r"let\s+_\s*=\s*store\.cleanup_expired\s*\(": (
            "transfer cleanup failure must be logged by category"
        ),
    },
    "src/app/resident/pipe.rs": {
        r"open_default\s*\(\s*\)\.ok\s*\(": (
            "resident pipe transfer store open failure must be logged by category"
        ),
    },
    "src/recording/service.rs": {
        r"let\s+_\s*=\s*self\.runtime\.cancel_recording\s*\(": (
            "recording cancel runtime failure must be returned or explicitly classified"
        ),
        r"let\s+_\s*=\s*self\.transfer_store\.discard_recording\s*\(": (
            "recording discard failure must be returned or isolated in best-effort helper"
        ),
    },
    "src/telemetry/runtime.rs": {
        r"let\s+_\s*=\s*self\.collector\.record_telemetry_warning\s*\(": (
            "telemetry warning drops must use the explicit best-effort helper"
        ),
        r"let\s+_\s*=\s*self\.record_(?:key_signal|mouse_event)\s*\(": (
            "telemetry event drops must use explicit best-effort helpers"
        ),
        r"let\s+_\s*=\s*self\.collector\.record_cursor_idle_signal\s*\(": (
            "telemetry static-signal drops must emit a warning category"
        ),
    },
    "src/telemetry/runtime/hooks.rs": {
        r"let\s+_\s*=\s*self\s*\.\s*collector\s*\.\s*record_telemetry_warning\s*\(": (
            "hook warning drops must use the explicit best-effort helper"
        ),
    },
}

# AUD11 security/diagnostics drift nets. Raw DPAPI calls must stay in exact owner adapters, and
# placeholder diagnostics output has a zero baseline after Wave 5.
AUD11_RAW_DPAPI_ALLOWED_FILES = {
    "src/install/windows/dpapi.rs",
    "src/transfer/windows_dpapi.rs",
}
AUD11_RAW_DPAPI_PATTERNS = {
    r"\bCryptProtectData\b",
    r"\bCryptUnprotectData\b",
    r"\bCRYPT_INTEGER_BLOB\b",
}
AUD11_DPAPI_COMMON_REQUIRED_TERMS = {
    "LocalBlob": "must keep a LocalBlob RAII owner for DPAPI LocalAlloc output",
    "input.is_empty()": "must reject zero-length input before calling DPAPI",
    "u32::try_from(input.len())": "must reject input larger than CRYPT_INTEGER_BLOB length",
    "InvalidOutput": "must reject null or zero-length DPAPI output",
    "ptr::write_bytes": "must wipe secret-bearing output before LocalFree",
    "LocalFree": "must release DPAPI LocalAlloc output",
}
AUD11_DPAPI_OWNER_REQUIRED_TERMS = {
    "src/install/windows/dpapi.rs": {
        "CryptUnprotectData": "install DPAPI adapter must unwrap pipe secrets",
        "unprotect_current_user_secret": "install DPAPI adapter must expose only pipe-secret unwrap",
        "DpapiSecretError": "install DPAPI adapter must expose typed install-owned errors",
    },
    "src/transfer/windows_dpapi.rs": {
        "CryptProtectData": "transfer DPAPI adapter must wrap media keys",
        "CryptUnprotectData": "transfer DPAPI adapter must unwrap media keys",
        "protect_current_user_media_key": "transfer DPAPI adapter must expose media-key protect",
        "unprotect_current_user_media_key": "transfer DPAPI adapter must expose media-key unprotect",
        "Sniptale temp media key": "transfer DPAPI adapter must keep media-key purpose string",
        "MediaDpapiError": "transfer DPAPI adapter must expose typed transfer-owned errors",
    },
}
AUD11_DPAPI_OWNER_FORBIDDEN_TERMS = {
    "src/install/windows/dpapi.rs": {
        "CryptProtectData": "install DPAPI adapter must stay unwrap-only",
        "fn protect_current_user_secret": "install DPAPI adapter must not expose a general protect API",
    },
    "src/transfer/windows_dpapi.rs": {
        "pipe secret": "transfer DPAPI adapter must not own pipe-secret semantics",
    },
}
AUD11_DPAPI_PROOF_REQUIRED_TERMS = {
    "src/install/windows/dpapi.rs": [
        "unwrap-only",
        "pipe-secret",
        "No protect API",
        "LocalBlob",
    ],
    "src/transfer/windows_dpapi.rs": [
        "per-media",
        "media-key",
        "protected temp media",
        "LocalBlob",
    ],
}
AUD11_PLACEHOLDER_RECENT_ERRORS_BASELINE = {}
AUD11_PLACEHOLDER_RECENT_ERRORS_PATTERN = re.compile(
    r"recentErrors\\?\"\s*:\s*\\?\[\s*\\?\]"
)


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def proof_reference_path(value: str) -> str | None:
    stripped = value.strip()
    if not stripped.startswith(PROOF_REFERENCE_PREFIXES):
        return None
    return stripped.split(maxsplit=1)[0].strip("`'\".,;:)")


def path_label(path: Path, root: Path = ROOT) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def write_selftest_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content.strip() + "\n", encoding="utf-8")


def rust_files() -> list[Path]:
    if not SRC.exists():
        return []
    return [path for path in SRC.rglob("*.rs") if "/tests/" not in path.as_posix()]


def rust_files_under(root: Path) -> list[Path]:
    src = root / "src"
    if not src.exists():
        return []
    return [
        path
        for path in src.rglob("*.rs")
        if "/tests/" not in path.as_posix() and path.name != "tests.rs"
    ]


def fail(messages: list[str]) -> None:
    print("FAIL security guardrails")
    for message in messages:
        print(f"- {message}")
    raise SystemExit(1)


def check_forbidden_runtime_patterns(errors: list[str]) -> None:
    for path in rust_files():
        relative = rel(path)
        text = read(path)
        for pattern, reason in FORBIDDEN_RUNTIME_PATTERNS.items():
            if not re.search(pattern, text):
                continue
            if relative.startswith(PARSER_ALLOWED):
                continue
            errors.append(f"{relative}: {reason}")


def check_sensitive_logging_markers(errors: list[str]) -> None:
    log_pattern = re.compile(r"\b(trace|debug|info|warn|error)!\s*\(")
    for path in rust_files():
        text = read(path)
        if not log_pattern.search(text):
            continue
        lowered = text.lower()
        markers = [marker for marker in SENSITIVE_LOG_MARKERS if marker in lowered]
        if markers:
            errors.append(
                f"{rel(path)} logs near sensitive marker(s): " + ", ".join(sorted(set(markers)))
            )


def raw_dpapi_adapter_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for path in rust_files_under(root):
        relative = path_label(path, root)
        if relative in AUD11_RAW_DPAPI_ALLOWED_FILES:
            continue
        text = path.read_text(encoding="utf-8")
        matched = [
            pattern
            for pattern in AUD11_RAW_DPAPI_PATTERNS
            if re.search(pattern, text)
        ]
        if matched:
            offenders.append(
                f"{relative} uses raw DPAPI mechanics outside AUD11 approved owner files: "
                + ", ".join(matched)
            )
    return offenders


def check_audit11_raw_dpapi_does_not_expand_allowlist(errors: list[str]) -> None:
    errors.extend(raw_dpapi_adapter_errors(ROOT))


def dpapi_owner_invariant_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for relative, required_terms in AUD11_DPAPI_OWNER_REQUIRED_TERMS.items():
        path = root / relative
        if not path.is_file():
            offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for term, reason in {
            **AUD11_DPAPI_COMMON_REQUIRED_TERMS,
            **required_terms,
        }.items():
            if term not in text:
                offenders.append(f"{relative}: {reason}")
        for term, reason in AUD11_DPAPI_OWNER_FORBIDDEN_TERMS.get(relative, {}).items():
            if term in text:
                offenders.append(f"{relative}: {reason}")
    return offenders


def check_audit11_dpapi_owner_invariants(errors: list[str]) -> None:
    errors.extend(dpapi_owner_invariant_errors(ROOT))


def unsafe_proof_catalog(root: Path) -> tuple[list[dict[str, object]], list[str]]:
    path = root / UNSAFE_ADAPTER_PROOF.relative_to(ROOT)
    if not path.is_file():
        return [], ["missing tooling/configs/qa/unsafe-ffi-adapter-proof.json"]
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        return [], [f"invalid unsafe adapter proof catalog JSON: {error}"]
    adapters = payload.get("adapters")
    if not isinstance(adapters, list):
        return [], ["unsafe adapter proof catalog adapters must be a list"]
    return [adapter for adapter in adapters if isinstance(adapter, dict)], []


def unsafe_dpapi_proof_errors(root: Path) -> list[str]:
    adapters, errors = unsafe_proof_catalog(root)
    offenders = list(errors)
    by_path = {
        adapter.get("path"): adapter
        for adapter in adapters
        if isinstance(adapter.get("path"), str)
    }
    for relative, required_terms in AUD11_DPAPI_PROOF_REQUIRED_TERMS.items():
        adapter = by_path.get(relative)
        if adapter is None:
            offenders.append(f"{relative} missing DPAPI unsafe proof entry")
            continue
        proof_text = " ".join(
            str(value)
            for key in ("boundary", "safeWrapper", "raiiWrapper")
            for value in [adapter.get(key, "")]
        )
        for key in ("invariants", "shutdownCancelProof"):
            values = adapter.get(key, [])
            if isinstance(values, list):
                proof_text += " " + " ".join(str(value) for value in values)
        for term in required_terms:
            if term not in proof_text:
                offenders.append(f"{relative} unsafe proof missing DPAPI ownership term {term!r}")
    return offenders


def check_audit11_dpapi_unsafe_proof(errors: list[str]) -> None:
    errors.extend(unsafe_dpapi_proof_errors(ROOT))


def placeholder_recent_errors_counts(root: Path) -> dict[str, int]:
    counts: dict[str, int] = {}
    for path in rust_files_under(root):
        relative = path_label(path, root)
        count = len(AUD11_PLACEHOLDER_RECENT_ERRORS_PATTERN.findall(path.read_text(encoding="utf-8")))
        if count:
            counts[relative] = count
    return counts


def placeholder_recent_errors_ratchet_errors(
    root: Path,
    baseline: dict[str, int] | None = None,
) -> list[str]:
    baseline = AUD11_PLACEHOLDER_RECENT_ERRORS_BASELINE if baseline is None else baseline
    offenders: list[str] = []
    for relative, count in sorted(placeholder_recent_errors_counts(root).items()):
        allowed = baseline.get(relative, 0)
        if count > allowed:
            offenders.append(
                f"{relative} has {count} hard-coded recentErrors empty output(s), "
                f"above AUD11 final baseline {allowed}"
            )
    return offenders


def check_audit11_placeholder_recent_errors_does_not_expand_baseline(errors: list[str]) -> None:
    errors.extend(placeholder_recent_errors_ratchet_errors(ROOT))


def check_unsafe_policy(errors: list[str]) -> None:
    for path in rust_files():
        relative = rel(path)
        text = read(path)
        if "#![allow(unsafe_code)]" in text and relative not in UNSAFE_ADAPTER_ALLOWLIST:
            errors.append(f"{relative} opts into unsafe_code outside allowlisted adapter policy")
        if not re.search(r"\bunsafe\s*\{|\bunsafe\s+fn\b|\bunsafe\s+impl\b", text):
            continue
        if relative not in UNSAFE_ADAPTER_ALLOWLIST:
            errors.append(f"{relative} contains unsafe outside allowlisted adapter policy")
            continue
        if "#![allow(unsafe_code)]" not in text:
            errors.append(f"{relative} must explicitly opt into unsafe_code")
        if "SAFETY:" not in text:
            errors.append(f"{relative} must document unsafe adapter invariants with SAFETY comments")
        if re.search(r"\bpub(?:\(crate\))?\s+unsafe\s+fn\b", text):
            errors.append(f"{relative} must not export unsafe functions across the owner boundary")


def check_unsafe_adapter_proof_catalog(errors: list[str]) -> None:
    if not UNSAFE_ADAPTER_PROOF.is_file():
        errors.append("missing tooling/configs/qa/unsafe-ffi-adapter-proof.json")
        return
    try:
        payload = json.loads(UNSAFE_ADAPTER_PROOF.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        errors.append(f"invalid unsafe adapter proof catalog JSON: {error}")
        return
    if payload.get("formatVersion") != 1:
        errors.append("unsafe adapter proof catalog formatVersion must be 1")
    adapters = payload.get("adapters")
    if not isinstance(adapters, list):
        errors.append("unsafe adapter proof catalog adapters must be a list")
        return
    catalog_paths: set[str] = set()
    for index, adapter in enumerate(adapters):
        if not isinstance(adapter, dict):
            errors.append(f"unsafe adapter proof entry {index} must be an object")
            continue
        path = adapter.get("path")
        if not isinstance(path, str) or not path:
            errors.append(f"unsafe adapter proof entry {index} missing path")
            continue
        if path in catalog_paths:
            errors.append(f"duplicate unsafe adapter proof entry: {path}")
        catalog_paths.add(path)
        if path not in UNSAFE_ADAPTER_ALLOWLIST:
            errors.append(f"{path} has unsafe proof but is not in UNSAFE_ADAPTER_ALLOWLIST")
        for field in ("owner", "boundary", "safeWrapper", "raiiWrapper"):
            value = adapter.get(field)
            if not isinstance(value, str) or not value.strip():
                errors.append(f"{path} unsafe proof missing non-empty {field}")
        for field in ("invariants", "shutdownCancelProof", "reviews"):
            value = adapter.get(field)
            if (
                not isinstance(value, list)
                or not value
                or any(not isinstance(item, str) or not item.strip() for item in value)
            ):
                errors.append(f"{path} unsafe proof missing non-empty {field} list")
                continue
            if field in {"invariants", "shutdownCancelProof"}:
                for item in value:
                    reference = proof_reference_path(item)
                    if reference is not None and not (ROOT / reference).is_file():
                        errors.append(f"{path} unsafe proof references missing file: {reference}")
        reviews = set(adapter.get("reviews", [])) if isinstance(adapter.get("reviews"), list) else set()
        if not {"security", "architecture"}.issubset(reviews):
            errors.append(f"{path} unsafe proof must require security and architecture reviews")
    missing_proof = sorted(UNSAFE_ADAPTER_ALLOWLIST - catalog_paths)
    extra_proof = sorted(catalog_paths - UNSAFE_ADAPTER_ALLOWLIST)
    if missing_proof:
        errors.append("unsafe allowlist missing proof entries: " + ", ".join(missing_proof))
    if extra_proof:
        errors.append("unsafe proof catalog has non-allowlisted entries: " + ", ".join(extra_proof))
    for relative in sorted(UNSAFE_ADAPTER_ALLOWLIST):
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"unsafe allowlist entry no longer exists: {relative}")
            continue
        text = read(path)
        if "#![allow(unsafe_code)]" not in text:
            errors.append(f"{relative} allowlist entry must explicitly opt into unsafe_code")
        if not re.search(r"\bunsafe\s*\{|\bunsafe\s+fn\b|\bunsafe\s+impl\b", text):
            errors.append(f"{relative} allowlist entry no longer contains unsafe code; lower the baseline")


def check_required_security_docs(errors: list[str]) -> None:
    for doc_path, terms in REQUIRED_SECURITY_DOCS.items():
        path = ROOT / doc_path
        if not path.is_file():
            errors.append(f"missing {doc_path}")
            continue
        text = path.read_text(encoding="utf-8")
        missing = [term for term in terms if term not in text]
        if missing:
            errors.append(f"{doc_path} missing required term(s): " + ", ".join(missing))
    if not REDACTION_CORPUS.is_file():
        errors.append("missing contracts/security/redaction-corpus.json")
    if not OPERATION_ERROR_CATALOG.is_file():
        errors.append("missing contracts/native/operation-error-catalog.json")


def check_pipe_secret_auth_algorithm_drift(errors: list[str]) -> None:
    install_secret = SRC / "install" / "pipe_secret.rs"
    ipc_mac = SRC / "ipc" / "pipe" / "auth_mac.rs"
    smoke = ROOT / "tooling" / "release" / "windows" / "smoke" / "windows-smoke.ps1"
    expected = "pipe-auth-v2-hmac-sha256"
    for path in [install_secret, ipc_mac, smoke]:
        if not path.is_file():
            errors.append(f"missing {rel(path)}")
            continue
        if expected not in read(path):
            errors.append(f"{rel(path)} must use pipe auth algorithm {expected}")


def has_non_scaffold_code(prefix: str) -> bool:
    base = SRC / prefix
    if not base.exists():
        return False
    for path in base.rglob("*.rs"):
        if "/tests/" in path.as_posix():
            continue
        for raw in path.read_text(encoding="utf-8").splitlines():
            stripped = raw.strip()
            if stripped and not stripped.startswith("//!") and not stripped.startswith("mod "):
                return True
    return False


def has_non_scaffold_role(path: Path) -> bool:
    if not path.is_file():
        return False
    for raw in path.read_text(encoding="utf-8").splitlines():
        stripped = raw.strip()
        if stripped and not stripped.startswith("//!") and not stripped.startswith("#![") and not stripped.startswith("mod "):
            return True
    return False


def check_high_risk_test_anchors(errors: list[str]) -> None:
    required_when_implemented = {
        "assets": "recovery",
        "diagnostics": "redaction",
        "telemetry": "redaction",
        "install": "trust",
    }
    for owner, marker in required_when_implemented.items():
        if not has_non_scaffold_code(owner):
            continue
        tests_dir = SRC / owner / "tests"
        has_anchor = any(marker in path.name for path in tests_dir.rglob("*.rs"))
        if not has_anchor:
            errors.append(f"src/{owner} implementation requires owner-local {marker} test")

    ipc_dir = SRC / "ipc"
    if not ipc_dir.exists():
        return
    tests_dir = ipc_dir / "tests"
    native_messaging = ipc_dir / "native_messaging.rs"
    if has_non_scaffold_role(native_messaging) and not any(
        "native_messaging" in path.name or "framing" in path.name for path in tests_dir.rglob("*.rs")
    ):
        errors.append("src/ipc/native_messaging.rs implementation requires owner-local framing test")

    auth_roles = [
        ipc_dir / "pipe" / "server.rs",
        ipc_dir / "pipe" / "client.rs",
        ipc_dir / "controller_lease.rs",
    ]
    if any(has_non_scaffold_role(path) for path in auth_roles) and not any(
        "auth" in path.name for path in tests_dir.rglob("*.rs")
    ):
        errors.append("src/ipc pipe/controller implementation requires owner-local auth test")


def check_authenticated_pipe_forwarding(errors: list[str]) -> None:
    bridge = SRC / "ipc" / "bridge.rs"
    handshake = SRC / "ipc" / "pipe" / "handshake.rs"
    if bridge.exists():
        handshake_text = read(handshake) if handshake.exists() else ""
        if "authenticate_client_pipe" not in handshake_text:
            errors.append("src/ipc/bridge.rs requires authenticated pipe handshake role")

    app_dir = SRC / "app"
    if not app_dir.exists():
        return
    for path in app_dir.rglob("*.rs"):
        if path.name == "tests.rs" or "/tests/" in path.as_posix():
            continue
        if re.search(r"\bforward_native_host_stream\s*\(", read(path)):
            errors.append(
                f"{rel(path)} must not call raw pipe forwarder; use authenticated IPC bridge API"
            )


def check_windows_pipe_instance_lifecycle(errors: list[str]) -> None:
    pipe = SRC / "ipc" / "pipe" / "windows_pipe.rs"
    overlapped = SRC / "ipc" / "pipe" / "windows_pipe_overlapped.rs"
    if not pipe.exists():
        return
    text = read(pipe)
    if "FILE_FLAG_FIRST_PIPE_INSTANCE" in text:
        errors.append(
            "src/ipc/pipe/windows_pipe.rs must not use FILE_FLAG_FIRST_PIPE_INSTANCE; "
            "resident pipe listeners are recreated across native-host sessions"
        )
    if "PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED" not in text:
        errors.append(
            "src/ipc/pipe/windows_pipe.rs must create resident pipe handles with "
            "FILE_FLAG_OVERLAPPED to avoid split-handle duplex deadlocks"
        )
    if "FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED" not in text:
        errors.append(
            "src/ipc/pipe/windows_pipe.rs must open native-host pipe client handles with "
            "FILE_FLAG_OVERLAPPED to avoid split-handle duplex deadlocks"
        )
    if not overlapped.exists():
        errors.append("missing src/ipc/pipe/windows_pipe_overlapped.rs")
        return
    overlapped_text = read(overlapped)
    for required in ("ReadFile", "WriteFile", "GetOverlappedResult", "CancelIoEx"):
        if required not in overlapped_text:
            errors.append(
                "src/ipc/pipe/windows_pipe_overlapped.rs must own "
                f"{required} for cancellable duplex pipe IO"
            )


def check_pipe_auth_and_backpressure_hardening(errors: list[str]) -> None:
    pipe = SRC / "ipc" / "pipe" / "windows_pipe.rs"
    pipe_peer = SRC / "ipc" / "pipe" / "windows_pipe_peer.rs"
    server = SRC / "ipc" / "pipe" / "server.rs"
    bridge_runtime = SRC / "app" / "bridge" / "runtime.rs"
    resident_pipe = SRC / "app" / "resident" / "pipe_windows.rs"
    resident_pipe_peer = SRC / "app" / "resident" / "pipe_peer.rs"
    resident_pipe_consent = SRC / "app" / "resident" / "pipe_peer" / "consent.rs"
    install_dev_local_trust = SRC / "install" / "dev_local_trust" / "mod.rs"
    install_program_files = SRC / "install" / "windows" / "program_files.rs"
    windows_smoke = ROOT / "tooling" / "release" / "windows" / "smoke" / "windows-smoke.ps1"
    session_handshake = SRC / "app" / "bridge" / "session" / "duplex" / "handshake.rs"
    session_reader = SRC / "app" / "bridge" / "session" / "duplex" / "reader.rs"

    if pipe.exists():
        pipe_text = read(pipe)
        if "PIPE_REJECT_REMOTE_CLIENTS" not in pipe_text:
            errors.append("src/ipc/pipe/windows_pipe.rs must reject remote named-pipe clients")

    if pipe_peer.exists():
        pipe_peer_text = read(pipe_peer)
        for term in (
            "GetNamedPipeClientProcessId",
            "GetNamedPipeClientSessionId",
            "QueryFullProcessImageNameW",
            "CreateToolhelp32Snapshot",
            "TH32CS_SNAPPROCESS",
            "th32ParentProcessID",
        ):
            if term not in pipe_peer_text:
                errors.append(f"src/ipc/pipe/windows_pipe_peer.rs missing connected peer identity term {term}")

    if server.exists():
        server_text = read(server)
        for term in ("USED_SERVER_NONCE_LIMIT", "used_server_nonces.remove(0)", "remember_server_nonce"):
            if term not in server_text:
                errors.append(f"src/ipc/pipe/server.rs missing bounded replay-cache term {term}")

    if bridge_runtime.exists():
        runtime_text = read(bridge_runtime)
        if "random_pipe_auth_bytes::<16>()" not in runtime_text:
            errors.append("src/app/bridge/runtime.rs must derive pipe client nonce from OS RNG")
        if "verify_current_runtime_manifest_identity()" not in read(SRC / "app" / "bridge" / "windows_runtime.rs"):
            errors.append("src/app/bridge/windows_runtime.rs must verify install identity before pipe connect")
        client_nonce_body = re.search(
            r"fn\s+bridge_client_nonce\s*\([^)]*\)\s*->\s*Result<[\s\S]+?\n}\n",
            runtime_text,
        )
        if client_nonce_body and re.search(r"SystemTime|UNIX_EPOCH|process::id", client_nonce_body.group(0)):
            errors.append("src/app/bridge/runtime.rs pipe client nonce must not use PID/time material")

    if resident_pipe.exists():
        resident_text = read(resident_pipe)
        for term in (
            "connected_client_identity()",
            "verify_connected_pipe_peer",
            "same_windows_path",
            "browser_parent_fingerprint",
            "AgentPipeConnectionContext",
            "host_exe_sha256",
            "return Ok(AgentPipeAccept::Pending)",
            "AgentPipeAccept::Rejected",
            "resident_pipe.peer_identity_unavailable",
            "resident_pipe.peer_identity_rejected",
        ):
            if term not in resident_text:
                errors.append(f"src/app/resident/pipe_windows.rs missing peer executable identity term {term}")

    if resident_pipe_peer.exists():
        pipe_peer_text = read(resident_pipe_peer)
        for term in (
            "current_program_files_roots",
            "current_user_local_app_data_roots",
            "dev_local_browser_trust_enabled",
            "ALLOWED_BROWSER_PARENT_EXECUTABLES",
            "TRUSTED_CHROMIUM_PARENT_SUFFIXES",
            "TRUSTED_EDGE_PARENT_SUFFIX",
            "TRUSTED_EDGE_VERSIONED_PARENT_MARKER",
            "DEV_LOCAL_CHROMIUM_PARENT_SUFFIX",
            r"\google\chrome\application\chrome.exe",
            r"\microsoft\edge\application\msedge.exe",
            r"\chromium\application\chrome.exe",
            r"\chromium\application\chromium.exe",
            "is_under_trusted_root",
            "ParentProcessFingerprint",
        ):
            if term not in pipe_peer_text:
                errors.append(f"src/app/resident/pipe_peer.rs missing trusted browser parent provenance term {term}")
        if "env::var_os" in pipe_peer_text or "TRUSTED_PROGRAM_FILES_ENV_VARS" in pipe_peer_text:
            errors.append(
                "src/app/resident/pipe_peer.rs must not trust inherited environment for browser parent roots"
            )
        if "file_name()" in pipe_peer_text and "is_under_trusted_root" not in pipe_peer_text:
            errors.append(
                "src/app/resident/pipe_peer.rs must not authorize browser parents by basename only"
            )
    elif resident_pipe.exists():
        errors.append("src/app/resident/pipe_windows.rs requires src/app/resident/pipe_peer.rs provenance policy")

    if resident_pipe_consent.exists():
        consent_text = read(resident_pipe_consent)
        for term in (
            "trusted-connections.json",
            "safe_extension_label",
            "connection_consent_required",
            "ConnectionConsentPromptResult::Timeout",
            "ConnectionConsentPromptResult::UiUnavailable",
        ):
            if term not in consent_text:
                errors.append(f"src/app/resident/pipe_peer/consent.rs missing consent trust term {term}")
    elif resident_pipe.exists():
        errors.append("src/app/resident/pipe_windows.rs requires resident pipe consent store")

    if install_dev_local_trust.exists():
        dev_local_trust_text = read(install_dev_local_trust)
        for term in (
            "DEV_LOCAL_BROWSER_TRUST_FILE",
            "dev-local-browser-trust.json",
            "sniptale-dev-local-browser-trust",
            "current-user-browser-parent",
            "SNIPTALE_DEV_LOCAL_BROWSER_TRUST",
            "executable_sha256",
            "std::env::current_exe",
            "file_sha256_hex",
        ):
            if term not in dev_local_trust_text:
                errors.append(f"src/install/dev_local_trust/mod.rs missing dev-local browser trust term {term}")

    if install_program_files.exists():
        program_files_text = read(install_program_files)
        for term in (
            "current_program_files_roots",
            "PROGRAM_FILES_REGISTRY_SUBKEY",
            "ProgramFilesDir",
            "ProgramFilesDir (x86)",
            "ProgramW6432Dir",
            "RegistryHive::LocalMachine",
        ):
            if term not in program_files_text:
                errors.append(f"src/install/windows/program_files.rs missing OS-backed Program Files root term {term}")

    if windows_smoke.exists():
        windows_smoke_text = read(windows_smoke)
        for term in (
            "Write-DevLocalBrowserTrust",
            "Remove-DevLocalBrowserTrust",
            "dev-local-browser-trust.json",
            "sniptale-dev-local-browser-trust",
            "SNIPTALE_DEV_LOCAL_BROWSER_TRUST",
            "executableSha256",
            "Get-Sha256Hex",
        ):
            if term not in windows_smoke_text:
                errors.append(f"tooling/release/windows/smoke/windows-smoke.ps1 missing dev-local browser trust term {term}")

    if session_handshake.exists():
        duplex_text = read(session_handshake)
        for term in (
            "authenticate_shared_server_pipe_with_timeout",
            "handshake-timeout",
            "resident_pipe.handshake_timeout",
            "shutdown.shutdown_agent_read()",
        ):
            if term not in duplex_text:
                errors.append(f"src/app/bridge/session/duplex/handshake.rs missing handshake timeout term {term}")

    if session_reader.exists():
        reader_text = read(session_reader)
        enqueue_index = reader_text.find("enqueue_forwarded_message()")
        read_index = reader_text.find("read_forwarded_payload_with_permit")
        complete_index = reader_text.find("complete_forwarded_message()")
        if enqueue_index < 0 or read_index < 0 or enqueue_index > read_index:
            errors.append("src/app/bridge/session/duplex/reader.rs must acquire backpressure permit before payload read")
        if complete_index < 0:
            errors.append("src/app/bridge/session/duplex/reader.rs must release backpressure permit after forwarded message handling")


def check_collect_diagnostics_bundle_contract(errors: list[str]) -> None:
    if not APP_HOST.is_file():
        errors.append("missing src/app/host.rs")
        return
    if not APP_SUPPORT.is_file():
        errors.append("missing src/app/support.rs")
        return
    if not DIAGNOSTICS_BUNDLE.is_file():
        errors.append("missing src/diagnostics/bundle.rs")
        return

    host_text = read(APP_HOST)
    support_text = read(APP_SUPPORT)
    bundle_text = read(DIAGNOSTICS_BUNDLE)
    if "bundle-planned" in host_text or "bundle-planned" in support_text:
        errors.append("--collect-diagnostics must write a real bundle, not return bundle-planned")
    if (
        "write_collect_diagnostics_json" not in host_text
        or "write_runtime_bundle" not in support_text
    ):
        errors.append("--collect-diagnostics must call the diagnostics bundle writer")

    required_exclusions = [
        "BundleEntryKind::RawLog",
        "BundleEntryKind::MediaPayload",
        "BundleEntryKind::TempMedia",
        "BundleEntryKind::TextInputBuffer",
    ]
    missing_exclusions = [term for term in required_exclusions if term not in bundle_text]
    if missing_exclusions:
        errors.append(
            "diagnostics bundle must exclude sensitive entry categories: "
            + ", ".join(missing_exclusions)
        )


def check_operation_failure_error_taxonomy(errors: list[str]) -> None:
    outbound = SRC / "app" / "responses" / "outbound.rs"
    if not outbound.is_file():
        errors.append("missing src/app/responses/outbound.rs")
        return
    outbound_text = read(outbound)
    for term in ("kind: OperationalErrorKind", "descriptor.public_code", "descriptor.recoverable"):
        if term not in outbound_text:
            errors.append(f"src/app/responses/outbound.rs missing operational taxonomy term {term}")

    responses_dir = SRC / "app" / "responses"
    response_files = [SRC / "app" / "responses.rs"]
    response_files.extend(responses_dir.rglob("*.rs"))
    for path in response_files:
        if "/tests/" in path.as_posix():
            continue
        text = read(path)
        if "NativeAppErrorCode::Unknown" in text:
            errors.append(f"{rel(path)} must not collapse operation failures to Unknown")


def business_critical_silent_failure_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for relative, patterns in BUSINESS_CRITICAL_SILENT_FAILURE_PATTERNS.items():
        path = root / relative
        if not path.is_file():
            offenders.append(f"missing {relative}")
            continue
        text = read(path)
        for pattern, reason in patterns.items():
            if re.search(pattern, text):
                offenders.append(f"{relative}: {reason}")
    return offenders


def check_business_critical_silent_failures(errors: list[str]) -> None:
    errors.extend(business_critical_silent_failure_errors(ROOT))


def assert_contains(values: list[str], needle: str) -> None:
    if not any(needle in value for value in values):
        raise AssertionError(f"expected {needle!r} in {values!r}")


def run_self_tests() -> int:
    if proof_reference_path("src/telemetry/tests/hook_policy.rs covers hooks") != (
        "src/telemetry/tests/hook_policy.rs"
    ):
        raise AssertionError("proof reference parser must extract repo-relative paths")
    if proof_reference_path("Drop unhooks the low-level hook") is not None:
        raise AssertionError("proof reference parser must ignore prose-only proof notes")

    with tempfile.TemporaryDirectory(prefix="sniptale-security-") as temp_dir:
        root = Path(temp_dir)
        write_selftest_file(
            root / "src/install/windows/dpapi.rs",
            """
            use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};
            fn allowed() { let _ = CRYPT_INTEGER_BLOB::default(); let _ = CryptUnprotectData; }
            """,
        )
        write_selftest_file(
            root / "src/app/bad_dpapi.rs",
            """
            use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData};
            fn bad() { let _ = CRYPT_INTEGER_BLOB::default(); let _ = CryptProtectData; }
            """,
        )
        dpapi_errors = raw_dpapi_adapter_errors(root)
        assert_contains(dpapi_errors, "src/app/bad_dpapi.rs")
        if any("src/install/windows/dpapi.rs" in error for error in dpapi_errors):
            raise AssertionError(f"allowlisted DPAPI adapter was rejected: {dpapi_errors!r}")

        write_selftest_file(
            root / "src/install/windows/dpapi.rs",
            """
            use std::ptr;
            use windows::Win32::{Foundation::LocalFree, Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData}};
            enum DpapiSecretError { EmptyInput, InputTooLarge, InvalidOutput }
            struct LocalBlob;
            pub(crate) fn unprotect_current_user_secret(input: &[u8]) -> Result<Vec<u8>, DpapiSecretError> {
                if input.is_empty() { return Err(DpapiSecretError::EmptyInput); }
                let _ = u32::try_from(input.len()).map_err(|_| DpapiSecretError::InputTooLarge)?;
                let _ = CryptUnprotectData;
                let _ = CRYPT_INTEGER_BLOB::default();
                let _ = LocalBlob;
                let _ = DpapiSecretError::InvalidOutput;
                let _ = ptr::write_bytes::<u8>;
                let _ = LocalFree;
                Ok(Vec::new())
            }
            """,
        )
        write_selftest_file(
            root / "src/transfer/windows_dpapi.rs",
            """
            use std::ptr;
            use windows::{core::w, Win32::{Foundation::LocalFree, Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData}}};
            enum MediaDpapiError { EmptyInput, InputTooLarge, InvalidOutput }
            struct LocalBlob;
            pub(crate) fn protect_current_user_media_key(input: &[u8]) -> Result<Vec<u8>, MediaDpapiError> {
                if input.is_empty() { return Err(MediaDpapiError::EmptyInput); }
                let _ = u32::try_from(input.len()).map_err(|_| MediaDpapiError::InputTooLarge)?;
                let _ = w!("Sniptale temp media key");
                let _ = CryptProtectData;
                let _ = CRYPT_INTEGER_BLOB::default();
                let _ = MediaDpapiError::InvalidOutput;
                let _ = ptr::write_bytes::<u8>;
                let _ = LocalFree;
                Ok(Vec::new())
            }
            pub(crate) fn unprotect_current_user_media_key(_: &[u8]) -> Result<Vec<u8>, MediaDpapiError> {
                let _ = CryptUnprotectData;
                let _ = LocalBlob;
                Ok(Vec::new())
            }
            """,
        )
        dpapi_owner_errors = dpapi_owner_invariant_errors(root)
        if dpapi_owner_errors:
            raise AssertionError(f"valid DPAPI owner fixtures failed: {dpapi_owner_errors!r}")
        write_selftest_file(
            root / "src/install/windows/dpapi.rs",
            """
            use windows::Win32::Security::Cryptography::CryptProtectData;
            fn protect_current_user_secret() { let _ = CryptProtectData; }
            """,
        )
        dpapi_install_errors = dpapi_owner_invariant_errors(root)
        assert_contains(dpapi_install_errors, "install DPAPI adapter must stay unwrap-only")
        write_selftest_file(
            root / "src/transfer/windows_dpapi.rs",
            """
            use std::ptr;
            use windows::Win32::{Foundation::LocalFree, Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData}};
            enum MediaDpapiError { EmptyInput, InputTooLarge, InvalidOutput }
            struct LocalBlob;
            pub(crate) fn protect_current_user_media_key(input: &[u8]) -> Result<Vec<u8>, MediaDpapiError> {
                if input.is_empty() { return Err(MediaDpapiError::EmptyInput); }
                let _ = u32::try_from(input.len()).map_err(|_| MediaDpapiError::InputTooLarge)?;
                let _ = CryptProtectData;
                let _ = CRYPT_INTEGER_BLOB::default();
                let _ = MediaDpapiError::InvalidOutput;
                let _ = ptr::write_bytes::<u8>;
                let _ = LocalFree;
                Ok(Vec::new())
            }
            pub(crate) fn unprotect_current_user_media_key(_: &[u8]) -> Result<Vec<u8>, MediaDpapiError> {
                let _ = CryptUnprotectData;
                let _ = LocalBlob;
                Ok(Vec::new())
            }
            """,
        )
        dpapi_transfer_errors = dpapi_owner_invariant_errors(root)
        assert_contains(dpapi_transfer_errors, "media-key purpose string")

        write_selftest_file(
            root / "tooling/configs/qa/unsafe-ffi-adapter-proof.json",
            """
            {
              "formatVersion": 1,
              "adapters": [
                {
                  "path": "src/install/windows/dpapi.rs",
                  "owner": "install",
                  "boundary": "DPAPI current-user unwrap-only adapter for pipe-secret protection",
                  "invariants": ["No protect API is exposed by the install DPAPI adapter"],
                  "safeWrapper": "Install owner receives unwrapped pipe-secret bytes.",
                  "raiiWrapper": "LocalBlob",
                  "shutdownCancelProof": ["Drop wipes secret blobs before LocalFree"],
                  "reviews": ["security", "architecture"]
                },
                {
                  "path": "src/transfer/windows_dpapi.rs",
                  "owner": "transfer",
                  "boundary": "DPAPI current-user wrapping for per-media protected temp media keys",
                  "invariants": ["Unwrapped media-key bytes stay in transfer-owned encryption lifecycle"],
                  "safeWrapper": "Transfer owner receives protected media-key blobs.",
                  "raiiWrapper": "LocalBlob",
                  "shutdownCancelProof": ["Drop wipes secret blobs before LocalFree"],
                  "reviews": ["security", "architecture"]
                }
              ]
            }
            """,
        )
        dpapi_proof_errors = unsafe_dpapi_proof_errors(root)
        if dpapi_proof_errors:
            raise AssertionError(f"valid DPAPI proof fixture failed: {dpapi_proof_errors!r}")
        write_selftest_file(
            root / "tooling/configs/qa/unsafe-ffi-adapter-proof.json",
            """
            {
              "formatVersion": 1,
              "adapters": [
                {
                  "path": "src/install/windows/dpapi.rs",
                  "owner": "install",
                  "boundary": "DPAPI current-user pipe-secret protection owner",
                  "invariants": ["DPAPI output blobs are LocalFree-owned"],
                  "safeWrapper": "Install owner receives secret bytes.",
                  "raiiWrapper": "LocalBlob",
                  "shutdownCancelProof": ["Drop wipes secret blobs before LocalFree"],
                  "reviews": ["security", "architecture"]
                },
                {
                  "path": "src/transfer/windows_dpapi.rs",
                  "owner": "transfer",
                  "boundary": "DPAPI current-user wrapping for per-media protected temp media keys",
                  "invariants": ["Unwrapped media-key bytes stay in transfer-owned encryption lifecycle"],
                  "safeWrapper": "Transfer owner receives protected media-key blobs.",
                  "raiiWrapper": "LocalBlob",
                  "shutdownCancelProof": ["Drop wipes secret blobs before LocalFree"],
                  "reviews": ["security", "architecture"]
                }
              ]
            }
            """,
        )
        dpapi_stale_proof_errors = unsafe_dpapi_proof_errors(root)
        assert_contains(dpapi_stale_proof_errors, "unsafe proof missing DPAPI ownership term")

        write_selftest_file(
            root / "src/app/support.rs",
            'fn current() { let _ = "{\\"recentErrors\\":[]}"; }',
        )
        write_selftest_file(
            root / "src/diagnostics/bad_recent_errors.rs",
            'fn bad() { let _ = "{\\"recentErrors\\":[]}"; }',
        )
        recent_errors = placeholder_recent_errors_ratchet_errors(root)
        assert_contains(recent_errors, "src/diagnostics/bad_recent_errors.rs")
        assert_contains(recent_errors, "src/app/support.rs")

        write_selftest_file(
            root / "src/app/support.rs",
            """
            fn current() {
                let _ = "{\\"recentErrors\\":[]}";
                let _ = "{\\"recentErrors\\":[]}";
            }
            """,
        )
        recent_growth = placeholder_recent_errors_ratchet_errors(root)
        assert_contains(recent_growth, "src/app/support.rs")

        for relative in BUSINESS_CRITICAL_SILENT_FAILURE_PATTERNS:
            write_selftest_file(root / relative, "fn ok() {}")
        write_selftest_file(
            root / "src/app/host.rs",
            """
            fn logger() {
                let _ = start_process_logger_for_mode_with_options().ok();
            }
            """,
        )
        write_selftest_file(
            root / "src/app/resident/transfer.rs",
            """
            fn cleanup(store: Store) {
                let _ = store.cleanup_expired();
            }
            """,
        )
        write_selftest_file(
            root / "src/telemetry/runtime.rs",
            """
            fn drop_warning(collector: Collector) {
                let _ = self.collector.record_telemetry_warning();
            }
            """,
        )
        silent_failure_errors = business_critical_silent_failure_errors(root)
        assert_contains(silent_failure_errors, "src/app/host.rs")
        assert_contains(silent_failure_errors, "src/app/resident/transfer.rs")
        assert_contains(silent_failure_errors, "src/telemetry/runtime.rs")

    print("OK security guardrail self-tests")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return run_self_tests()
    errors: list[str] = []
    check_required_security_docs(errors)
    check_pipe_secret_auth_algorithm_drift(errors)
    check_forbidden_runtime_patterns(errors)
    check_sensitive_logging_markers(errors)
    check_audit11_raw_dpapi_does_not_expand_allowlist(errors)
    check_audit11_dpapi_owner_invariants(errors)
    check_audit11_dpapi_unsafe_proof(errors)
    check_audit11_placeholder_recent_errors_does_not_expand_baseline(errors)
    check_unsafe_policy(errors)
    check_unsafe_adapter_proof_catalog(errors)
    check_high_risk_test_anchors(errors)
    check_authenticated_pipe_forwarding(errors)
    check_windows_pipe_instance_lifecycle(errors)
    check_pipe_auth_and_backpressure_hardening(errors)
    check_collect_diagnostics_bundle_contract(errors)
    check_operation_failure_error_taxonomy(errors)
    check_business_critical_silent_failures(errors)
    if errors:
        fail(errors)
    print("OK security guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
