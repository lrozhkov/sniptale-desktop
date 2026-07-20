# Native contracts

`contract-index.json` is the tracked inventory of wire Contract IDs. `fixture-requirements.json` maps each implemented contract to positive and negative parser/test markers enforced by `tooling/qa/rules/verify_contracts.py`. Ignored `.tmp/**` briefs are not contract sources.

When a contract changes, update the index, fixture requirements, Rust DTO/parser tests, positive and negative fixtures, and every external fixture consumer in the same change. Negative proof covers version mismatch, malformed or oversized fields, stale authority, duplicate/replay behavior, framing and chunk budgets, identifiers, filenames, accepted settings payloads, and operation failures as applicable.
