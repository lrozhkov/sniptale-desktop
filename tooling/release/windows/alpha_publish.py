"""Trusted dispatch admission and draft-first publication of an existing alpha build."""

from __future__ import annotations

import base64
import json
import os
import re
import subprocess
import zipfile
from pathlib import Path

from alpha_artifacts import digest, verify_bundle, version_tag, write_json

REPO = "lrozhkov/sniptale-desktop"
RELEASE_CASES = {"network-isolation", "clean-install", "first-run", "capture", "recording", "restart", "uninstall"}


def gh(*args: str, body: dict | None = None) -> str:
    command = ["gh", *args]
    if body is not None:
        command.extend(("--method", "POST", "--input", "-"))
    return subprocess.check_output(command, input=json.dumps(body) if body is not None else None, text=True).strip()


def api(path: str, body: dict | None = None) -> dict:
    return json.loads(gh("api", f"repos/{REPO}/{path}", body=body))


def admit(run: dict, artifact: dict, receipt: dict, commit: str, tag: str) -> None:
    version_tag(tag)
    if (
        run.get("head_sha") != commit
        or run.get("head_branch") != "main"
        or run.get("event") != "workflow_dispatch"
        or run.get("conclusion") != "success"
        or run.get("status") != "completed"
        or run.get("path") != ".github/workflows/alpha.yml"
        or run.get("repository", {}).get("full_name") != REPO
        or run.get("head_repository", {}).get("full_name") != REPO
    ):
        raise ValueError("untrusted or unsuccessful source run")
    if (
        artifact.get("expired") is not False
        or artifact.get("name") != f"alpha-{commit}-{run['id']}-{run['run_attempt']}"
        or not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", ""))
    ):
        raise ValueError("source artifact identity or digest invalid")
    expected = {
        "schemaVersion": 1,
        "kind": "sniptale-alpha-windows-acceptance",
        "commit": commit,
        "tag": tag,
        "runId": str(run["id"]),
        "runAttempt": str(run["run_attempt"]),
        "artifactId": artifact["id"],
        "artifactDigest": artifact["digest"],
        "result": "pass",
        "verified": True,
        "cleanTeardown": True,
        "devLocalTrust": False,
    }
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError("Windows acceptance identity mismatch")
    if (
        receipt.get("tests") != {name: "pass" for name in sorted(RELEASE_CASES)}
        or not re.fullmatch(r"[0-9a-f]{64}", receipt.get("checksumsSha256", ""))
        or not re.fullmatch(r"[0-9a-f]{64}", receipt.get("receiptSha256", ""))
    ):
        raise ValueError("incomplete or malformed Windows acceptance")


def main() -> None:
    if os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ValueError("publication controls must run from main")
    run_id = os.environ["ALPHA_RUN_ID"]
    artifact_id = os.environ["ALPHA_ARTIFACT_ID"]
    commit = os.environ["ALPHA_COMMIT"]
    if os.environ.get("GITHUB_SHA") != commit:
        raise ValueError("candidate must match the workflow commit whose environment was audited")
    tag = os.environ["ALPHA_TAG"]
    encoded = os.environ["ALPHA_ACCEPTANCE"]
    if not re.fullmatch(r"[1-9][0-9]*", run_id) or not re.fullmatch(r"[1-9][0-9]*", artifact_id):
        raise ValueError("invalid source run/artifact ID")
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or len(encoded) > 16000:
        raise ValueError("invalid source commit or oversized acceptance")
    receipt = json.loads(base64.b64decode(encoded, validate=True))
    run = api(f"actions/runs/{run_id}")
    artifact = api(f"actions/artifacts/{artifact_id}")
    listed = api(f"actions/runs/{run_id}/artifacts?per_page=100")["artifacts"]
    if sum(item["id"] == int(artifact_id) for item in listed) != 1:
        raise ValueError("artifact does not belong to source run")
    admit(run, artifact, receipt, commit, tag)
    # Checkout is controlled by the trusted workflow. Source bundle contents are only data.
    source_commit = gh("api", f"repos/{REPO}/commits/main", "--jq", ".sha")
    if source_commit != commit:
        raise ValueError("candidate must be current reviewed main")
    directory = Path(".tmp/alpha-publication")
    directory.mkdir(parents=True, exist_ok=False)
    archive = directory / "candidate.zip"
    with archive.open("wb") as stream:
        subprocess.run(["gh", "api", f"repos/{REPO}/actions/artifacts/{artifact_id}/zip"], stdout=stream, check=True)
    if "sha256:" + digest(archive) != artifact["digest"]:
        raise ValueError("downloaded Actions artifact digest mismatch")
    bundle = directory / "bundle"
    bundle.mkdir()
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        if len(entries) > 512 or sum(item.file_size for item in entries) > 512 * 1024 * 1024:
            raise ValueError("oversized candidate archive")
        for item in entries:
            path = Path(item.filename)
            if path.is_absolute() or ".." in path.parts or "\\" in item.filename:
                raise ValueError("invalid candidate archive entry")
            if item.is_dir():
                continue
            target = bundle / path
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists():
                raise ValueError("duplicate candidate archive entry")
            target.write_bytes(zipped.read(item))
    provenance = verify_bundle(bundle, commit, tag)
    if (
        provenance["runId"] != run_id
        or provenance["runAttempt"] != str(run["run_attempt"])
        or digest(bundle / "SHA256SUMS") != receipt["checksumsSha256"]
    ):
        raise ValueError("candidate and tested artifact mismatch")
    assets = directory / "assets"
    assets.mkdir()
    version = version_tag(tag)
    msi_name = f"SniptaleNativeAgent-dev-{version}.msi"
    (assets / msi_name).write_bytes((bundle / msi_name).read_bytes())
    (assets / "SHA256SUMS").write_bytes((bundle / "SHA256SUMS").read_bytes())
    write_json(assets / "windows-acceptance.json", receipt)
    with zipfile.ZipFile(assets / f"SniptaleDesktop-{tag}.zip", "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(bundle.rglob("*")):
            if path.is_file():
                entry = zipfile.ZipInfo(path.relative_to(bundle).as_posix(), (1980, 1, 1, 0, 0, 0))
                entry.compress_type = zipfile.ZIP_DEFLATED
                zipped.writestr(entry, path.read_bytes())
    notes = directory / "notes.txt"
    notes.write_text(
        f"""First Windows alpha for Manual settings, screenshots and local MP4 recording.

Install `{msi_name}`. For the first launch, run `%LOCALAPPDATA%\\Sniptale\\NativeAgent\\sniptale-native-agent.exe`; later Windows sign-ins use the registered autostart. In the tray menu choose Mode > Manual, select a local output folder, and choose Save and switch to Manual. Capture and recording controls are in the tray menu. This is an unsigned development-channel build; Windows can display an unknown-publisher warning. Trusted browser-extension integration is unsupported in this alpha. No development trust bypass is shipped.

Tested on the Windows 11 laboratory image identified in windows-acceptance.json; Windows 10 compatibility is not certified. Development/beta/stable packages currently share the install location: uninstall any existing Sniptale Desktop package before this alpha. Side-by-side installation is unsupported. Future releases must advance the numeric MSI version before distribution.

The ZIP contains the exact EXE/MSI, manifest, licenses, SBOM, supply-chain reports, source/run provenance and SHA256SUMS. Verify SHA256SUMS after extracting it. Source: {commit}. Actions run: https://github.com/{REPO}/actions/runs/{run_id} (attempt {run["run_attempt"]}).
""",
        encoding="utf-8",
    )
    # Existing tags/releases are never overwritten, even after a partially failed publication.
    if subprocess.run(["gh", "api", f"repos/{REPO}/git/ref/tags/{tag}"], capture_output=True).returncode == 0:
        raise ValueError("release tag already exists; inspect partial publication before retry")
    api("git/refs", {"ref": "refs/tags/" + tag, "sha": commit})
    gh(
        "release",
        "create",
        tag,
        "--repo",
        REPO,
        "--verify-tag",
        "--draft",
        "--prerelease",
        "--title",
        "Sniptale Desktop " + tag,
        "--notes-file",
        str(notes),
        *[str(path) for path in sorted(assets.iterdir())],
    )
    downloaded = directory / "uploaded"
    gh("release", "download", tag, "--repo", REPO, "--dir", str(downloaded))
    if {path.name: digest(path) for path in downloaded.iterdir()} != {
        path.name: digest(path) for path in assets.iterdir()
    }:
        raise ValueError("draft upload verification failed; draft retained for inspection")
    gh("release", "edit", tag, "--repo", REPO, "--draft=false", "--prerelease", "--latest=false")
    print(f"Published https://github.com/{REPO}/releases/tag/{tag}")


if __name__ == "__main__":
    main()
