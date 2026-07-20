# ADR 0001: Resident media worker ownership

Status: Accepted, 2026-07-09.

## Decision

The resident application is one Windows process with owner-specific worker lifecycles. `app/**` owns startup order, `sync_channel(1)` command queues, typed events, cancellation, reverse-order joining, and failure projection. `AssetPublisher`, `CaptureWorker`, `TelemetryWorker`, `EncoderWorker`, `AudioWorker`, `TransferWorker`, and `PipeServer` delegate domain behavior to their owner modules.

Command acceptance never runs capture or recording work on the caller. A full queue produces typed backpressure instead of hidden buffering.

## Trade-off

One process keeps installation and coordination small but does not isolate media and privileged adapters from a process-wide crash or compromise. Owner-local typed boundaries are mandatory so a future broker-controlled media process can be introduced without moving raw handles, raw JSON, or media bytes across the boundary. Process separation requires a new contract and security decision.

## Proof

`src/app/resident/startup.rs`, `src/app/resident/workers.rs`, and owner worker tests define the live topology. `tooling/qa/rules/verify_architecture.py`, `tooling/qa/rules/verify_boundaries.py`, and Windows desktop E2E receipts reject no-op workers, direct media execution, unbounded queues, and missing shutdown ownership.
