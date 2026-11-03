---
name: asd-ste100
description: Rewrite English technical documentation and agent instructions with short, explicit sentences. Use for README, procedures, reviews, and controlled-language requests.
---

# Simplified Technical English

Apply this skill to all English repository documentation, including AGENTS.md and other skills. Preserve technical meaning and requirement strength.

## Procedure

1. Read the complete source section before you rewrite it.
2. Use strict instructions for procedures, safety rules, and agent instructions. Use descriptive prose for explanations and README.
3. Keep each instruction within 20 words. Keep each descriptive sentence within 25 words.
4. Use active voice and simple tenses. Give each sentence one instruction and each paragraph one topic.
5. Preserve conditions, exceptions, numbers, technical names, and uncertainty. Never change a possibility into a fact.
6. Use lists for three or more steps or conditions. Keep each Markdown paragraph or list item on one physical line.
7. Remove semicolons, vague claims, and unnecessary noun phrases. Prefer a direct verb.
8. Read the result against the source. Check every requirement and exception.
9. Run the repository documentation gate through the normal QA wrappers. Inspect advisory findings as well as errors.

Do not alter commands, code, contracts, or exact legal text to satisfy a prose check. Keep necessary technical terms consistent. Keep compound tenses when a simpler tense would change uncertainty or current relevance. Explain a necessary departure in the review.

## Scope and limits

The repository uses the structural principles of ASD-STE100. It does not include the official dictionary or certify full compliance. Obtain the standard from the [official request page](https://www.asd-ste100.org/STE_downloads.html) when exact dictionary compliance matters.

Read [writing rules](references/writing-rules.md) for the rule summary and local checker policy. The checker uses heuristics. A green result does not replace semantic review.

## Provenance

This skill adapts the supplied asd-ste100 skill, version 0.4.0. The original author is Dustin Yuchen Teng. The [MIT license](LICENSE) remains unchanged. The checker retains the upstream behavior with repository formatting and a type-narrowing guard. Repository prose and integration are local adaptations.
