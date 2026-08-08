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

For each passing removal:

1. Inspect the removal and the complete affected test.
2. Infer the intended behavior from tests, documentation, comments, related tests, and implementation. Determine why the test passes without the removed operation, and form a concrete bug hypothesis when the removal appears meaningful.
3. Seek supporting or refuting evidence in the affected test, implementation, callers, and focused non-mutating diagnostics.
4. Consider benign explanations, including idempotence, duplicate setup, unreachable conditions, equivalent operations, nondeterminism, and persistent state. Do not treat a single rerun as proof that a flaky result is stable.
5. Before reporting a finding or lead, confirm that its recorded source location and removed text match the current checkout. If they do not, mark the result as stale and recommend rerunning Necessist. Otherwise, cite repository-relative locations and the evidence supporting the conclusion.

Do not infer that a passing removal is a bug merely because Necessist reports it.

## Classify results

Classify a result as a finding only when all of the following are established:

- a specific intended contract, invariant, or behavior and its source;
- the affected code or test location and the mechanism that violates it;
- evidence connecting the Necessist removal to the behavior; and
- consideration and rejection of reasonable benign explanations.

If any required element is missing, classify the result as a lead and state what evidence is missing. When uncertain, classify the result as a lead.

## Report

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
