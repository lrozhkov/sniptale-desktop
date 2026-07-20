# Lab topology

## Owners

- `tooling/desktop/e2e-lab` is the user entrypoint; `tooling/desktop/e2e_lab/**` owns parsing, lifecycle, transport, builds, and receipts.
- `.artifacts/desktop-e2e-lab/cache/**` holds ignored checksum-verified tools and source/profile/lane-bound build artifacts.
- `.artifacts/desktop-e2e-lab/golden/<lane>/**` holds ignored immutable clean disks and identity.
- `.tmp/qa/desktop-e2e-lab/runtime/<lane>/**` holds one disposable overlay and process state.
- `.tmp/qa/desktop-e2e-lab/<run-id>/**` holds receipts, JUnit, environment, manifests, and collected evidence.

## Invariants

The controller lock serializes mutations and permits one heavy VM. SSH/VNC bind to loopback and QMP uses a Unix socket. Runtime overlays use a verified flattened golden as read-only backing state and are destroyed on success or failure; forced termination is recorded.

State, QMP, SSH, guest JSON, artifacts, manifests, and files are untrusted until exact-schema, size, path, and SHA-256 validation succeeds. Destructive paths must resolve below the lab roots. Source archives include current tracked and untracked work but exclude `.git`, `target`, `.tmp`, `.artifacts`, VM state, caches, secrets, signing material, and user media.

The build cache key binds deterministic source digest, lane, and profile. Cache hits skip compilation only; install and tests always use a new overlay. Corruption is a cache miss.

Runtime guests use restricted QEMU networking and prove denied egress; provisioning and the explicit build overlay may enable network for checksum-verified dependencies. Windows builds use `Cargo.lock`. Resource ceilings, host floors, and retention are machine-owned by `tooling/desktop/e2e_lab/lab-config.json`; use controller `gc` for cleanup.
