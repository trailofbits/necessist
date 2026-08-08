---
name: necessist-audit
description: Use to audit Necessist results, running Necessist first if needed, and investigate whether passing removals reveal bugs in code or tests, including test-harness bugs that let tests pass without checking intended behavior.
metadata:
  version: "0.1.0"
  license: AGPL-3.0-only
  compatibility:
    requires:
      - shell access
      - the necessist command to be available on PATH
---

# Audit Necessist results

Use passing removals as leads for finding bugs in the code or tests being audited.

Do not modify project source unless the user explicitly requests changes. Running Necessist and allowing it to create `necessist.db` is permitted.

## Scope

Analyze only removals whose outcome is `passed`.

- Do not report failed, timed-out, or nonbuildable removals as findings.
- Treat tests both as evidence about intended behavior and as possible subjects of findings.
- Report defects in project-owned code, tests, test helpers, fixtures, mocks, and test configuration.
- In tests, look for harness bugs such as missing synchronization, ineffective assertions, swallowed errors, unchecked setup, and mocks or timing assumptions that stop exercising intended behavior.
- Do not report defects confined to generated code, vendored code, or third-party dependencies.

## Locate results

Look for `necessist.db` in the current directory. If it does not exist, run `necessist` there and use the resulting database. If Necessist is unavailable or the run fails, report the error and ask the user how to proceed.

Read passing removals with `necessist --dump`. Use read-only SQLite queries only if needed.

## Investigate removals

First scan all passing removals and prioritize removals most likely to expose a real defect, including removed assertions, error checks, synchronization, channel operations, waits, cleanup, setup, and calls whose comments or names imply verification. Investigate those before benign-looking removals such as duplicate assignments, redundant initialization, platform skips, logging, or unreachable branches.

For each passing removal investigated:

1. Inspect the removal and the complete affected test.
2. Infer the intended behavior from tests, documentation, comments, related tests, and implementation. Determine why the test passes without the removed operation, and form a concrete bug hypothesis when the removal appears meaningful.
3. Seek supporting or refuting evidence in the affected test, implementation, callers, and focused non-mutating diagnostics.
4. Consider benign explanations, including idempotence, duplicate setup, unreachable conditions, equivalent operations, nondeterminism, and persistent state. Do not treat a single rerun as proof that a flaky result is stable.
5. Before reporting a finding or lead, confirm that its recorded source location and removed text match the current checkout. If they do not, mark the result as stale and recommend rerunning Necessist. Otherwise, cite locations consistently, using paths relative to the audited directory or repository root.

Do not infer that a passing removal is a bug merely because Necessist reports it.

## Classify results

Classify a result as a finding only when all of the following are established:

- a specific intended contract, invariant, or behavior and its source;
- the affected code or test location and the mechanism that violates it;
- evidence connecting the Necessist removal to the behavior; and
- consideration and rejection of reasonable benign explanations.

If any required element is missing, classify the result as a lead and state what evidence is missing. When uncertain, classify the result as a lead.

Treat removed assertions, error checks, synchronization, channel operations, waits, cleanup, setup, and verification calls as leads unless the current checkout shows a clear benign explanation. A benign explanation must explain why the specific intended check still runs and is observed after the removal, not merely why the test process still completes. If focused diagnostics are unavailable or inconclusive, preserve the result as a lead rather than classifying it as no bug established.

For synchronization, channel operations, waits, sleeps, joins, callbacks, and goroutine or task coordination, do not treat other ordering or eventual completion as a clear benign explanation unless the current checkout shows what remaining synchronization or ordering makes the intended check run and be observed. If the removed operation may be the only reason the test waits for an asynchronous check, callback, error path, or assertion to run, classify the result as a lead even if the test has other waits or protocol-completion steps.

## Report

Report findings and leads in Markdown. After investigating each high-priority removal, immediately classify it as a finding, lead, no-bug, or stale. Write `necessist-audit.json` as soon as a credible finding is established or after a small initial batch of high-priority removals has produced only leads, then continue broader review. Write the Markdown report after the broader review is complete or the user's time or budget limit is reached.

Order findings by likely impact. For each finding, report:

- removed code and source location;
- affected code or test location;
- intended behavior and its source;
- why the test still passes;
- potential impact;
- supporting evidence;
- suggested fix.

List leads separately. End with explicit counts of passing removals examined, findings, leads, results for which no bug was established, and stale results. Those counts should match the number of passing removals examined; if they do not because some removals were skipped or could not be read, explain why.

Write the machine-readable JSON report in the audited directory; this write is permitted even though source modifications are not. Always write the file, using empty arrays when no findings or leads are established. Include only findings and leads; put each result’s explanation, evidence, impact, and suggested fix in `details`. The JSON report must satisfy this JSON Schema:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["version", "findings", "leads"],
  "properties": {
    "version": {
      "type": "string",
      "const": "0.1.0"
    },
    "findings": {
      "type": "array",
      "items": { "$ref": "#/$defs/result" }
    },
    "leads": {
      "type": "array",
      "items": { "$ref": "#/$defs/result" }
    }
  },
  "$defs": {
    "result": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "id",
        "removed_code",
        "removed_location",
        "affected_location",
        "details"
      ],
      "properties": {
        "id": { "type": "string" },
        "removed_code": { "type": "string" },
        "removed_location": { "type": "string" },
        "affected_location": { "type": "string" },
        "details": { "type": "string" }
      }
    }
  }
}
```

Use concise Markdown. Do not implement recommendations unless the user asks.
