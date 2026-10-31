#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import tempfile
import textwrap
from collections import defaultdict
from pathlib import Path

try:
    from .exception_registry import exact_edges, exact_paths
except ImportError:
    from exception_registry import exact_edges, exact_paths


ROOT = Path(__file__).resolve().parents[3]
# Product boundary checks intentionally scan src/** only. Tooling and docs live in a separate
# harness/documentation scope and must not be judged as product-code topology.
SRC = ROOT / "src"

OWNERS = {
    "app",
    "protocol",
    "ipc",
    "settings",
    "assets",
    "tray",
    "hotkeys",
    "capture",
    "recording",
    "audio",
    "telemetry",
    "transfer",
    "install",
    "diagnostics",
}

MAX_RUST_FILE_LINES = 300
MAX_MAIN_LINES = 80
MAX_ROOT_MOD_LINES = 120
MAX_DIRECT_OWNER_FILES = 12
MAX_OWNER_FILES = 40
MAX_FUNCTION_LINES = 50
FILE_LEVEL_DEAD_CODE_BASELINE = 0
PLATFORM_CFG_DEAD_CODE_BASELINE = 0
ALLOWLISTED_COMPAT_FACADES: dict[str, str] = {
    path: "reviewed canonical role facade" for path in exact_paths("architecture.compatibility-facade")
}
ALLOWLISTED_WILDCARD_REEXPORTS: dict[str, str] = {}

ROOT_FACADE_FORBIDDEN = [
    r"\bstruct\b",
    r"\benum\b",
    r"\bimpl\b",
    r"\bmatch\b",
    r"\bfor\b",
    r"\bwhile\b",
    r"\bloop\b",
    r"\bserde_json::from",
    r"\bwindows::",
]

OWNER_IMPORT_DENY = {
    "protocol": OWNERS - {"protocol"},
    "ipc": {"capture", "recording", "audio", "tray", "hotkeys", "transfer", "diagnostics"},
    "settings": {"ipc", "capture", "recording", "audio", "assets", "tray", "hotkeys", "transfer"},
    "assets": {"ipc", "capture", "recording", "audio", "settings", "tray", "hotkeys"},
    "transfer": {"ipc", "capture", "recording", "audio", "tray", "hotkeys"},
    "diagnostics": {"capture", "recording", "audio", "transfer", "tray", "hotkeys"},
    "capture": {"ipc", "settings", "tray", "hotkeys"},
    "recording": {"ipc", "settings", "tray", "hotkeys"},
    "audio": {"ipc", "settings", "tray", "hotkeys"},
    "telemetry": {"ipc", "settings", "tray", "hotkeys"},
    "hotkeys": {"tray"},
}

OWNER_IMPORT_ALLOW = exact_edges("architecture.owner-import")

LIFECYCLE_SMELLS = {
    r"\bstd::thread::spawn\b": "thread spawn requires explicit shutdown owner",
    r"\btokio::spawn\b": "async task spawn requires explicit shutdown owner",
    r"\bmpsc::unbounded": "unbounded channel is forbidden in runtime code",
    r"\bfutures::channel::mpsc::unbounded\b": "unbounded channel is forbidden in runtime code",
    r"\basync_channel::unbounded\b": "unbounded channel is forbidden in runtime code",
    r"\bcrossbeam_channel::unbounded\b": "unbounded channel is forbidden in runtime code",
    r"\bflume::unbounded\b": "unbounded channel is forbidden in runtime code",
    r"\bstd::sync::mpsc::channel\b": "std mpsc channel lacks explicit capacity; use bounded channels",
}
FILE_LEVEL_DEAD_CODE_ALLOW = re.compile(r"(?m)^\s*#!\[allow\([^\]]*\bdead_code\b")
PLATFORM_CFG_DEAD_CODE_ALLOW = re.compile(
    r"(?m)^\s*#!\[cfg_attr\(not\(any\(windows,\s*test\)\),\s*allow\(dead_code\)\)\]"
)

APP_ROLE_FILES = {
    "src/app/bridge.rs",
    "src/app/host.rs",
    "src/app/resident.rs",
    "src/app/responses.rs",
    "src/app/support.rs",
}

APP_LEGACY_ROLE_PATHS = {
    "src/app/agent.rs",
    "src/app/agent_pipe.rs",
    "src/app/agent_pipe_server.rs",
    "src/app/agent_pipe_windows.rs",
    "src/app/mode.rs",
    "src/app/native_host.rs",
    "src/app/native_host_responses.rs",
    "src/app/native_host_hello.rs",
    "src/app/native_host_tray.rs",
    "src/app/shutdown.rs",
    "src/app/startup.rs",
    "src/app/agent",
    "src/app/agent_pipe",
    "src/app/agent_pipe_server",
    "src/app/mode",
    "src/app/native_host",
    "src/app/native_host_responses",
    "src/app/native_host_hello",
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def path_label(path: Path, root: Path = ROOT) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def fail(messages: list[str]) -> None:
    print("FAIL boundary guardrails")
    for message in messages:
        print(f"- {message}")
    raise SystemExit(1)


def rust_files() -> list[Path]:
    if not SRC.exists():
        return []
    return [path for path in SRC.rglob("*.rs") if "/tests/" not in path.as_posix() and path.name != "tests.rs"]


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def line_count(path: Path) -> int:
    return len(read(path).splitlines())


def owner_for(path: Path) -> str | None:
    parts = path.relative_to(SRC).parts
    if not parts:
        return None
    if parts[0] == "main.rs":
        return "main"
    if parts[0] in OWNERS:
        return parts[0]
    return None


def facade_statements(path: Path) -> list[str]:
    statements: list[str] = []
    current: list[str] = []
    for raw in read(path).splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("//") or stripped.startswith("#[") or stripped.startswith("#![doc"):
            continue
        current.append(stripped)
        if stripped.endswith(";"):
            statements.append(" ".join(current))
            current = []
    if current:
        statements.append(" ".join(current))
    return statements


def is_facade_statement(statement: str) -> bool:
    return bool(
        re.match(r"^(?:pub(?:\(crate\))?\s+)?mod\s+\w+\s*;$", statement)
        or re.match(r"^use\s+.+;$", statement)
        or re.match(r"^pub(?:\(crate\))?\s+use\s+.+;$", statement)
        or re.match(r"^(?:pub(?:\(crate\))?\s+)?type\s+\w+(?:<[^>]+>)?\s*=.+;$", statement)
    )


def is_facade_like(path: Path) -> bool:
    statements = facade_statements(path)
    return bool(statements) and all(is_facade_statement(statement) for statement in statements)


def is_canonical_owner_facade(
    path: Path,
    src: Path = SRC,
    owners: set[str] = OWNERS,
) -> bool:
    return path.name == "mod.rs" and path.parent.parent == src and path.parent.name in owners


def has_wildcard_reexport(statement: str) -> bool:
    return bool(re.search(r"^pub(?:\(crate\))?\s+use\b.+::\s*\*\s*;$", statement))


def has_macro_call(statement: str) -> bool:
    return bool(re.search(r"\b\w+!", statement))


def has_function_call(statement: str) -> bool:
    without_visibility = re.sub(r"\bpub\s*\(\s*crate\s*\)", "pub", statement)
    return bool(re.search(r"\b[a-z_]\w*\s*\(", without_visibility))


def has_type_alias(statement: str) -> bool:
    return bool(re.match(r"^(?:pub(?:\(crate\))?\s+)?type\s+", statement))


def reexport_targets(path: Path, statement: str, src: Path = SRC) -> list[Path]:
    if not re.match(r"^pub(?:\(crate\))?\s+use\b", statement):
        return []

    match = re.match(r"^pub(?:\(crate\))?\s+use\s+(.+);$", statement)
    if not match:
        return []

    use_path = match.group(1).strip()
    if "{" in use_path:
        use_path = use_path.split("::{", 1)[0]
    elif use_path.endswith("::*"):
        use_path = use_path.removesuffix("::*")
    else:
        parts = use_path.split("::")
        use_path = "::".join(parts[:-1])

    parts = [part for part in use_path.split("::") if part]
    if not parts:
        return []

    targets: list[Path] = []
    base = path.parent
    if parts[0] == "crate" and len(parts) >= 2:
        base = src / parts[1]
        parts = parts[2:]
    elif parts[0] in {"super", "self"}:
        parts = parts[1:]

    if parts:
        module_path = base.joinpath(*parts)
        targets.append(module_path.with_suffix(".rs"))
        targets.append(module_path / "mod.rs")

    return list(dict.fromkeys(targets))


def facade_file_errors(
    path: Path,
    facade_like: set[Path],
    root: Path = ROOT,
    src: Path = SRC,
    owners: set[str] = OWNERS,
) -> list[str]:
    errors: list[str] = []
    relative = path_label(path, root)
    statements = facade_statements(path)
    compat_allowlisted = relative in ALLOWLISTED_COMPAT_FACADES
    wildcard_allowlisted = relative in ALLOWLISTED_WILDCARD_REEXPORTS

    if not is_canonical_owner_facade(path, src, owners) and not compat_allowlisted:
        errors.append(f"{relative} is an accidental facade-like file")

    if any(has_wildcard_reexport(statement) for statement in statements) and not wildcard_allowlisted:
        errors.append(f"{relative} contains wildcard facade re-export")

    if any(has_type_alias(statement) for statement in statements) and not compat_allowlisted:
        errors.append(f"{relative} contains compatibility type alias without facade allowlist")

    if any(has_macro_call(statement) for statement in statements):
        errors.append(f"{relative} contains macro call in facade-like file")

    if any(has_function_call(statement) for statement in statements):
        errors.append(f"{relative} contains function call in facade-like file")

    statement_text = " ".join(statements)
    forbidden = [
        pattern
        for pattern in [
            r"\bfn\b",
            r"\bstruct\b",
            r"\benum\b",
            r"\bimpl\b",
            r"\bconst\b",
            r"\bstatic\b",
            r"\bmatch\b",
            r"\bfor\b",
            r"\bwhile\b",
        ]
        if re.search(pattern, statement_text)
    ]
    if forbidden:
        errors.append(f"{relative} contains forbidden facade tokens: " + ", ".join(forbidden))

    for statement in statements:
        for target in reexport_targets(path, statement, src):
            if target in facade_like:
                errors.append(
                    f"{relative} re-exports facade-like file {path_label(target, root)}; facade chains are forbidden"
                )

    return errors


def check_facade_files(errors: list[str]) -> None:
    facade_like = {path for path in rust_files() if is_facade_like(path)}
    stale = sorted(set(ALLOWLISTED_COMPAT_FACADES) - {rel(path) for path in facade_like})
    if stale:
        errors.append("stale compatibility facade exception(s): " + ", ".join(stale))

    for path in sorted(facade_like):
        errors.extend(facade_file_errors(path, facade_like))


def check_file_sizes(errors: list[str]) -> None:
    for path in rust_files() + ([SRC / "main.rs"] if (SRC / "main.rs").exists() else []):
        lines = line_count(path)
        relative = rel(path)
        if relative == "src/main.rs" and lines > MAX_MAIN_LINES:
            errors.append(f"{relative} has {lines} lines; main.rs limit is {MAX_MAIN_LINES}")
            continue
        if path.name == "mod.rs" and path.parent.parent == SRC and lines > MAX_ROOT_MOD_LINES:
            errors.append(f"{relative} has {lines} lines; root owner mod.rs limit is {MAX_ROOT_MOD_LINES}")
            continue
        if lines > MAX_RUST_FILE_LINES:
            errors.append(f"{relative} has {lines} lines; file limit is {MAX_RUST_FILE_LINES}")


def file_level_dead_code_files(files: list[Path]) -> list[Path]:
    return [path for path in files if FILE_LEVEL_DEAD_CODE_ALLOW.search(read(path))]


def dead_code_ratchet_error(
    suppressed: list[Path],
    baseline: int,
    label: str,
) -> str | None:
    if len(suppressed) <= baseline:
        return None
    overflow = len(suppressed) - baseline
    return (
        f"{label} count increased "
        f"to {len(suppressed)} above baseline {baseline}; "
        f"remove {overflow} new suppression(s) or lower the baseline after cleanup"
    )


def file_level_dead_code_ratchet_error(files: list[Path], baseline: int) -> str | None:
    return dead_code_ratchet_error(
        file_level_dead_code_files(files),
        baseline,
        "file-level allow(dead_code)",
    )


def platform_cfg_dead_code_files(files: list[Path]) -> list[Path]:
    return [path for path in files if PLATFORM_CFG_DEAD_CODE_ALLOW.search(read(path))]


def platform_cfg_dead_code_ratchet_error(files: list[Path], baseline: int) -> str | None:
    return dead_code_ratchet_error(
        platform_cfg_dead_code_files(files),
        baseline,
        "platform cfg allow(dead_code)",
    )


def check_file_level_dead_code_ratchet(errors: list[str]) -> None:
    files = rust_files()
    for error in [
        file_level_dead_code_ratchet_error(files, FILE_LEVEL_DEAD_CODE_BASELINE),
        platform_cfg_dead_code_ratchet_error(files, PLATFORM_CFG_DEAD_CODE_BASELINE),
    ]:
        if error:
            errors.append(error)


def check_root_facades(errors: list[str]) -> None:
    main = SRC / "main.rs"
    if main.exists():
        text = read(main)
        forbidden = [pattern for pattern in ROOT_FACADE_FORBIDDEN if re.search(pattern, text)]
        if forbidden:
            errors.append("src/main.rs contains non-dispatch logic markers: " + ", ".join(forbidden))

    for owner in OWNERS:
        path = SRC / owner / "mod.rs"
        if not path.exists():
            continue
        text = read(path)
        forbidden = [pattern for pattern in ROOT_FACADE_FORBIDDEN if re.search(pattern, text)]
        if forbidden:
            errors.append(f"{rel(path)} must stay a thin facade; found: " + ", ".join(forbidden))


def check_app_role_topology(errors: list[str]) -> None:
    app_mod = SRC / "app" / "mod.rs"
    if not app_mod.exists():
        errors.append("src/app/mod.rs is missing")
        return
    app_mod_text = read(app_mod)
    for role in ("bridge", "host", "resident", "responses", "support"):
        if f"mod {role};" not in app_mod_text:
            errors.append(f"src/app/mod.rs must declare app role module {role}")
    if "pub(crate) use host::run;" not in app_mod_text:
        errors.append("src/app/mod.rs must expose only the host run entrypoint")
    for legacy in ("agent", "agent_pipe", "agent_pipe_server", "mode", "native_host"):
        if re.search(rf"\bmod\s+{legacy}\s*;", app_mod_text):
            errors.append(f"src/app/mod.rs must not declare legacy app role {legacy}")

    missing_roles = [relative for relative in sorted(APP_ROLE_FILES) if not (ROOT / relative).is_file()]
    if missing_roles:
        errors.append("missing app role file(s): " + ", ".join(missing_roles))
    legacy_paths = [relative for relative in sorted(APP_LEGACY_ROLE_PATHS) if (ROOT / relative).exists()]
    if legacy_paths:
        errors.append("legacy app role path(s) must stay removed: " + ", ".join(legacy_paths))


def extract_owner_imports(path: Path) -> set[str]:
    text = read(path)
    imports: set[str] = set()
    for owner in OWNERS:
        patterns = [
            rf"\buse\s+crate::{owner}\b",
            rf"\bcrate::{owner}::",
            rf"\bsuper::super::{owner}::",
            rf"\buse\s+crate::\{{[^}}]*\b{owner}\b",
            rf"\buse\s+crate::\{{[^}}]*\{{[^}}]*\b{owner}\b",
        ]
        if any(re.search(pattern, text) for pattern in patterns):
            imports.add(owner)
    return imports


def check_import_direction(errors: list[str]) -> None:
    graph: dict[str, set[str]] = defaultdict(set)
    observed_exceptions: set[tuple[str, str, str]] = set()
    for path in rust_files():
        owner = owner_for(path)
        if owner is None or owner == "main":
            continue
        imports = extract_owner_imports(path)
        graph[owner].update(imported_owner for imported_owner in imports if imported_owner != owner)
        denied = OWNER_IMPORT_DENY.get(owner, set())
        bad = sorted(
            imported for imported in imports & denied if (rel(path), owner, imported) not in OWNER_IMPORT_ALLOW
        )
        if bad:
            errors.append(f"{rel(path)} imports forbidden owner(s): {', '.join(bad)}")
        for imported in imports & denied:
            edge = (rel(path), owner, imported)
            if edge in OWNER_IMPORT_ALLOW:
                observed_exceptions.add(edge)

    for owner, imports in graph.items():
        for imported in imports:
            if owner in graph.get(imported, set()):
                errors.append(f"owner import cycle detected: {owner} <-> {imported}")
    stale = sorted(set(OWNER_IMPORT_ALLOW) - observed_exceptions)
    if stale:
        errors.append(
            "stale owner-import exception(s): "
            + ", ".join(f"{path}:{owner}->{imported}" for path, owner, imported in stale)
        )


def check_directory_shape(errors: list[str]) -> None:
    for owner in OWNERS:
        owner_dir = SRC / owner
        if not owner_dir.exists():
            continue
        direct_files = [path for path in owner_dir.glob("*.rs") if path.name != "mod.rs"]
        all_files = [path for path in owner_dir.rglob("*.rs") if "/tests/" not in path.as_posix()]
        sub_owner_dirs = sorted(
            {path.relative_to(owner_dir).parts[0] for path in all_files if len(path.relative_to(owner_dir).parts) > 1}
        )
        if len(direct_files) > MAX_DIRECT_OWNER_FILES:
            errors.append(f"src/{owner} has {len(direct_files)} direct Rust files; split into sub-owners")
        if len(all_files) > MAX_OWNER_FILES and len(sub_owner_dirs) < 2:
            errors.append(f"src/{owner} has {len(all_files)} Rust files; split owner directory")
        for sub_owner in sub_owner_dirs:
            sub_owner_files = [path for path in all_files if path.relative_to(owner_dir).parts[0] == sub_owner]
            nested_sub_owners = {
                path.relative_to(owner_dir).parts[1]
                for path in sub_owner_files
                if len(path.relative_to(owner_dir).parts) > 2
            }
            if len(sub_owner_files) > MAX_OWNER_FILES and len(nested_sub_owners) < 2:
                errors.append(
                    f"src/{owner}/{sub_owner} has {len(sub_owner_files)} Rust files; split sub-owner directory"
                )

        repeated_prefix = [
            rel(path) for path in all_files if path.stem.startswith(f"{owner}_") or path.stem.startswith(f"{owner}-")
        ]
        if repeated_prefix:
            errors.append(f"src/{owner} repeats owner name in file names: " + ", ".join(repeated_prefix))


def check_lifecycle_smells(errors: list[str]) -> None:
    for path in rust_files():
        text = read(path)
        for pattern, reason in LIFECYCLE_SMELLS.items():
            if re.search(pattern, text):
                errors.append(f"{rel(path)}: {reason}")


def check_function_sizes(errors: list[str]) -> None:
    function_start = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+")
    for path in rust_files():
        lines = read(path).splitlines()
        start_line: int | None = None
        brace_depth = 0
        for index, line in enumerate(lines, start=1):
            if start_line is None and function_start.search(line):
                start_line = index
                brace_depth = line.count("{") - line.count("}")
                if brace_depth <= 0 and "{" in line:
                    start_line = None
                continue
            if start_line is not None:
                brace_depth += line.count("{") - line.count("}")
                if brace_depth <= 0:
                    length = index - start_line + 1
                    if length > MAX_FUNCTION_LINES:
                        errors.append(
                            f"{rel(path)}:{start_line} function has {length} lines; limit is {MAX_FUNCTION_LINES}"
                        )
                    start_line = None


def extract_struct_body(text: str, struct_name: str) -> str | None:
    match = re.search(rf"\bstruct\s+{re.escape(struct_name)}\b[^\{{;]*\{{", text)
    if not match:
        return None

    body_start = match.end()
    brace_depth = 1
    for index in range(body_start, len(text)):
        char = text[index]
        if char == "{":
            brace_depth += 1
        elif char == "}":
            brace_depth -= 1
            if brace_depth == 0:
                return text[body_start:index]
    return None


def check_resident_state_authority(errors: list[str]) -> None:
    session_path = SRC / "app" / "responses" / "session.rs"
    server_path = SRC / "app" / "resident" / "pipe_broker.rs"
    duplex_server_path = SRC / "app" / "resident" / "pipe_broker" / "duplex.rs"
    if not session_path.exists() or not server_path.exists():
        return

    session_text = read(session_path)
    shared_body = extract_struct_body(session_text, "NativeHostSharedState")
    if shared_body is None:
        errors.append("src/app/responses/session.rs must define NativeHostSharedState")
    elif "Arc<Mutex<ControllerLeaseBook>>" not in re.sub(r"\s+", "", shared_body):
        errors.append(
            "src/app/responses/session.rs NativeHostSharedState must own "
            "Arc<Mutex<ControllerLeaseBook>> for resident-wide controller leases"
        )

    native_session_body = extract_struct_body(session_text, "NativeHostSession")
    if native_session_body is None:
        errors.append("src/app/responses/session.rs must define NativeHostSession")
    elif "ControllerLeaseBook" in native_session_body or re.search(r"\blease_book\s*:", native_session_body):
        errors.append(
            "src/app/responses/session.rs NativeHostSession must not own "
            "ControllerLeaseBook; controller lease authority is resident-wide shared state"
        )

    server_text = read(server_path)
    if "let shared_state = NativeHostSharedState::default();" not in server_text:
        errors.append("src/app/resident/pipe_broker.rs must create one shared native-host state per server loop")
    if "run_agent_pipe_session_with_effects_and_state" not in server_text:
        errors.append("src/app/resident/pipe_broker.rs must pass shared native-host state into blocking pipe sessions")
    duplex_server_text = read(duplex_server_path) if duplex_server_path.exists() else ""
    if "let shared_state = NativeHostSharedState::default();" not in duplex_server_text:
        errors.append(
            "src/app/resident/pipe_broker/duplex.rs must create one shared native-host state per concurrent server loop"
        )
    if "run_agent_pipe_session_with_effects_events_shared_auth_state" not in duplex_server_text:
        errors.append("src/app/resident/pipe_broker/duplex.rs must pass shared auth/state into duplex pipe sessions")


def write_selftest_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(content).strip() + "\n", encoding="utf-8")


def assert_contains(errors: list[str], needle: str) -> None:
    if not any(needle in error for error in errors):
        raise AssertionError(f"expected guardrail error containing {needle!r}, got {errors!r}")


def run_self_tests() -> int:
    with tempfile.TemporaryDirectory(prefix="sniptale-boundary-") as temp_dir:
        root = Path(temp_dir)
        src = root / "src"
        owner = src / "capture"
        write_selftest_file(
            owner / "mod.rs",
            "mod chain;\nmod wgc;\npub(crate) use chain::ChainPlan;\npub(crate) use wgc::CapturePlan;",
        )
        write_selftest_file(owner / "chain" / "mod.rs", "pub(crate) mod model;")
        write_selftest_file(owner / "wgc.rs", "pub(crate) use super::source::CaptureSource;")
        write_selftest_file(owner / "source.rs", "pub(crate) mod nested;")
        write_selftest_file(owner / "dto.rs", "pub(crate) use super::wgc::*;")
        write_selftest_file(owner / "alias.rs", "pub(crate) type CaptureDto = wgc::CapturePlan;")
        write_selftest_file(owner / "call.rs", "pub(crate) use build_facade();")

        files = sorted(owner.rglob("*.rs"))
        facade_like = {path for path in files if is_facade_like(path)}
        owners = {"capture"}

        root_errors = facade_file_errors(owner / "mod.rs", facade_like, root, src, owners)
        assert_contains(root_errors, "facade chains are forbidden")

        dto_errors = facade_file_errors(owner / "dto.rs", facade_like, root, src, owners)
        assert_contains(dto_errors, "accidental facade-like file")
        assert_contains(dto_errors, "wildcard facade re-export")
        assert_contains(dto_errors, "facade chains are forbidden")

        alias_errors = facade_file_errors(owner / "alias.rs", facade_like, root, src, owners)
        assert_contains(alias_errors, "compatibility type alias without facade allowlist")

        call_errors = facade_file_errors(owner / "call.rs", facade_like, root, src, owners)
        assert_contains(call_errors, "function call in facade-like file")

        nested_targets = reexport_targets(
            owner / "mod.rs",
            "pub(crate) use source::nested::Nested;",
            src,
        )
        assert owner / "source" / "nested.rs" in nested_targets
        assert owner / "source.rs" not in nested_targets

        write_selftest_file(owner / "allowed.rs", "#![allow(dead_code)]\npub(crate) fn helper() {}")
        write_selftest_file(owner / "allowed_too.rs", "#![allow(dead_code)]\npub(crate) fn helper() {}")
        write_selftest_file(
            owner / "platform.rs",
            "#![cfg_attr(not(any(windows, test)), allow(dead_code))]\npub(crate) fn helper() {}",
        )
        dead_code_files = file_level_dead_code_files([owner / "allowed.rs", owner / "allowed_too.rs", owner / "wgc.rs"])
        assert len(dead_code_files) == 2
        assert file_level_dead_code_ratchet_error(dead_code_files, 2) is None
        assert "above baseline 1" in (file_level_dead_code_ratchet_error(dead_code_files, 1) or "")
        platform_files = platform_cfg_dead_code_files([owner / "platform.rs", owner / "allowed.rs"])
        assert len(platform_files) == 1
        assert platform_cfg_dead_code_ratchet_error(platform_files, 1) is None
        assert "above baseline 0" in (platform_cfg_dead_code_ratchet_error(platform_files, 0) or "")
        hotkeys = src / "hotkeys"
        write_selftest_file(
            hotkeys / "bad.rs",
            "use crate::tray::TrayActionRegistry;\nfn call(_: TrayActionRegistry) {}",
        )
        assert "tray" in OWNER_IMPORT_DENY["hotkeys"]
        assert "tray" in extract_owner_imports(hotkeys / "bad.rs")

    sample = """
        pub(crate) struct NativeHostSharedState {
            lease_book: Arc<Mutex<ControllerLeaseBook>>,
        }

        pub(crate) struct NativeHostSession {
            shared_state: NativeHostSharedState,
            session_token: Option<String>,
        }
    """
    assert "ControllerLeaseBook" in (extract_struct_body(textwrap.dedent(sample), "NativeHostSharedState") or "")
    assert "ControllerLeaseBook" not in (extract_struct_body(textwrap.dedent(sample), "NativeHostSession") or "")

    print("OK boundary guardrail self-tests")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return run_self_tests()
    errors: list[str] = []
    check_file_sizes(errors)
    check_file_level_dead_code_ratchet(errors)
    check_root_facades(errors)
    check_app_role_topology(errors)
    check_facade_files(errors)
    check_import_direction(errors)
    check_directory_shape(errors)
    check_lifecycle_smells(errors)
    check_function_sizes(errors)
    check_resident_state_authority(errors)
    if errors:
        fail(errors)
    print("OK boundary guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
