#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import tempfile
from collections import Counter
from pathlib import Path

try:
    from .exception_registry import path_terms
except ImportError:
    from exception_registry import path_terms


ROOT = Path(__file__).resolve().parents[3]

REQUIRED_FILES = [
    "Cargo.toml",
    "rust-toolchain.toml",
    "AGENTS.md",
    "docs/README.md",
    "docs/architecture/repository-overview.md",
    "docs/architecture/module-ownership.md",
    "docs/architecture/adr/0001-resident-media-worker-ownership.md",
    "docs/architecture/adr/0002-dpapi-owner-local-adapters.md",
    "docs/architecture/adr/0003-transfer-lifecycle-ownership.md",
    "docs/architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md",
    "docs/engineering/implementation-rules.md",
    "docs/engineering/testing-policy.md",
    "docs/engineering/unsafe-ffi-policy.md",
    "docs/security/data-handling.md",
    "docs/security/ipc-auth.md",
    "docs/security/install-trust.md",
    "docs/security/threat-model.md",
    "docs/tooling/qa.md",
    "docs/oss/provenance.md",
    "docs/oss/release.md",
    "contracts/native/fixture-requirements.json",
    "contracts/security/redaction-corpus.json",
    "tooling/configs/qa/unsafe-ffi-adapter-proof.json",
    "deny.toml",
    ".config/nextest.toml",
    "tooling/configs/qa/tool-versions.json",
]

REQUIRED_OWNER_DOC_TERMS = [
    "protocol/**",
    "ipc/**",
    "settings/**",
    "assets/**",
    "tray/**",
    "hotkeys/**",
    "capture/**",
    "recording/**",
    "audio/**",
    "telemetry/**",
    "transfer/**",
    "install/**",
    "diagnostics/**",
]

REQUIRED_ARCHITECTURE_DECISION_DOC_TERMS = {
    "docs/architecture/adr/0001-resident-media-worker-ownership.md": [
        "AssetPublisher",
        "CaptureWorker",
        "EncoderWorker",
        "TelemetryWorker",
        "TransferWorker",
        "sync_channel(1)",
        "tooling/qa/rules/verify_architecture.py",
    ],
    "docs/architecture/adr/0002-dpapi-owner-local-adapters.md": [
        "src/install/windows/dpapi.rs",
        "src/transfer/windows_dpapi.rs",
        "tooling/qa/rules/verify_security.py",
        "tooling/configs/qa/unsafe-ffi-adapter-proof.json",
    ],
    "docs/architecture/adr/0003-transfer-lifecycle-ownership.md": [
        "TransferLifecycleSnapshot",
        "src/transfer/lifecycle/mod.rs",
        "tooling/qa/rules/verify_architecture.py",
        "tooling/qa/rules/verify_security.py",
    ],
    "docs/architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md": [
        "OperatingMode",
        "ModeEpoch",
        "ExtensionTransfer",
        "LocalAsset",
        "ShellActionId",
        "ExtensionIntentActionId",
        "assets/**",
    ],
}

PROTOCOL_FORBIDDEN_PATTERNS = [
    r"\bWin32\b",
    r"\bWindows Graphics Capture\b",
    r"\bWGC\b",
    r"\bWASAPI\b",
    r"\bMedia Foundation\b",
    r"\bHWND\b",
    r"\bHMONITOR\b",
    r"\bHKEY\b",
    r"\bHKCU\b",
    r"\bnamed pipe\b",
    r"\bregistry\b",
]

WINDOWS_API_ALLOWED_PREFIXES = (
    "src/capture/",
    "src/audio/",
    "src/recording/",
    "src/telemetry/",
    "src/tray/",
    "src/hotkeys/",
    "src/install/",
    "src/ipc/",
    "src/diagnostics/windows/",
)

WINDOWS_API_ALLOWED_FILES = {
    "src/diagnostics/log/windows_exception.rs",
    "src/transfer/windows_dpapi.rs",
}

CONTROL_PIPE_FILES = [
    "src/ipc/bridge.rs",
    "src/ipc/bridge/authenticated.rs",
    "src/app/bridge/session.rs",
    "src/app/bridge/session/duplex.rs",
    "src/app/bridge/session/duplex/reader.rs",
    "src/app/bridge/session/duplex/writer.rs",
    "src/app/bridge/runtime.rs",
    "src/app/bridge/windows_runtime.rs",
]

COMMAND_RESPONSE_FILES = [
    "src/app/responses/command.rs",
    "src/app/responses/effects.rs",
]

MEDIA_COMMAND_PRODUCER_FILES = [
    "src/capture/screenshot.rs",
    "src/recording/service.rs",
    "src/recording/service/finalize.rs",
    "src/recording/service/progress_tick.rs",
]

TYPED_COMMAND_EVENT_FILES = [
    "src/app/resident/command_events.rs",
    "src/capture/service_contract.rs",
    "src/capture/screenshot.rs",
    "src/recording/service_contract.rs",
    "src/recording/service.rs",
    "src/recording/service/finalize.rs",
    "src/recording/service/progress_tick.rs",
]

CONTROL_PIPE_FORBIDDEN_DATA_BUS_PATTERNS = [
    r"\bbase64\b",
    r"\bmedia\b",
    r"\bchunk(?:s|ed|ing)?\b",
    r"\btransfer\s+store\b",
    r"\bscreenshot\s+bytes\b",
    r"\brecording\s+bytes\b",
    r"\baudio\s+bytes\b",
]

COMMAND_RESPONSE_FORBIDDEN_MEDIA_PATTERNS = [
    r"\bVec\s*<\s*WireMessage\s*>",
    r"json!\(\s*message\s*\)",
    r"json!\(\s*WireMessage::App(?:Screenshot|Recording)Chunk",
    r"\bWireMessage::App(?:Screenshot|Recording)Chunk\b",
    r"\b(?:screenshot|recording)_chunk\s*\(",
    r'"type"\s*:\s*"app\.(?:screenshot|recording)\.chunk"',
    r"\bbase64\s*:",
]

MEDIA_COMMAND_PUSH_PATTERNS = [
    r"\bWireMessage::App(?:Screenshot|Recording)Chunk\b",
    r"\bscreenshot_chunk_message_from_encoded\b",
]

TYPED_COMMAND_EVENT_FORBIDDEN_PATTERNS = [
    r"\bVec\s*<\s*WireMessage\s*>",
    r"\bWireMessage\b",
]

NATIVE_HOST_TYPED_OUTBOUND_FILES = [
    "src/app/responses.rs",
    "src/app/responses/command.rs",
    "src/app/responses/controller.rs",
    "src/app/responses/ping.rs",
    "src/app/responses/reconnect.rs",
    "src/app/responses/settings_sync.rs",
    "src/app/responses/transfer.rs",
]

NATIVE_HOST_STRINGLY_OUTBOUND_PATTERNS = [
    r"\bjson!\s*\(",
    r"\bWireMessage::App(?:CommandAccepted|SettingsAccepted|OperationFailed|ScreenshotStart|ScreenshotChunk|ScreenshotCommit|RecordingStarted|RecordingProgress|RecordingStopped|RecordingChunk|ControllerLease|Pong)\b",
    r'"type"\s*:\s*"app\.(?:command\.accepted|settings\.accepted|operation\.failed|screenshot\.start|screenshot\.chunk|screenshot\.commit|recording\.started|recording\.progress|recording\.stopped|recording\.chunk|controller\.lease|pong)"',
]

SETTINGS_SYNC_FILES = [
    "src/app/responses/settings_sync.rs",
]

SETTINGS_SYNC_FORBIDDEN_PATTERNS = [
    r"\blet\s+_\s*=\s*effects\.apply_(?:hotkey_plan|tray_registry)\b",
    r"\beffects\.apply_(?:hotkey_plan|tray_registry)\([^;]+;\s*(?:\*settings_store|hotkey_state\.commit)",
]

SETTINGS_EFFECTIVE_REQUIRED = {
    "src/settings/mod.rs": [
        "pub(crate) mod effective;",
        "pub(crate) use effective::effective_settings_for_sync;",
    ],
    "src/settings/effective.rs": [
        "pub(crate) fn effective_settings_for_sync",
        "pub(crate) fn effective_recording_quality",
        "DEFERRED_EVALUATION_WARNING_CODE",
    ],
    "src/recording/quality.rs": [
        "effective_recording_quality",
        "QualityLimits as RecordingQualityLimits",
        "SettingsError as RecordingQualityError",
    ],
    "src/app/responses/outbound.rs": [
        "effective_settings_for_sync",
        "let warnings = effective_settings.warnings.clone();",
    ],
}

SETTINGS_EFFECTIVE_FORBIDDEN = {
    "src/settings/mod.rs": [
        r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+effective\s*;",
    ],
    "src/recording/quality.rs": [
        r"\bfn\s+effective_fps\b",
        r"\bfn\s+effective_video_bitrate\b",
        r"\bfn\s+effective_encoder\b",
        r"\bstruct\s+BitratePreset\b",
        r"\bimpl\s+NativeVideoQuality\b",
        r"\bvideo_bitrate_mbps_override[\s\S]{0,200}\.clamp\s*\(\s*2\.0\s*,\s*80\.0\s*\)",
    ],
    "src/app/responses/outbound.rs": [
        r"settings_accepted[\s\S]{0,900}warnings\s*:\s*Vec::new\s*\(",
        r"settings_accepted[\s\S]{0,900}warnings:\s*\[\s*\]",
    ],
}

TRAY_APPLY_ACK_REQUIRED = {
    "src/app/resident.rs": [
        "ApplyTrayRegistry",
        "result_sender: SyncSender<Result<(), crate::tray::TrayRuntimeError>>",
    ],
    "src/app/resident/tray.rs": [
        "result_sender.send(result)",
        "source.apply_render_plan(",
    ],
    "src/app/resident/tray/sink.rs": [
        "recv_timeout(RUNTIME_APPLY_ACK_TIMEOUT)",
        "WorkerCommand::ApplyTrayRegistry",
    ],
}

STARTUP_DEGRADED_REQUIRED = {
    "src/app/resident/startup.rs": [
        "AgentStartupStatus",
        "RepairRequired",
        "UnsupportedCacheSchema",
        "RepairRequiredAndUnsupportedCacheSchema",
        "inputs.active_settings",
        "allows_extension_ingress",
        "inputs.install_repair_required",
        "inputs.cache_schema_supported",
    ],
    "src/app/resident.rs": [
        "startup_status: AgentStartupStatus",
        "startup_status: plan.status",
    ],
    "src/app/support/doctor.rs": [
        "runtime_agent_startup_plan",
    ],
    "src/app/support/doctor/startup.rs": [
        "DoctorCheckName::RuntimeStartup",
        "startup_doctor_status",
        "startup_doctor_detail",
    ],
    "src/tray/icon/state.rs": [
        "TrayStartupStatus",
        "startup_status: TrayStartupStatus",
        "UnsupportedCacheSchema",
        "RepairRequiredAndUnsupportedCacheSchema",
    ],
    "src/app/resident/tray.rs": [
        "startup_status: AgentStartupStatus",
        "tray_startup_status",
        "tray_icon_tooltip",
        "tray_icon_variant",
        "source.apply_render_plan(",
    ],
    "src/tray/runtime/windows.rs": [
        "tooltip: &str",
        "WindowsTrayIcon::add(",
        "icon.update_tooltip(tooltip)",
    ],
}

STARTUP_DEGRADED_FORBIDDEN = {
    "src/app/resident/startup.rs": [
        r"\bbrowser_connected\b",
        r"\bstarts_disconnected\b",
    ],
    "src/tray/icon/state.rs": [
        r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:#\[[^\]]+\]\s*)*pub\(crate\)\s+enum\s+TrayStartupStatus",
    ],
    "src/tray/runtime/windows.rs": [
        r"WindowsTrayIcon::add\([\s\S]{0,300}\"Sniptale\"",
    ],
}

BASE64_ENCODING_ALLOWED_PREFIXES = ("src/transfer/",)

BASE64_ENCODING_PATTERNS = {
    r"\bbase64::": "imports base64 encoder outside transfer owner",
    r"\bSTANDARD\s*\.\s*encode\s*\(": "encodes base64 outside transfer owner",
}

APP_TRANSFER_READ_ALLOWED_FILES: set[str] = {
    "src/app/resident/effects/transfer.rs",
}

APP_TRANSFER_READ_PATTERNS = {
    r"\.\s*(?:screenshot|recording)_chunk(?:_at)?\s*\(": (
        "reads media chunks directly from app outside typed transfer side-effect dispatch"
    ),
}

PROTECTED_FINALIZE_REQUIRED = {
    "src/capture/screenshot.rs": [
        "protect_screenshot_media(capture_id)",
        "media_protection: Some(media_protection)",
    ],
    "src/recording/transfer_store/lifecycle.rs": [
        "protect_recording_media(&media_path)",
        "Some(media_protection)",
    ],
}

PROTECTED_CHUNK_STORE_REQUIRED = {
    "src/capture/transfer_store/service.rs": [
        "None => Err(TransferError::ProtectedMedia)",
        "read_protected_media_chunk",
    ],
    "src/recording/transfer_store/mod.rs": [
        "None => Err(TransferError::ProtectedMedia)",
        "read_protected_media_chunk",
    ],
}

PROTECTED_CHUNK_STORE_FORBIDDEN = {
    r"\bfile::encode_file_chunk\b": "falls back to plaintext chunk encoding",
    r"\breject_protected_file_without_key\b": "allows plaintext media when protected key metadata is missing",
}

TRANSFER_PENDING_POLICY_REQUIRED = {
    "src/transfer/mod.rs": [
        "#[cfg(any(windows, test))]",
        "pub(crate) mod pending;",
        "pub(crate) mod lifecycle;",
    ],
    "src/transfer/lifecycle/mod.rs": [
        "TransferLifecyclePolicy",
        "TransferLifecycleSnapshot",
        "TransferRecoveryDecision",
        "cleanup_recovery_decision",
        "PendingTransfer",
        "PendingTransfer::resume",
        "recovery_decision",
        "record_chunk_read",
        "acknowledge_for_cleanup",
        "reject_for_terminal",
    ],
    "src/capture/transfer_store/maintenance.rs": [
        "cleanup_recovery_decision",
    ],
    "src/recording/transfer_store/maintenance.rs": [
        "cleanup_recovery_decision",
    ],
    "src/recording/temp_media/disk.rs": [
        "cleanup_recovery_decision",
        "delete_recording_media",
    ],
    "src/recording/transfer_store/mod.rs": [
        "cleanup_recovery_decision",
        "self.delete_recording(recording_id)?",
    ],
}

TRANSFER_PENDING_POLICY_FORBIDDEN = {
    "src/transfer/mod.rs": [
        r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*pub\(crate\)\s+mod\s+pending\s*;",
    ],
    "src/capture/transfer_record.rs": [
        r"\benum\s+RecoveryDecision\b",
        r"\bexpire_decision\s*\(",
    ],
    "src/recording/temp_media/state.rs": [
        r"\benum\s+StartupRecoveryDecision\b",
        r"\bexpire_decision\s*\(",
    ],
    "src/capture/transfer_store/maintenance.rs": [
        r"delete_writing\s*&&\s*record\.is_writing\(\)",
    ],
    "src/recording/transfer_store/maintenance.rs": [
        r"delete_writing\s*&&\s*item\.record\.is_writing\(\)",
    ],
    "src/recording/temp_media/disk.rs": [
        r"item\.decision\s*==\s*StartupRecoveryDecision::Delete",
    ],
    "src/recording/transfer_store/mod.rs": [
        r"if\s+decision\s*==\s*StartupRecoveryDecision::ReportPending",
    ],
}

TRANSFER_LIFECYCLE_OWNER_FORBIDDEN_FILES = [
    "src/capture/transfer_record/lifecycle.rs",
    "src/recording/temp_media/state/lifecycle.rs",
]

TRANSFER_IDENTITY_REQUIRED = {
    "src/transfer/mod.rs": [
        "pub(crate) mod identity;",
    ],
    "src/transfer/identity.rs": [
        "derived_media_id",
        "validate_media_id",
        "validate_media_filename",
        "validate_sha256_hex",
        "media_path",
        "TransferMediaFileKind",
        "is_reserved_device_name",
        "contains(['/', '\\\\', ':'])",
    ],
    "src/capture/transfer_record/validation.rs": [
        "validate_media_id",
        "validate_media_filename",
        "validate_sha256_hex",
    ],
    "src/recording/temp_media/state/validation.rs": [
        "validate_transfer_media_id",
        "validate_sha256_hex",
    ],
    "src/recording/temp_media/metadata.rs": [
        "validate_transfer_media_filename",
    ],
    "src/capture/transfer_disk.rs": [
        "media_path",
        "TransferMediaFileKind::Png",
        "TransferMediaFileKind::Json",
        "TransferMediaFileKind::JsonTemp",
    ],
    "src/recording/temp_media/disk.rs": [
        "media_path",
        "TransferMediaFileKind::Mp4",
        "TransferMediaFileKind::Json",
        "TransferMediaFileKind::JsonTemp",
    ],
    "src/recording/service/source.rs": [
        "derived_media_id",
    ],
}

TRANSFER_IDENTITY_FORBIDDEN = {
    "src/capture/transfer_record/validation.rs": [
        r"\bstrip_suffix\s*\(\s*\"\\.png\"",
        r"\bis_ascii_hexdigit\b",
    ],
    "src/recording/temp_media/state/validation.rs": [
        r"\bis_ascii_hexdigit\b",
        r"\bbyte\.is_ascii_alphanumeric\b",
    ],
    "src/recording/temp_media/metadata.rs": [
        r"\brsplit_once\s*\(",
        r"\beq_ignore_ascii_case\s*\(\s*\"mp4\"",
    ],
    "src/capture/transfer_disk.rs": [
        r"\.join\s*\(\s*format!\s*\(",
    ],
    "src/recording/temp_media/disk.rs": [
        r"\.join\s*\(\s*format!\s*\(",
    ],
    "src/recording/service/source.rs": [
        r"format!\s*\(\s*\"recording-\{command_id\}\"",
    ],
}

MEDIA_POLICY_DOCS = [
    "contracts/native/README.md",
    "contracts/native/contract-index.json",
    "docs/security/ipc-auth.md",
    "docs/security/data-handling.md",
    "docs/architecture/module-ownership.md",
]

MEDIA_CONTRACT_DOCS = [
    "contracts/native/README.md",
    "contracts/native/contract-index.json",
]

COMMAND_RESPONSE_MEDIA_CONTRADICTIONS = [
    r"\bcommand[- ]responses?\b.{0,80}\b(?:deliver|return|send|carry|include)\w*\b.{0,80}\b(?:media bytes|base64|chunks?|payload)\b",
    r"\b(?:media bytes|base64|chunks?|payload)\b.{0,80}\b(?:deliver|return|send|carry|include)\w*\b.{0,80}\bcommand[- ]responses?\b",
    r"\bcommand/control\b.{0,80}\b(?:deliver|return|send|carry|include)\w*\b.{0,80}\b(?:media bytes|base64|chunks?|payload)\b",
    r"\b(?:media bytes|base64|chunks?|payload)\b.{0,80}\b(?:deliver|return|send|carry|include)\w*\b.{0,80}\bcommand/control\b",
]

TRANSPORT_AGNOSTIC_OWNER_PREFIXES = (
    "src/capture/",
    "src/recording/",
    "src/audio/",
    "src/telemetry/",
    "src/transfer/",
    "src/tray/",
    "src/hotkeys/",
    "src/settings/",
    "src/diagnostics/",
    "src/install/",
)

TRANSPORT_FORBIDDEN_PATTERNS = {
    r"\bcrate\s*::\s*ipc\b": "imports ipc transport owner",
    r"\bcrate\s*::\s*\{[^;]*\bipc\s*::": "imports ipc transport owner",
    r"\bcrate\s*::\s*\{[^;]*\bipc\s*(?:as\s+\w+)?\s*[,}]": "imports ipc transport owner",
    r"\bcrate\s*::\s*app\s*::\s*(?:bridge|resident::pipe_broker|resident::pipe_windows|native_host|agent_pipe|agent_pipe_server)\b": (
        "imports app transport coordinator"
    ),
    r"\bcrate\s*::\s*app\s*(?:as\s+\w+)?\s*;": "imports app transport coordinator",
    r"\bcrate\s*::\s*\{[^;]*\bapp\s*::\s*(?:bridge|resident::pipe_broker|resident::pipe_windows|native_host|agent_pipe|agent_pipe_server)\b": (
        "imports app transport coordinator"
    ),
    r"\bcrate\s*::\s*\{[^;]*\bapp\s*::\s*\{[^;]*(?:bridge|resident|native_host|agent_pipe|agent_pipe_server)\b": (
        "imports app transport coordinator"
    ),
    r"\bcrate\s*::\s*\{[^;]*\bapp\s*(?:as\s+\w+)?\s*[,}]": "imports app transport coordinator",
    r"\b(?:super\s*::)+\s*(?:ipc|app)\s*::": "uses parent-relative transport owner path",
    r"\bapp\s*::\s*(?:bridge|resident::pipe_broker|resident::pipe_windows|native_host|agent_pipe|agent_pipe_server)\b": (
        "uses app transport coordinator path"
    ),
    r"\bNativeMessageFrame\b": "uses Native Messaging frame type",
    r"\bAgentPipe(?:Client|Server|Session|Frame)\b": "uses resident pipe transport type",
}

HOT_PATH_SLEEP_ALLOWLIST = path_terms("architecture.hot-path-sleep")

CONTROL_BLOCKING_RECV_PREFIXES = (
    "src/app/bridge/",
    "src/app/resident/pipe_broker/",
    "src/ipc/bridge/",
)

CONTROL_BLOCKING_RECV_FILES = {
    "src/ipc/bridge.rs",
}

# AUD11 final ratchets. These stay at zero so removed scaffold paths cannot reappear.
AUD11_MEDIA_NOOP_WORKER_BASELINE = set()
AUD11_NON_MEDIA_WORKER_EXEMPTIONS = {"TransferWorker"}
AUD11_DIRECT_APP_MEDIA_EXECUTION_BASELINE = {}
AUD11_APP_MEDIA_WORKER_EXECUTION_FILES = {
    "src/app/resident/capture_worker.rs",
    "src/app/resident/recording_worker.rs",
    "src/app/resident/telemetry_worker.rs",
}
AUD11_DEFERRED_EVALUATION_ALLOWED_FILES = {
    "src/settings/effective.rs",  # target: Wave 1 removes placeholder producer
    "src/protocol/validation/warnings.rs",  # public warning-code registry
}
SETTINGS_QUALITY_POLICY_ALLOWED_PREFIXES = (
    "src/settings/",
    "src/recording/quality.rs",
)
SETTINGS_QUALITY_POLICY_DUPLICATION_PATTERNS = {
    r"\bfn\s+effective_fps\b": "effective FPS policy must stay settings-owned",
    r"\bfn\s+effective_video_bitrate\b": "effective bitrate policy must stay settings-owned",
    r"\bfps_cap\s*\(": "FPS cap policy must stay settings-owned",
    r"\bbitrate_preset\s*\(": "bitrate preset policy must stay settings-owned",
    r"\bvideo_bitrate_mbps_override[\s\S]{0,200}\.clamp\s*\(": ("manual bitrate clamp policy must stay settings-owned"),
    r"\b2\.0\s*\.\.\s*=\s*80\.0\b": "video bitrate bounds must use settings-owned constants",
    r"\b96\s*\|\s*128\s*\|\s*160\s*\|\s*192\b": ("audio bitrate allowlist must use settings-owned constants"),
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write_selftest_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def fail(message: str) -> None:
    print(f"FAIL {message}")
    raise SystemExit(1)


def check_required_files() -> None:
    missing = [path for path in REQUIRED_FILES if not (ROOT / path).is_file()]
    if missing:
        fail("missing required files: " + ", ".join(missing))


def check_owner_docs() -> None:
    text = read(ROOT / "docs/architecture/module-ownership.md")
    missing = [term for term in REQUIRED_OWNER_DOC_TERMS if term not in text]
    if missing:
        fail("module ownership doc is missing terms: " + ", ".join(missing))


def check_architecture_decision_docs() -> None:
    errors: list[str] = []
    for relative, terms in REQUIRED_ARCHITECTURE_DECISION_DOC_TERMS.items():
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"missing {relative}")
            continue
        text = read(path)
        missing = [term for term in terms if term not in text]
        if missing:
            errors.append(f"{relative} missing required term(s): " + ", ".join(missing))
    if errors:
        fail("architecture decision docs are stale: " + "; ".join(errors))


def check_cargo_windows_dependency_scope() -> None:
    cargo = read(ROOT / "Cargo.toml")
    if "[target.'cfg(windows)'.dependencies]" not in cargo:
        fail("windows crate must be target-scoped under cfg(windows)")
    common_deps = cargo.split("[target.'cfg(windows)'.dependencies]", 1)[0]
    if re.search(r"(?m)^windows\s*=", common_deps):
        fail("windows crate must not be a cross-platform dependency")


def scan_text_files(prefix: str) -> list[Path]:
    base = ROOT / prefix
    if not base.exists():
        return []
    return [path for path in base.rglob("*") if path.is_file() and path.suffix in {".rs", ".md", ".toml"}]


def rust_files(root: Path) -> list[Path]:
    src = root / "src"
    if not src.exists():
        return []
    return [path for path in src.rglob("*.rs") if path.is_file()]


def production_rust_files(root: Path) -> list[Path]:
    return [path for path in rust_files(root) if "/tests/" not in path.as_posix() and path.name != "tests.rs"]


def check_protocol_neutrality() -> None:
    offenders: list[str] = []
    # Product protocol neutrality is a src/protocol/** rule. Tooling docs may describe Win32 or
    # registry behavior without becoming product protocol code.
    for path in scan_text_files("src/protocol"):
        text = read(path)
        for pattern in PROTOCOL_FORBIDDEN_PATTERNS:
            if re.search(pattern, text, flags=re.IGNORECASE):
                offenders.append(f"{rel(path)} matches {pattern}")
    if offenders:
        fail("protocol boundary is not platform-neutral: " + "; ".join(offenders))


def check_windows_api_imports() -> None:
    offenders: list[str] = []
    for path in (ROOT / "src").rglob("*.rs"):
        text = read(path)
        if "windows::" not in text:
            continue
        relative = rel(path)
        if relative not in WINDOWS_API_ALLOWED_FILES and not relative.startswith(WINDOWS_API_ALLOWED_PREFIXES):
            offenders.append(relative)
    if offenders:
        fail("windows crate imports outside approved owner modules: " + ", ".join(offenders))


NATIVE_HOST_STDOUT_GUARD_PATHS = (
    "src/ipc",
    "src/app/host.rs",
    "src/app/host",
    "src/app/bridge",
)


def native_host_stdout_smell_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    candidates: list[Path] = []
    for relative in NATIVE_HOST_STDOUT_GUARD_PATHS:
        path = root / relative
        if path.is_file():
            candidates.append(path)
        elif path.is_dir():
            candidates.extend(path.rglob("*.rs"))
    for path in candidates:
        text = read(path)
        if "println!" in text or "print!" in text:
            offenders.append(rel(path) if root == ROOT else path.relative_to(root).as_posix())
    return offenders


def check_no_native_host_stdout_smells() -> None:
    offenders = native_host_stdout_smell_errors(ROOT)
    if offenders:
        fail("native-host control paths must not print diagnostics to stdout: " + ", ".join(offenders))


def check_control_pipe_is_not_data_bus() -> None:
    offenders: list[str] = []
    for relative in CONTROL_PIPE_FILES:
        path = ROOT / relative
        if not path.is_file():
            offenders.append(f"{relative} is missing")
            continue
        text = read(path)
        for pattern in CONTROL_PIPE_FORBIDDEN_DATA_BUS_PATTERNS:
            if re.search(pattern, text, flags=re.IGNORECASE):
                offenders.append(f"{relative} matches {pattern}")
    offenders.extend(command_response_media_errors(ROOT))
    if offenders:
        fail("resident/native-host control pipe must not become a data bus: " + "; ".join(offenders))


def media_command_push_errors(
    root: Path,
    files: list[str] | None = None,
    require_files: bool = True,
) -> list[str]:
    offenders: list[str] = []
    for relative in files or MEDIA_COMMAND_PRODUCER_FILES:
        path = root / relative
        if not path.is_file():
            if require_files:
                offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for pattern in MEDIA_COMMAND_PUSH_PATTERNS:
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative} matches {pattern}")
    return offenders


def check_media_command_producers_do_not_push_chunks() -> None:
    offenders = media_command_push_errors(ROOT)
    if offenders:
        fail("capture/recording command producers must not push media chunks: " + "; ".join(offenders))


def typed_command_event_errors(
    root: Path,
    files: list[str] | None = None,
    require_files: bool = True,
) -> list[str]:
    offenders: list[str] = []
    for relative in files or TYPED_COMMAND_EVENT_FILES:
        path = root / relative
        if not path.is_file():
            if require_files:
                offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for pattern in TYPED_COMMAND_EVENT_FORBIDDEN_PATTERNS:
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative} matches {pattern}")
    return offenders


def check_command_events_are_typed_before_app_response() -> None:
    offenders = typed_command_event_errors(ROOT)
    if offenders:
        fail("command event plumbing must stay typed before app response: " + "; ".join(offenders))


def native_host_typed_outbound_errors(
    root: Path,
    files: list[str] | None = None,
    require_files: bool = True,
) -> list[str]:
    offenders: list[str] = []
    for relative in files or NATIVE_HOST_TYPED_OUTBOUND_FILES:
        path = root / relative
        if not path.is_file():
            if require_files:
                offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for pattern in NATIVE_HOST_STRINGLY_OUTBOUND_PATTERNS:
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative} matches {pattern}")
    return offenders


def check_native_host_responses_use_typed_outbound() -> None:
    offenders = native_host_typed_outbound_errors(ROOT)
    if offenders:
        fail("native-host app responses must use typed outbound builders: " + "; ".join(offenders))


def command_response_media_errors(
    root: Path,
    files: list[str] | None = None,
    require_files: bool = True,
) -> list[str]:
    offenders: list[str] = []
    for relative in files or COMMAND_RESPONSE_FILES:
        path = root / relative
        if not path.is_file():
            if require_files:
                offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for pattern in COMMAND_RESPONSE_FORBIDDEN_MEDIA_PATTERNS:
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative} matches {pattern}")
    return offenders


def media_policy_alignment_errors(root: Path) -> list[str]:
    docs: dict[str, str] = {}
    missing: list[str] = []
    for relative in MEDIA_POLICY_DOCS:
        path = root / relative
        if not path.is_file():
            missing.append(relative)
            continue
        docs[relative] = path.read_text(encoding="utf-8")
    if missing:
        return ["missing media policy docs: " + ", ".join(missing)]

    ipc_auth = docs["docs/security/ipc-auth.md"].lower()
    if "control channel" not in ipc_auth or "media bytes" not in ipc_auth:
        return []

    offenders: list[str] = []
    for relative in MEDIA_CONTRACT_DOCS:
        text = docs[relative]
        for pattern in COMMAND_RESPONSE_MEDIA_CONTRADICTIONS:
            if re.search(pattern, text, flags=re.I | re.S):
                offenders.append(f"{relative} contradicts media/control policy with {pattern}")
    return offenders


def check_media_policy_alignment() -> None:
    offenders = media_policy_alignment_errors(ROOT)
    if offenders:
        fail("media transfer policy docs are contradictory: " + "; ".join(offenders))


def settings_sync_drift_errors(
    root: Path,
    files: list[str] | None = None,
    require_files: bool = True,
) -> list[str]:
    offenders: list[str] = []
    for relative in files or SETTINGS_SYNC_FILES:
        path = root / relative
        if not path.is_file():
            if require_files:
                offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for pattern in SETTINGS_SYNC_FORBIDDEN_PATTERNS:
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative} matches {pattern}")
    return offenders


def check_settings_sync_does_not_ignore_runtime_side_effects() -> None:
    offenders = settings_sync_drift_errors(ROOT)
    if offenders:
        fail("settings-sync must not ignore runtime side effects: " + "; ".join(offenders))


def required_and_forbidden_file_errors(
    root: Path,
    required: dict[str, list[str]],
    forbidden: dict[str, list[str]] | None = None,
) -> list[str]:
    offenders: list[str] = []
    for relative, terms in required.items():
        path = root / relative
        if not path.is_file():
            offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for term in terms:
            if term not in text:
                offenders.append(f"{relative} missing required term {term!r}")
    for relative, patterns in (forbidden or {}).items():
        path = root / relative
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        for pattern in patterns:
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative} matches forbidden pattern {pattern}")
    return offenders


def settings_effective_policy_errors(root: Path) -> list[str]:
    return required_and_forbidden_file_errors(
        root,
        SETTINGS_EFFECTIVE_REQUIRED,
        SETTINGS_EFFECTIVE_FORBIDDEN,
    )


def check_settings_effective_policy_stays_settings_owned() -> None:
    offenders = settings_effective_policy_errors(ROOT)
    if offenders:
        fail("effective settings policy must stay settings-owned: " + "; ".join(offenders))


def quality_policy_duplication_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for path in production_rust_files(root):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if not relative.startswith("src/recording/"):
            continue
        if relative.startswith(SETTINGS_QUALITY_POLICY_ALLOWED_PREFIXES):
            continue
        text = path.read_text(encoding="utf-8")
        for pattern, reason in SETTINGS_QUALITY_POLICY_DUPLICATION_PATTERNS.items():
            if re.search(pattern, text, flags=re.S):
                offenders.append(f"{relative}: {reason}")
    return offenders


def check_recording_quality_policy_has_no_duplicate_owner() -> None:
    offenders = quality_policy_duplication_errors(ROOT)
    if offenders:
        fail("effective recording quality policy must not be duplicated: " + "; ".join(offenders))


def tray_apply_ack_errors(root: Path) -> list[str]:
    return required_and_forbidden_file_errors(root, TRAY_APPLY_ACK_REQUIRED)


def check_tray_apply_is_bounded_and_acked() -> None:
    offenders = tray_apply_ack_errors(ROOT)
    if offenders:
        fail("tray settings apply must be bounded and acked: " + "; ".join(offenders))


def startup_degraded_policy_errors(root: Path) -> list[str]:
    return required_and_forbidden_file_errors(
        root,
        STARTUP_DEGRADED_REQUIRED,
        STARTUP_DEGRADED_FORBIDDEN,
    )


def check_startup_degraded_policy_gates_operational_workers() -> None:
    offenders = startup_degraded_policy_errors(ROOT)
    if offenders:
        fail("resident startup degraded policy must gate operational workers: " + "; ".join(offenders))


def base64_media_encoding_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for path in rust_files(root):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if relative.startswith(BASE64_ENCODING_ALLOWED_PREFIXES):
            continue
        text = path.read_text(encoding="utf-8")
        for pattern, reason in BASE64_ENCODING_PATTERNS.items():
            if re.search(pattern, text):
                offenders.append(f"{relative} {reason}")
    return offenders


def check_base64_media_encoding_stays_transfer_owned() -> None:
    offenders = base64_media_encoding_errors(ROOT)
    if offenders:
        fail("media base64 encoding must stay transfer-owned: " + "; ".join(offenders))


def app_transfer_store_read_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    app = root / "src/app"
    if not app.exists():
        return offenders
    for path in app.rglob("*.rs"):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if relative in APP_TRANSFER_READ_ALLOWED_FILES:
            continue
        if "/tests/" in relative or relative.endswith("/tests.rs"):
            continue
        text = path.read_text(encoding="utf-8")
        for pattern, reason in APP_TRANSFER_READ_PATTERNS.items():
            if re.search(pattern, text):
                offenders.append(f"{relative} {reason}")
    return offenders


def check_app_reads_transfer_stores_only_via_typed_dispatch() -> None:
    offenders = app_transfer_store_read_errors(ROOT)
    if offenders:
        fail("app transfer reads must stay in typed side-effect dispatch: " + "; ".join(offenders))


def agent_components(root: Path) -> set[str]:
    path = root / "src/app/resident/startup.rs"
    if not path.is_file():
        return set()
    match = re.search(r"enum\s+AgentComponent\s*\{(?P<body>.*?)\n\s*\}", read(path), re.S)
    if not match:
        return set()
    return set(re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*,", match.group("body"), re.M))


def worker_components_handled_by_role(root: Path) -> set[str]:
    texts = []
    for relative in [
        "src/app/resident/workers.rs",
        "src/app/resident/worker_start.rs",
    ]:
        path = root / relative
        if path.is_file():
            texts.append(read(path))
    text = "\n".join(texts)
    handled = set(re.findall(r"component\s*==\s*AgentComponent::([A-Z][A-Za-z0-9_]*)", text))
    handled.update(re.findall(r"AgentComponent::([A-Z][A-Za-z0-9_]*)\s*=>", text))
    return handled


def media_noop_worker_components(root: Path) -> set[str]:
    candidates = {
        component
        for component in agent_components(root)
        if component.endswith("Worker") and component not in AUD11_NON_MEDIA_WORKER_EXEMPTIONS
    }
    return candidates - worker_components_handled_by_role(root)


def media_noop_worker_ratchet_errors(
    root: Path,
    baseline: set[str] | None = None,
) -> list[str]:
    baseline = AUD11_MEDIA_NOOP_WORKER_BASELINE if baseline is None else baseline
    noops = media_noop_worker_components(root)
    additions = sorted(noops - baseline)
    if not additions:
        return []
    return [
        "new media worker component(s) fall through run_basic_worker above AUD11 final "
        "baseline: " + ", ".join(additions)
    ]


def check_audit11_media_workers_do_not_expand_basic_worker_baseline() -> None:
    offenders = media_noop_worker_ratchet_errors(ROOT)
    if offenders:
        fail("AUD11 media worker baseline ratchet failed: " + "; ".join(offenders))


def direct_app_media_execution_counts(root: Path) -> Counter[tuple[str, str]]:
    counts: Counter[tuple[str, str]] = Counter()
    app = root / "src/app"
    if not app.exists():
        return counts
    for path in app.rglob("*.rs"):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if "/tests/" in relative or relative.endswith("/tests.rs"):
            continue
        if relative in AUD11_APP_MEDIA_WORKER_EXECUTION_FILES:
            continue
        text = path.read_text(encoding="utf-8")
        for line in text.splitlines():
            if ".capture_screenshot(" in line:
                counts[(relative, "capture_screenshot")] += 1
            if (
                ".start_recording(" in line
                and "effects.start_recording(" not in line
                and "self.recording.start_recording(" not in line
            ):
                counts[(relative, "start_recording")] += 1
    return counts


def direct_app_media_execution_ratchet_errors(
    root: Path,
    baseline: dict[tuple[str, str], int] | None = None,
) -> list[str]:
    baseline = AUD11_DIRECT_APP_MEDIA_EXECUTION_BASELINE if baseline is None else baseline
    counts = direct_app_media_execution_counts(root)
    offenders: list[str] = []
    for key, count in sorted(counts.items()):
        allowed = baseline.get(key, 0)
        if count > allowed:
            path, method = key
            offenders.append(f"{path} has {count} direct {method} call(s), above AUD11 final baseline {allowed}")
    return offenders


def check_audit11_app_media_execution_does_not_expand_baseline() -> None:
    offenders = direct_app_media_execution_ratchet_errors(ROOT)
    if offenders:
        fail("AUD11 direct app media execution baseline ratchet failed: " + "; ".join(offenders))


def deferred_evaluation_warning_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for path in production_rust_files(root):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if relative in AUD11_DEFERRED_EVALUATION_ALLOWED_FILES:
            continue
        if '"deferred-evaluation"' in path.read_text(encoding="utf-8"):
            offenders.append(f"{relative} produces deferred-evaluation outside settings-owned policy baseline")
    return offenders


def check_audit11_deferred_evaluation_does_not_expand_baseline() -> None:
    offenders = deferred_evaluation_warning_errors(ROOT)
    if offenders:
        fail("AUD11 deferred effective-settings baseline ratchet failed: " + "; ".join(offenders))


def native_message_control_limit_errors(root: Path) -> list[str]:
    versions_path = root / "src/protocol/versions.rs"
    native_messaging_tests = root / "src/ipc/tests/native_messaging.rs"
    offenders: list[str] = []
    if not versions_path.is_file():
        return ["src/protocol/versions.rs is missing"]
    versions = read(versions_path)
    required_terms = [
        "EXTENSION_TO_HOST_CONTROL_MAX_MESSAGE_BYTES: usize = 1_048_576",
        "EXTENSION_TO_HOST_MAX_MESSAGE_BYTES: usize =\n    EXTENSION_TO_HOST_CONTROL_MAX_MESSAGE_BYTES",
        "EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024",
    ]
    for term in required_terms:
        if term not in versions:
            offenders.append(f"src/protocol/versions.rs missing control-limit term {term!r}")

    for path in rust_files(root):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if "/tests/" in relative or relative.endswith("/tests.rs"):
            continue
        text = path.read_text(encoding="utf-8")
        if re.search(r"read_payload\s*\([^;]+EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES", text, re.S):
            offenders.append(f"{relative} reads control payloads with Chromium hard cap")

    if not native_messaging_tests.is_file():
        offenders.append("src/ipc/tests/native_messaging.rs is missing")
    elif "chromium_sized_control_frame_is_rejected_before_body_read" not in read(native_messaging_tests):
        offenders.append(
            "src/ipc/tests/native_messaging.rs must prove Chromium-sized control frames fail before body read"
        )
    return offenders


def check_native_message_control_limit_is_bounded() -> None:
    offenders = native_message_control_limit_errors(ROOT)
    if offenders:
        fail("Native Messaging control frame limit must stay bounded: " + "; ".join(offenders))


def app_transfer_chunk_dispatch_errors(root: Path) -> list[str]:
    chunks_path = root / "src/app/responses/transfer/chunks.rs"
    outbound_path = root / "src/app/responses/outbound.rs"
    transfer_messages_path = root / "src/transfer/messages.rs"
    offenders: list[str] = []
    if not chunks_path.is_file():
        return ["app transfer chunk policy is missing: src/app/responses/transfer/chunks.rs"]
    chunks_text = read(chunks_path)
    for term in (
        "AppScreenshotChunkMessage",
        "AppRecordingChunkMessage",
        "EncodedChunk",
        "screenshot_chunk_message_from_encoded",
        "recording_chunk_message_from_encoded",
        "base64",
        "WireMessage::AppScreenshotChunk",
        "WireMessage::AppRecordingChunk",
    ):
        if term in chunks_text:
            offenders.append(f"src/app/responses/transfer/chunks.rs must not build media chunk DTOs: {term}")
    for term in (
        "screenshot_transfer_chunk(",
        "recording_transfer_chunk(",
        "screenshot_chunk_value(",
        "recording_chunk_value(",
        "NativeHostTransferChunkError",
    ):
        if term not in chunks_text:
            offenders.append(f"src/app/responses/transfer/chunks.rs missing transfer chunk dispatch term {term!r}")
    for term in (
        "unsupported_transfer_channel_failure",
        "OperationalErrorKind::TransferChannelUnsupported",
    ):
        if term in chunks_text:
            offenders.append(f"src/app/responses/transfer/chunks.rs still uses obsolete unsupported chunk path: {term}")

    if outbound_path.is_file():
        outbound_text = read(outbound_path)
        for term in ("screenshot_chunk(", "recording_chunk(", "AppScreenshotChunkMessage", "AppRecordingChunkMessage"):
            if term in outbound_text:
                offenders.append(f"src/app/responses/outbound.rs must not expose media chunk builders: {term}")
    if not transfer_messages_path.is_file():
        offenders.append("src/transfer/messages.rs is missing transfer-owned chunk DTO builders")
    else:
        transfer_messages = read(transfer_messages_path)
        for term in (
            "AppScreenshotChunkMessage",
            "AppRecordingChunkMessage",
            "EncodedChunk",
            "WireMessage::AppScreenshotChunk",
            "WireMessage::AppRecordingChunk",
            "base64: chunk.base64",
        ):
            if term not in transfer_messages:
                offenders.append(f"src/transfer/messages.rs missing transfer-owned chunk builder term {term!r}")
    return offenders


def check_app_control_channel_rejects_media_chunk_dto_path() -> None:
    offenders = app_transfer_chunk_dispatch_errors(ROOT)
    if offenders:
        fail("app transfer chunk path must stay transfer-owned: " + "; ".join(offenders))


def hot_path_sleep_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for path in rust_files(root):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if "/tests/" in relative or relative.endswith("/tests.rs"):
            continue
        text = path.read_text(encoding="utf-8")
        for match in re.finditer(r"thread::sleep\s*\(([^)]*)\)", text):
            expression = match.group(1)
            allowed_terms = HOT_PATH_SLEEP_ALLOWLIST.get(relative, set())
            if any(term in expression for term in allowed_terms):
                continue
            offenders.append(f"{relative} uses thread::sleep without an allowlisted bounded wait constant")
    return offenders


def stale_hot_path_sleep_allowances(root: Path) -> list[str]:
    stale: list[str] = []
    for relative, terms in HOT_PATH_SLEEP_ALLOWLIST.items():
        path = root / relative
        if not path.is_file():
            stale.append(f"{relative} no longer exists")
            continue
        expressions = [
            match.group(1) for match in re.finditer(r"thread::sleep\s*\(([^)]*)\)", path.read_text(encoding="utf-8"))
        ]
        for term in terms:
            if not any(term in expression for expression in expressions):
                stale.append(f"{relative}:{term} no longer identifies a bounded thread::sleep")
    return stale


def check_hot_path_sleeps_are_allowlisted() -> None:
    offenders = hot_path_sleep_errors(ROOT) + stale_hot_path_sleep_allowances(ROOT)
    if offenders:
        fail("control/capture hot-path sleeps must stay explicit and bounded: " + "; ".join(offenders))


def control_blocking_recv_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    for path in rust_files(root):
        relative = rel(path) if root == ROOT else path.relative_to(root).as_posix()
        if "/tests/" in relative or relative.endswith("/tests.rs"):
            continue
        if relative not in CONTROL_BLOCKING_RECV_FILES and not any(
            relative.startswith(prefix) for prefix in CONTROL_BLOCKING_RECV_PREFIXES
        ):
            continue
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            if re.search(r"\.recv\s*\(\s*\)", line):
                offenders.append(
                    f"{relative}:{line_number} uses blocking recv() in a control-plane path; "
                    "use recv_timeout with an explicit bounded timeout"
                )
    return offenders


def check_control_plane_receives_are_bounded() -> None:
    offenders = control_blocking_recv_errors(ROOT)
    if offenders:
        fail("control-plane receives must stay bounded: " + "; ".join(offenders))


def protected_media_lifecycle_errors(root: Path) -> list[str]:
    offenders: list[str] = []
    offenders.extend(
        required_and_forbidden_file_errors(
            root,
            TRANSFER_PENDING_POLICY_REQUIRED,
            TRANSFER_PENDING_POLICY_FORBIDDEN,
        )
    )
    offenders.extend(
        required_and_forbidden_file_errors(
            root,
            TRANSFER_IDENTITY_REQUIRED,
            TRANSFER_IDENTITY_FORBIDDEN,
        )
    )
    for relative in TRANSFER_LIFECYCLE_OWNER_FORBIDDEN_FILES:
        if (root / relative).exists():
            offenders.append(f"{relative} must not own transfer lifecycle policy")
    for relative, required_terms in PROTECTED_FINALIZE_REQUIRED.items():
        path = root / relative
        if not path.is_file():
            offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for term in required_terms:
            if term not in text:
                offenders.append(f"{relative} missing required protected finalize term {term!r}")

    for relative, required_terms in PROTECTED_CHUNK_STORE_REQUIRED.items():
        path = root / relative
        if not path.is_file():
            offenders.append(f"{relative} is missing")
            continue
        text = path.read_text(encoding="utf-8")
        for term in required_terms:
            if term not in text:
                offenders.append(f"{relative} missing protected chunk-read term {term!r}")
        for pattern, reason in PROTECTED_CHUNK_STORE_FORBIDDEN.items():
            if re.search(pattern, text):
                offenders.append(f"{relative} {reason}")
    return offenders


def check_protected_media_lifecycle_is_fail_closed() -> None:
    offenders = protected_media_lifecycle_errors(ROOT)
    if offenders:
        fail("protected media lifecycle must fail closed: " + "; ".join(offenders))


def transport_agnostic_owner_errors(root: Path, owner_prefixes: tuple[str, ...]) -> list[str]:
    offenders: list[str] = []
    for prefix in owner_prefixes:
        base = root / prefix
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            text = path.read_text(encoding="utf-8")
            try:
                relative = path.relative_to(root).as_posix()
            except ValueError:
                relative = path.as_posix()
            for pattern, reason in TRANSPORT_FORBIDDEN_PATTERNS.items():
                if re.search(pattern, text, flags=re.S | re.M):
                    offenders.append(f"{relative} {reason}")
    return offenders


def check_os_owners_do_not_call_transports() -> None:
    offenders = transport_agnostic_owner_errors(ROOT, TRANSPORT_AGNOSTIC_OWNER_PREFIXES)
    if offenders:
        fail("media/OS owners must stay transport-agnostic: " + "; ".join(offenders))


def check_windows_pipe_worker_event_bridge() -> None:
    path = ROOT / "src/app/resident/pipe.rs"
    text = read(path)
    if "_agent_events" in text:
        fail("windows pipe worker must not discard AgentEventHub")
    if not re.search(
        r"\bserver\.run_with_effects_events_and_log\(\s*effects_factory,\s*&agent_events,\s*log\.as_ref\(\),",
        text,
        re.S,
    ):
        fail(
            "windows pipe worker must use event-aware session-factory pipe server path with process-log lifecycle proof"
        )
    if "let effects_factory =" not in text:
        fail("windows pipe worker must create per-session side-effect adapters")
    if re.search(r"\bserver\.run_with_effects\(&mut effects", text):
        fail("windows pipe worker must not use non-event pipe server path")


def check_resident_agent_startup_does_not_block_pipe_for_repair_state() -> None:
    path = ROOT / "src/app/resident/startup.rs"
    text = read(path)
    if "pub(crate) fn plan_agent_start" not in text:
        fail("resident agent startup plan function is missing")
    if "AgentComponent::PipeServer" not in text:
        fail("resident agent startup plan must gate PipeServer by operating mode")
    if "allows_extension_ingress" not in text:
        fail("resident startup must admit PipeServer only through active mode authority")
    if "AgentStartupStatus::Operational" not in text:
        fail("resident startup must expose typed operational/degraded status")


def check_agent_event_hub_keeps_concurrent_sessions() -> None:
    path = ROOT / "src/app/resident/events.rs"
    text = read(path)
    if "active_subscriber" in text:
        fail("agent event hub must not keep a single active subscriber")
    if "AGENT_EVENT_MAX_SUBSCRIBERS" not in text:
        fail("agent event hub must bound concurrent outbound subscribers")
    if "subscribers: Arc<Mutex<Vec<SyncSender<AgentOutboundEvent>>>>" not in text:
        fail("agent event hub must retain concurrent outbound subscribers")


def check_windows_pipe_allows_concurrent_instances() -> None:
    adapter_path = ROOT / "src/ipc/pipe/windows_pipe.rs"
    resident_path = ROOT / "src/app/resident/pipe_windows.rs"
    adapter_text = read(adapter_path)
    resident_text = read(resident_path)
    if "max_instances: u32" not in adapter_text:
        fail("windows pipe adapter must accept explicit max_instances")
    if re.search(r"CreateNamedPipeW\([\s\S]+?,\s*1\s*,", adapter_text):
        fail("windows pipe adapter must not hard-code one named-pipe instance")
    if "PIPE_MAX_SESSIONS as u32" not in resident_text:
        fail("resident pipe listener must pass session limit as named-pipe max instances")


def check_windows_native_host_uses_duplex_forwarding() -> None:
    path = ROOT / "src/app/bridge/runtime.rs"
    text = read(path)
    if "forward_authenticated_native_host_stream_duplex_with_authenticated_hook" not in text:
        fail("windows native-host runtime must use authenticated duplex pipe forwarding")
    if "forward_authenticated_native_host_stream_with_log_and_authenticated_hook" in text:
        fail("windows native-host runtime must not use turn-based pipe forwarding")
    if re.search(
        r"\bforward_authenticated_native_host_stream_polling_with_authenticated_hook\b",
        text,
    ):
        fail("windows native-host runtime must not use polling pipe forwarding")


def assert_contains(values: list[str], needle: str) -> None:
    if not any(needle in value for value in values):
        raise AssertionError(f"expected {needle!r} in {values!r}")


def run_self_tests() -> int:
    with tempfile.TemporaryDirectory(prefix="sniptale-architecture-") as temp_dir:
        root = Path(temp_dir)
        write_selftest_file(
            root / "src/app/host/native_host.rs",
            'fn bad() { println!("diagnostic leak"); }',
        )
        write_selftest_file(
            root / "src/app/support.rs",
            'fn support_stdout() { println!("support mode json"); }',
        )
        stdout_errors = native_host_stdout_smell_errors(root)
        assert_contains(stdout_errors, "src/app/host/native_host.rs")
        if any("src/app/support.rs" in error for error in stdout_errors):
            raise AssertionError(f"support-mode stdout was rejected: {stdout_errors!r}")

        write_selftest_file(
            root / "src/capture/direct.rs",
            "use crate::ipc::native_messaging;\nfn call() { let _ = NativeMessageFrame; }",
        )
        write_selftest_file(
            root / "src/recording/grouped.rs",
            "use crate::{ipc};\nfn call() { ipc::bridge(); }",
        )
        write_selftest_file(
            root / "src/audio/alias.rs",
            "use crate::{ipc as transport};\nfn call() { transport::bridge(); }",
        )
        write_selftest_file(
            root / "src/telemetry/app.rs",
            "use crate::app;\nfn call() { app::bridge::run(); }",
        )
        write_selftest_file(
            root / "src/transfer/nested_app.rs",
            "use crate::{app::{bridge}};\nfn call() { bridge::run(); }",
        )
        write_selftest_file(
            root / "src/tray/parent.rs",
            "fn call() { super::super::ipc::bridge(); }",
        )
        write_selftest_file(
            root / "src/hotkeys/pipe_type.rs",
            "struct Holder { frame: AgentPipeFrame }",
        )
        write_selftest_file(
            root / "src/install/allowed.rs",
            "use crate::protocol::WireMessage;\nfn call(message: WireMessage) { let _ = message; }",
        )

        errors = transport_agnostic_owner_errors(root, TRANSPORT_AGNOSTIC_OWNER_PREFIXES)
        assert_contains(errors, "src/capture/direct.rs imports ipc transport owner")
        assert_contains(errors, "src/recording/grouped.rs imports ipc transport owner")
        assert_contains(errors, "src/audio/alias.rs imports ipc transport owner")
        assert_contains(errors, "src/telemetry/app.rs imports app transport coordinator")
        assert_contains(errors, "src/transfer/nested_app.rs imports app transport coordinator")
        assert_contains(errors, "src/tray/parent.rs uses parent-relative transport owner path")
        assert_contains(errors, "src/hotkeys/pipe_type.rs uses resident pipe transport type")
        if any("src/install/allowed.rs" in error for error in errors):
            raise AssertionError(f"allowed protocol import was rejected: {errors!r}")

        command_file = "src/app/responses/command.rs"
        write_selftest_file(
            root / command_file,
            """
            use crate::protocol::WireMessage;
            fn serialize(message: WireMessage) {
                let _ = json!(message);
                let _messages: Vec<WireMessage> = Vec::new();
                let _chunk = WireMessage::AppScreenshotChunk;
                let _chunk_response = screenshot_chunk(screenshot);
            }
            """,
        )
        command_errors = command_response_media_errors(
            root,
            files=[command_file],
            require_files=False,
        )
        assert_contains(command_errors, "json!\\(\\s*message\\s*\\)")
        assert_contains(command_errors, "Vec\\s*<\\s*WireMessage\\s*>")
        assert_contains(command_errors, "WireMessage::App(?:Screenshot|Recording)Chunk")
        assert_contains(command_errors, "(?:screenshot|recording)_chunk")

        capture_file = "src/capture/screenshot.rs"
        write_selftest_file(
            root / capture_file,
            """
            fn capture() {
                let _ = WireMessage::AppScreenshotChunk;
                let _ = screenshot_chunk_message_from_encoded();
            }
            """,
        )
        push_errors = media_command_push_errors(
            root,
            files=[capture_file],
            require_files=False,
        )
        assert_contains(push_errors, "WireMessage::App(?:Screenshot|Recording)Chunk")
        assert_contains(push_errors, "screenshot_chunk_message_from_encoded")

        typed_event_file = "src/app/resident/command_events.rs"
        write_selftest_file(
            root / typed_event_file,
            """
            use crate::protocol::WireMessage;
            fn convert(messages: Vec<WireMessage>) {
                let _ = messages;
            }
            """,
        )
        typed_event_errors = typed_command_event_errors(
            root,
            files=[typed_event_file],
            require_files=False,
        )
        assert_contains(typed_event_errors, "Vec\\s*<\\s*WireMessage\\s*>")
        assert_contains(typed_event_errors, "\\bWireMessage\\b")

        outbound_file = "src/app/responses/command.rs"
        write_selftest_file(
            root / outbound_file,
            """
            fn serialize(message: WireMessage) {
                let _ = json!({ "type": "app.command.accepted" });
                let _ = WireMessage::AppRecordingStopped;
            }
            """,
        )
        outbound_errors = native_host_typed_outbound_errors(
            root,
            files=[outbound_file],
            require_files=False,
        )
        assert_contains(outbound_errors, "json!\\s*\\(")
        assert_contains(outbound_errors, "WireMessage::App")

        for relative in MEDIA_POLICY_DOCS:
            write_selftest_file(root / relative, "policy placeholder")
        write_selftest_file(
            root / "docs/security/ipc-auth.md",
            """
            The pipe is a control channel.
            Media bytes must stay out of command responses.
            """,
        )
        write_selftest_file(
            root / "contracts/native/README.md",
            "Command response must deliver screenshot chunks with base64 payload.",
        )
        policy_errors = media_policy_alignment_errors(root)
        assert_contains(policy_errors, "contracts/native/README.md")
        if any(".tmp/task" in error for error in policy_errors):
            raise AssertionError(f"architecture guardrail still depends on .tmp task docs: {policy_errors!r}")

        settings_file = "src/app/responses/settings_sync.rs"
        write_selftest_file(
            root / settings_file,
            """
            fn sync(effects: &mut Effects) {
                let _ = effects.apply_tray_registry(registry);
                effects.apply_hotkey_plan(plan);
                *settings_store = settings_store_update;
            }
            """,
        )
        settings_errors = settings_sync_drift_errors(
            root,
            files=[settings_file],
            require_files=False,
        )
        assert_contains(settings_errors, "let\\s+_\\s*=\\s*effects\\.apply_")
        assert_contains(settings_errors, "effects\\.apply_")

        write_selftest_file(
            root / "src/settings/mod.rs",
            """
            #[cfg(test)]
            mod effective;
            """,
        )
        write_selftest_file(
            root / "src/settings/effective.rs",
            """
            pub(crate) fn effective_settings_for_sync() {}
            """,
        )
        write_selftest_file(
            root / "src/recording/quality.rs",
            """
            fn effective_fps() {}
            fn effective_video_bitrate() {
                video_bitrate_mbps_override.clamp(2.0, 80.0);
            }
            struct BitratePreset;
            impl NativeVideoQuality {}
            """,
        )
        write_selftest_file(
            root / "src/app/responses/outbound.rs",
            """
            fn settings_accepted() -> AppSettingsAcceptedMessage {
                AppSettingsAcceptedMessage { warnings: Vec::new() }
            }
            """,
        )
        effective_errors = settings_effective_policy_errors(root)
        assert_contains(effective_errors, "src/settings/mod.rs matches forbidden pattern")
        assert_contains(effective_errors, "src/settings/effective.rs missing required term")
        assert_contains(effective_errors, "src/recording/quality.rs matches forbidden pattern")
        assert_contains(effective_errors, "src/app/responses/outbound.rs matches forbidden pattern")

        write_selftest_file(
            root / "src/recording/encoder.rs",
            """
            fn effective_video_bitrate(value: f64) -> f64 {
                value.clamp(2.0, 80.0)
            }
            fn allowed_audio(value: u16) -> bool {
                matches!(value, 96 | 128 | 160 | 192)
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/quality.rs",
            "pub(crate) use crate::settings::effective::MIN_VIDEO_BITRATE_MBPS;",
        )
        quality_errors = quality_policy_duplication_errors(root)
        assert_contains(quality_errors, "src/recording/encoder.rs")
        if any("src/recording/quality.rs" in error for error in quality_errors):
            raise AssertionError(f"thin quality adapter was rejected: {quality_errors!r}")

        write_selftest_file(
            root / "src/app/resident.rs",
            """
            enum WorkerCommand {
                ApplyTrayRegistry { registry: TrayActionRegistry },
            }
            """,
        )
        write_selftest_file(
            root / "src/app/resident/tray.rs",
            """
            fn submit(sender: Sender<WorkerCommand>) {
                sender.try_send(WorkerCommand::ApplyTrayRegistry { registry });
            }
            """,
        )
        tray_errors = tray_apply_ack_errors(root)
        assert_contains(tray_errors, "src/app/resident.rs missing required term")
        assert_contains(tray_errors, "src/app/resident/tray.rs missing required term")

        write_selftest_file(
            root / "src/app/resident/startup.rs",
            """
            struct AgentStartupInputs { browser_connected: bool }
            struct AgentStartupPlan { starts_disconnected: bool }
            pub(crate) fn plan_agent_start() -> AgentStartupPlan {
                AgentStartupPlan { starts_disconnected: true }
            }
            """,
        )
        write_selftest_file(
            root / "src/tray/icon/state.rs",
            """
            #[cfg(test)]
            pub(crate) enum TrayStartupStatus { Operational }
            """,
        )
        write_selftest_file(
            root / "src/tray/runtime/windows.rs",
            """
            fn add(icon: Icon) {
                WindowsTrayIcon::add(window, icon, id, callback, "Sniptale");
            }
            """,
        )
        startup_errors = startup_degraded_policy_errors(root)
        assert_contains(startup_errors, "src/app/resident/startup.rs matches forbidden pattern")
        assert_contains(startup_errors, "src/app/resident.rs missing required term")
        assert_contains(startup_errors, "src/tray/icon/state.rs matches forbidden pattern")
        assert_contains(startup_errors, "src/tray/runtime/windows.rs matches forbidden pattern")

        write_selftest_file(
            root / "src/app/media_base64.rs",
            """
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            fn encode(raw: &[u8]) -> String { STANDARD.encode(raw) }
            """,
        )
        write_selftest_file(
            root / "src/transfer/chunks.rs",
            """
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            fn encode(raw: &[u8]) -> String { STANDARD.encode(raw) }
            """,
        )
        base64_errors = base64_media_encoding_errors(root)
        assert_contains(base64_errors, "src/app/media_base64.rs imports base64 encoder")
        assert_contains(base64_errors, "src/app/media_base64.rs encodes base64")
        if any("src/transfer/chunks.rs" in error for error in base64_errors):
            raise AssertionError(f"transfer-owned base64 encoding was rejected: {base64_errors!r}")

        write_selftest_file(
            root / "src/app/responses/command.rs",
            """
            fn replay(store: &Store) {
                let _ = store.recording_chunk("recording-1", 0);
            }
            """,
        )
        write_selftest_file(
            root / "src/app/resident/effects/transfer.rs",
            """
            fn dispatch(store: &Store) {
                let _ = store.recording_chunk("recording-1", 0);
            }
            """,
        )
        transfer_read_errors = app_transfer_store_read_errors(root)
        assert_contains(transfer_read_errors, "src/app/responses/command.rs reads media chunks")
        if any("src/app/resident/effects/transfer.rs" in error for error in transfer_read_errors):
            raise AssertionError(f"typed resident side-effect dispatch was rejected: {transfer_read_errors!r}")

        write_selftest_file(
            root / "src/protocol/versions.rs",
            """
            pub(crate) const EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
            pub(crate) const EXTENSION_TO_HOST_MAX_MESSAGE_BYTES: usize =
                EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES;
            """,
        )
        write_selftest_file(
            root / "src/ipc/tests/native_messaging.rs",
            "fn oversized_frame_test_placeholder() {}",
        )
        write_selftest_file(
            root / "src/app/bad_limit.rs",
            """
            fn read(reader: &mut Reader) {
                let _ = read_payload(reader, EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES);
            }
            """,
        )
        limit_errors = native_message_control_limit_errors(root)
        assert_contains(limit_errors, "missing control-limit term")
        assert_contains(limit_errors, "reads control payloads with Chromium hard cap")
        assert_contains(limit_errors, "must prove Chromium-sized control frames fail before body read")

        write_selftest_file(
            root / "src/app/responses/transfer/chunks.rs",
            """
            use crate::protocol::AppScreenshotChunkMessage;
            fn chunk() {
                let _ = screenshot_chunk_message_from_encoded();
                let _ = "base64";
                let _ = OperationalErrorKind::TransferChannelUnsupported;
            }
            """,
        )
        write_selftest_file(
            root / "src/app/responses/outbound.rs",
            "fn screenshot_chunk() -> AppScreenshotChunkMessage { todo!() }",
        )
        write_selftest_file(
            root / "src/transfer/messages.rs",
            "fn missing_chunk_builders() {}",
        )
        chunk_dto_errors = app_transfer_chunk_dispatch_errors(root)
        assert_contains(chunk_dto_errors, "must not build media chunk DTOs")
        assert_contains(chunk_dto_errors, "missing transfer chunk dispatch term")
        assert_contains(chunk_dto_errors, "obsolete unsupported chunk path")
        assert_contains(chunk_dto_errors, "must not expose media chunk builders")
        assert_contains(chunk_dto_errors, "missing transfer-owned chunk builder term")

        write_selftest_file(
            root / "src/app/bridge/session/duplex.rs",
            "fn wait() { thread::sleep(Duration::from_millis(1)); }",
        )
        write_selftest_file(
            root / "src/app/resident/pipe_windows.rs",
            "fn wait() { thread::sleep(PIPE_ACCEPT_POLL_INTERVAL); }",
        )
        sleep_errors = hot_path_sleep_errors(root)
        assert_contains(sleep_errors, "src/app/bridge/session/duplex.rs uses thread::sleep")
        if any("src/app/resident/pipe_windows.rs" in error for error in sleep_errors):
            raise AssertionError(f"allowlisted bounded wait was rejected: {sleep_errors!r}")

        write_selftest_file(
            root / "src/app/bridge/session/duplex.rs",
            "fn wait(events: Receiver<Event>) { let _ = events.recv(); }",
        )
        write_selftest_file(
            root / "src/app/resident/pipe_broker/duplex/readiness.rs",
            "fn wait(ready: Receiver<Ready>) { let _ = ready.recv_timeout(SESSION_START_TIMEOUT); }",
        )
        write_selftest_file(
            root / "src/app/resident/effects/agent.rs",
            "fn wait(events: Receiver<Event>) { let _ = events.recv(); }",
        )
        recv_errors = control_blocking_recv_errors(root)
        assert_contains(recv_errors, "src/app/bridge/session/duplex.rs:1 uses blocking recv()")
        if any("src/app/resident/pipe_broker/duplex/readiness.rs" in error for error in recv_errors):
            raise AssertionError(f"bounded recv_timeout was rejected: {recv_errors!r}")
        if any("src/app/resident/effects/agent.rs" in error for error in recv_errors):
            raise AssertionError(f"out-of-scope worker receive was rejected: {recv_errors!r}")

        write_selftest_file(
            root / "src/transfer/mod.rs",
            """
            #[cfg(test)]
            pub(crate) mod pending;
            """,
        )
        write_selftest_file(
            root / "src/transfer/lifecycle/mod.rs",
            """
            pub(crate) struct TransferLifecycleSnapshot;
            """,
        )
        write_selftest_file(
            root / "src/transfer/identity.rs",
            """
            pub(crate) fn validate_media_id(id: &str) -> bool { !id.is_empty() }
            """,
        )
        write_selftest_file(
            root / "src/capture/transfer_record/lifecycle.rs",
            """
            enum RecoveryDecision { Delete }
            """,
        )
        write_selftest_file(
            root / "src/capture/transfer_record.rs",
            """
            enum RecoveryDecision { Delete }
            fn cleanup() { expire_decision(); }
            """,
        )
        write_selftest_file(
            root / "src/capture/transfer_record/validation.rs",
            """
            fn validate_filename(filename: &str) { filename.strip_suffix(".png"); }
            fn validate_sha256(sha: &str) { sha.bytes().all(|byte| byte.is_ascii_hexdigit()); }
            """,
        )
        write_selftest_file(
            root / "src/capture/transfer_disk.rs",
            """
            fn media_path(root: &Path, capture_id: &str) -> PathBuf {
                root.join(format!("{capture_id}.png"))
            }
            """,
        )
        write_selftest_file(
            root / "src/capture/transfer_store/maintenance.rs",
            """
            fn cleanup(record: &Record, delete_writing: bool) {
                if delete_writing && record.is_writing() {}
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/temp_media/state/lifecycle.rs",
            """
            fn pending_transfer_for_transfer() {}
            """,
        )
        write_selftest_file(
            root / "src/recording/temp_media/state.rs",
            """
            enum StartupRecoveryDecision { Delete }
            fn cleanup() { expire_decision(); }
            """,
        )
        write_selftest_file(
            root / "src/recording/temp_media/state/validation.rs",
            """
            fn validate_media_id(id: &str) { id.bytes().all(|byte| byte.is_ascii_alphanumeric()); }
            fn validate_sha256(sha: &str) { sha.bytes().all(|byte| byte.is_ascii_hexdigit()); }
            """,
        )
        write_selftest_file(
            root / "src/recording/temp_media/metadata.rs",
            """
            fn validate_filename(filename: &str) {
                let (_, extension) = filename.rsplit_once('.').unwrap();
                extension.eq_ignore_ascii_case("mp4");
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/temp_media/disk.rs",
            """
            fn media_path(root: &Path, media_id: &str) -> PathBuf {
                root.join(format!("{media_id}.mp4"))
            }
            fn cleanup(item: &Recovered) {
                if item.decision == StartupRecoveryDecision::Delete {}
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/transfer_store/maintenance.rs",
            """
            fn cleanup(item: &Recovered, delete_writing: bool) {
                if delete_writing && item.record.is_writing() {}
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/service/source.rs",
            """
            fn recording_id_for_command(command_id: &str) -> String {
                format!("recording-{command_id}")
            }
            """,
        )
        write_selftest_file(
            root / "src/capture/screenshot.rs",
            """
            fn finalize(store: &Store) {
                store.write_finalized_capture_at(FinalizedScreenshotTransfer {
                    media_protection: None,
                });
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/transfer_store/lifecycle.rs",
            """
            fn finalize(record: &mut Record) {
                record.finalize_recording_with_protection(bytes, hash, metadata, None, now);
            }
            """,
        )
        write_selftest_file(
            root / "src/capture/transfer_store/service.rs",
            """
            use crate::transfer::file::encode_file_chunk;
            fn read(path: &Path) {
                None => reject_protected_file_without_key(path).and_then(|()| encode_file_chunk());
            }
            """,
        )
        write_selftest_file(
            root / "src/recording/transfer_store/mod.rs",
            """
            use crate::transfer::file::encode_file_chunk;
            fn read(path: &Path) {
                None => reject_protected_file_without_key(path).and_then(|()| encode_file_chunk());
            }
            fn pending_record(decision: StartupRecoveryDecision) {
                if decision == StartupRecoveryDecision::ReportPending {}
            }
            """,
        )
        protected_errors = protected_media_lifecycle_errors(root)
        assert_contains(protected_errors, "src/capture/screenshot.rs missing required protected finalize term")
        assert_contains(
            protected_errors, "src/recording/transfer_store/lifecycle.rs missing required protected finalize term"
        )
        assert_contains(protected_errors, "src/capture/transfer_store/service.rs missing protected chunk-read term")
        assert_contains(protected_errors, "src/capture/transfer_store/service.rs falls back to plaintext")
        assert_contains(protected_errors, "src/capture/transfer_store/service.rs allows plaintext media")
        assert_contains(protected_errors, "src/recording/transfer_store/mod.rs missing protected chunk-read term")
        assert_contains(protected_errors, "src/transfer/mod.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/transfer/lifecycle/mod.rs missing required term")
        assert_contains(
            protected_errors,
            "src/capture/transfer_record/lifecycle.rs must not own transfer lifecycle policy",
        )
        assert_contains(
            protected_errors,
            "src/recording/temp_media/state/lifecycle.rs must not own transfer lifecycle policy",
        )
        assert_contains(protected_errors, "src/capture/transfer_record.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/temp_media/state.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/transfer/identity.rs missing required term")
        assert_contains(protected_errors, "src/capture/transfer_record/validation.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/capture/transfer_disk.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/temp_media/state/validation.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/temp_media/metadata.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/temp_media/disk.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/service/source.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/capture/transfer_store/maintenance.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/transfer_store/maintenance.rs matches forbidden pattern")
        assert_contains(protected_errors, "src/recording/transfer_store/mod.rs matches forbidden pattern")

        write_selftest_file(
            root / "src/app/resident/startup.rs",
            """
            enum AgentComponent {
                Tray,
                PipeServer,
                CaptureWorker,
                EncoderWorker,
                TransferWorker,
                ExperimentalMediaWorker,
            }
            """,
        )
        write_selftest_file(
            root / "src/app/resident/workers.rs",
            """
            fn start(component: AgentComponent) {
                if component == AgentComponent::CaptureWorker { return; }
                if component == AgentComponent::TransferWorker { return; }
                run_basic_worker(component);
            }
            """,
        )
        worker_errors = media_noop_worker_ratchet_errors(
            root,
            baseline=set(),
        )
        assert_contains(worker_errors, "EncoderWorker")
        assert_contains(worker_errors, "ExperimentalMediaWorker")

        write_selftest_file(
            root / "src/app/resident/capture_worker.rs",
            """
            fn worker_owned(runner: &mut Runner) {
                runner.capture_screenshot(command);
            }
            """,
        )
        write_selftest_file(
            root / "src/app/resident/effects.rs",
            """
            fn enqueue(recording: &mut RecordingSink) {
                self.recording.start_recording(command, policy);
            }
            """,
        )
        write_selftest_file(
            root / "src/app/responses.rs",
            """
            fn allowed(effects: &mut Effects) {
                effects.start_recording(command, policy);
            }
            """,
        )
        write_selftest_file(
            root / "src/app/direct_recording.rs",
            """
            fn new_direct(runtime: &mut Runtime) {
                runtime.start_recording(command, policy);
            }
            """,
        )
        media_execution_errors = direct_app_media_execution_ratchet_errors(
            root,
            baseline={},
        )
        assert_contains(media_execution_errors, "src/app/direct_recording.rs")
        if any("src/app/resident/capture_worker.rs" in error for error in media_execution_errors):
            raise AssertionError(f"worker-owned execution path was rejected: {media_execution_errors!r}")
        if any("src/app/resident/effects.rs" in error for error in media_execution_errors):
            raise AssertionError(f"side-effect enqueue path was rejected: {media_execution_errors!r}")
        if any("src/app/responses.rs" in error for error in media_execution_errors):
            raise AssertionError(f"side-effect enqueue path was rejected: {media_execution_errors!r}")

        write_selftest_file(
            root / "src/settings/effective.rs",
            'const WARNING: &str = "deferred-evaluation";',
        )
        write_selftest_file(
            root / "src/protocol/validation/warnings.rs",
            'const WARNING: &str = "deferred-evaluation";',
        )
        write_selftest_file(
            root / "src/app/bad_effective_warning.rs",
            'const WARNING: &str = "deferred-evaluation";',
        )
        deferred_errors = deferred_evaluation_warning_errors(root)
        assert_contains(deferred_errors, "src/app/bad_effective_warning.rs")
        if any("src/settings/effective.rs" in error for error in deferred_errors):
            raise AssertionError(f"settings-owned warning path was rejected: {deferred_errors!r}")

    print("OK architecture guardrail self-tests")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return run_self_tests()
    check_required_files()
    check_owner_docs()
    check_architecture_decision_docs()
    check_cargo_windows_dependency_scope()
    check_protocol_neutrality()
    check_windows_api_imports()
    check_no_native_host_stdout_smells()
    check_control_pipe_is_not_data_bus()
    check_media_command_producers_do_not_push_chunks()
    check_command_events_are_typed_before_app_response()
    check_native_host_responses_use_typed_outbound()
    check_media_policy_alignment()
    check_settings_sync_does_not_ignore_runtime_side_effects()
    check_settings_effective_policy_stays_settings_owned()
    check_recording_quality_policy_has_no_duplicate_owner()
    check_tray_apply_is_bounded_and_acked()
    check_startup_degraded_policy_gates_operational_workers()
    check_base64_media_encoding_stays_transfer_owned()
    check_app_reads_transfer_stores_only_via_typed_dispatch()
    check_audit11_media_workers_do_not_expand_basic_worker_baseline()
    check_audit11_app_media_execution_does_not_expand_baseline()
    check_audit11_deferred_evaluation_does_not_expand_baseline()
    check_native_message_control_limit_is_bounded()
    check_app_control_channel_rejects_media_chunk_dto_path()
    check_hot_path_sleeps_are_allowlisted()
    check_control_plane_receives_are_bounded()
    check_protected_media_lifecycle_is_fail_closed()
    check_os_owners_do_not_call_transports()
    check_windows_pipe_worker_event_bridge()
    check_resident_agent_startup_does_not_block_pipe_for_repair_state()
    check_agent_event_hub_keeps_concurrent_sessions()
    check_windows_pipe_allows_concurrent_instances()
    check_windows_native_host_uses_duplex_forwarding()
    print("OK architecture guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
