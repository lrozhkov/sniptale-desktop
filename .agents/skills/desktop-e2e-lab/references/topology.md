# Lab topology

## Owners

- `tooling/desktop/e2e-lab` is the user entrypoint; `tooling/desktop/e2e_lab/**` owns parsing, lifecycle, transport, builds, and receipts.
- `.artifacts/desktop-e2e-lab/cache/**` holds ignored checksum-verified tools and source/profile/lane-bound build artifacts.
- `.artifacts/desktop-e2e-lab/golden/<lane>/**` holds ignored immutable clean disks and identity.
- `.tmp/qa/desktop-e2e-lab/runtime/<lane>/**` holds one disposable overlay and process state.
- `.tmp/qa/desktop-e2e-lab/<run-id>/**` holds receipts, JUnit, environment, manifests, and collected evidence.

## Invariants

The controller lock serializes mutations and permits one heavy VM. SSH/VNC bind to loopback and QMP uses a Unix socket. Runtime overlays use a verified flattened golden as read-only backing state. Acceptance overlays are destroyed on success or failure; forced termination is recorded. Development session overlays remain owned by the controller until close, expiry, or resource-limit termination.

State, QMP, SSH, guest JSON, artifacts, manifests, and files are untrusted until exact-schema, size, path, and SHA-256 validation succeeds. Destructive paths must resolve below the lab roots. Source archives include current tracked and non-ignored untracked work but exclude `.git`, `target`, `.tmp`, `.artifacts`, VM state, caches, secrets, signing material, and user media.

The build cache key binds deterministic source digest, lane, and profile. Cache hits skip compilation only; acceptance install and tests always use a new overlay. Corruption is a cache miss.

Runtime guests use restricted QEMU networking and prove denied egress; provisioning and the explicit build overlay may enable network for checksum-verified dependencies. Windows builds use `Cargo.lock`. Resource ceilings, host floors, and retention are machine-owned by `tooling/desktop/e2e_lab/lab-config.json`; use controller `gc` for cleanup.

Development sessions store their lease in `.tmp/qa/desktop-e2e-lab/work-session.json` and iteration evidence below `.tmp/qa/desktop-e2e-lab/sessions/<session-id>/iterations/<iteration-id>`. `work_sessions.py` owns generation, boot, dependency-lock, source, health, and lifetime checks; `session_evidence.py` owns diagnostic integrity collection. `session_watchdog.py` expires only its own generation. Clean controller operations require closing the session first. The guest retains its NTFS target directory outside the synchronized source tree; deleted sources are removed and changed bytes receive fresh mtimes. Vendored Cargo sources, the official Windows Rust toolchain and WiX/SDK packages resolved from the canonical tool catalog and generated toolchain lock are explicit hash-verified transfers, while guest egress remains denied.

A saved Windows work image lives under the controller cache and remains diagnostic-only. Saving uses zstd-compressed QCOW2 and checks the candidate before replacing an older cache. It retains compiled dependencies and synthetic application/browser profiles, has a bounded age and size, and is copied into a separate overlay after validating its checksum and original golden backing. Clean acceptance never restores this cache. Windows KVM guests use explicit paravirtual clocks and timers owned by `vm.cpu_model`; provisioning uses the same CPU policy.

External extension builds use the same source selection and limits, including new TypeScript files, but bind a streamed source-tree fingerprint instead of creating an unused transport archive. Git-ignored generated state is excluded; tracked files remain inputs even when an ignore rule matches. Failed bundle creation removes private partial files.

Iteration receipts record temporary storage usage. Before automatic teardown, the watchdog records the expiry or storage-budget reason in the session directory at `termination.json`. Resource ceilings remain owned by the lab configuration; check these receipts when a session disappears.
