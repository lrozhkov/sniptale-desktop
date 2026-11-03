# Independent visual review

Complete implementation and one batched inspection before review. Fix observed defects together and capture the confirmation matrix. Do not use repeated screenshot loops as the implementation method.

## Review packet

Give a fresh review agent the user brief, confirmed constraints, component paths, screenshots, design rules, and known integration limits. Include every viewport and theme that the review must assess. Supply actual captures rather than descriptions of expected pixels.

The reviewer reads the supplied evidence and implementation. It does not edit source or start another automation campaign. It checks task clarity, visual consistency, accessibility, adaptation, and complete relevant states.

## Review result

Require an evidence summary, material findings, exact affected scope, unproven boundaries, and a verdict. A finding needs a concrete defect and user impact. Separate observations from assumptions.

Use these verdicts:

- Ship: the reviewed scope meets its requirements.
- Fix: specific material defects require one correction batch.
- Recapture: evidence is invalid or incomplete.
- Rebuild: the reviewed structure fails the accepted direction.

A recapture verdict does not approve implementation. A fix verdict requires corrections and a verdict on those corrections. A rebuild verdict requires a new complete review. Stop when a round resolves nothing and report the remaining findings.

Do not claim operating-system support from browser fixtures. Do not claim release readiness from visual approval. Required architecture, security, package, and release checks remain separate.
