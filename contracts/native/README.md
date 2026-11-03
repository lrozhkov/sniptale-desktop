# Native contracts

`contract-index.json` is the tracked inventory of wire Contract IDs. `fixture-requirements.json` maps each implemented contract to positive and negative parser/test markers enforced by `tooling/qa/rules/verify_contracts.py`. Ignored `.tmp/**` briefs are not contract sources.

When a contract changes, update the index, fixture requirements, Rust DTO/parser tests, positive and negative fixtures, and every external fixture consumer in the same change. Negative proof covers version mismatch, malformed or oversized fields, stale authority, duplicate/replay behavior, and framing and chunk budgets. It also covers identifiers, filenames, accepted settings payloads, and operation failures as applicable.

[ADR 0006](../../docs/architecture/adr/0006-cursor-trajectory-blocks.md) fixes the jointly agreed cursor-trajectory-v1 attachment contract and frozen shared fixtures. Implementation must add its Contract IDs, parser wiring, and positive/negative proof together. Agreement alone does not add implemented Contract IDs to this inventory.

[ADR 0007](../../docs/architecture/adr/0007-recording-events-blocks.md) owns recording-events-v1, including the completed anonymous normal keyboard-hold extension. Its [shared hold vectors](fixtures/recording-events-keyboard-hold-v1.json) supplement the unchanged main fixture and exercise both original-byte admission and rejected privacy, timing, field, and integer variants.
