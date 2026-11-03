# Writing rules

Use short sentences with explicit actors and actions. Preserve the source meaning before you improve its wording.

| Area | Rule |
| --- | --- |
| Instructions | Use one instruction per sentence. Use at most 20 words. |
| Descriptions | Use at most 25 words per sentence. |
| Paragraphs | Use one topic and at most six sentences. |
| Voice | Prefer active voice. Retain passive voice only when the actor is unknown or irrelevant. |
| Verbs | Use simple tenses and direct verbs. Avoid phrasal verbs and actions expressed as nouns. |
| Terms | Keep technical terms and their meanings consistent. Avoid noun groups longer than three words where practical. |
| Precision | Preserve modality, conditions, scope, numbers, exceptions, and technical names. |
| Structure | Use lists for three or more steps or conditions. Do not use semicolons in prose. |

## Checker policy

The [repository checker](../../../../tooling/qa/rules/ste_lint.py) checks structural patterns without external dependencies. Run it with `--selftest` to check its negative fixtures. Use `--json` for machine output.

The repository documentation gate checks every registered Markdown document. It preserves generated notices, exact legal texts, code blocks, inline code, link targets, and YAML frontmatter. The gate reports lexical consistency, passive voice, and compound tenses as advice. Different technical actions can require different verbs. Do not replace technical terms solely to silence a heuristic.

Structural errors block the gate without a baseline. Human review must check sentence meaning and the stricter 20-word instruction limit. The checker enforces the 25-word descriptive limit.

## Sources

These instructions paraphrase rule categories. They do not reproduce the official dictionary or standard text.

- [ASD-STE100](https://www.asd-ste100.org/) owns the standard.
- [About STE](https://www.asd-ste100.org/about_STE.html) describes its purpose.
- [Official request page](https://www.asd-ste100.org/STE_downloads.html) provides access to the standard.
