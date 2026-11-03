---
name: impeccable
description: Design and review Sniptale web interfaces. Use for components, capture controls, settings, accessibility, localization, visual refinement, and shared design tokens.
metadata:
  version: 4.5.2-desktop.1
---

# Impeccable for Sniptale Desktop

This skill adapts impeccable 4.5.2 from Sniptale web. It retains the Operate guidance, quality requirements, bounded inspection, and independent review. Repository instructions and the user brief take precedence.

## Establish context

Read PRODUCT.md, DESIGN.md, and the applicable surface owner. Inspect the actual Sniptale web component and its states before implementation. Use the existing design system when the user selects it. Do not invent a replacement identity.

Ask only about material unknowns. Do not repeat settled questions or request another approval when the user authorizes autonomous work.

Read [Operate](references/operate.md) for application surfaces. Read [Quality requirements](references/quality.md) immediately before UI edits. Use [Review](references/review.md) for completed visual work.

## Implement

Use the real application components in the browser sandbox. Connect fixtures through the typed client. Keep state authority and command validation in Rust. Follow ADR 0008.

Keep the capture toolbar compact. Use compact appearance and language controls. Preserve shared fonts, semantic colors, icon geometry, and interaction conventions. Treat settings as a secondary surface.

Implement the complete requested behavior before visual inspection. Preserve content, product facts, security boundaries, and accepted workflows. Mark simulations and unsupported functions clearly.

## Inspect and finish

Inspect the relevant viewport, theme, language, scale, and interaction matrix in one batch. Include screenshots and defect inspection in that batch. Fix the observed defects together. Run one confirmation batch. Do not start an open-ended polishing loop.

Use an independent review agent after the confirmation batch. Supply the original brief, changed components, screenshots, design rules, and known limits. The reviewer must not edit implementation.

Act on material review findings in one batch. Confirm the affected behavior and obtain a verdict on those fixes. Stop when a round makes no progress. Report unresolved material findings honestly.

Update design documentation after the final corrections. Record actual evidence and unfinished boundaries. Use the repository QA pipeline and required architecture/security reviews. This visual review does not replace them.
