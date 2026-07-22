---
name: necessist-audit
description: Audit passing removals stored in necessist.db for bugs in project-owned non-test code. Use when reviewing Necessist results for behavior that violates an intended contract, invariant, or specification.
---

# Audit Necessist results

Use passing removals as leads for finding bugs in the code being tested.

Do not modify project source unless the user explicitly requests changes. Running Necessist and allowing it to create `necessist.db` is permitted.

## Scope

Analyze only removals whose outcome is `passed`. Treat tests as evidence about intended behavior, not as the subject of findings.

Report only defects in project-owned non-test code. Do not report defects confined to tests, test helpers, fixtures, mocks, test configuration, generated code, vendored code, or third-party dependencies. Mention excluded defects only to explain why a removal does not establish a production-code finding.

## Locate results

Look for `necessist.db` in the current directory. If it does not exist, run `necessist` there and use the resulting database. If Necessist is unavailable or the run fails, report the error and ask the user how to proceed.

Read passing removals from `necessist.db` with `necessist --dump` or a read-only SQLite query.

## Investigate removals

For each passing removal:

1. Inspect the removal and the complete affected test.
2. Infer the intended production behavior from the test, documentation, specifications, types, comments, and related tests. Trace how the removed operation should influence that behavior.
3. Determine why the test passes without that influence. If the explanation is confined to the test, do not pursue it as a finding.
4. Form a concrete production-bug hypothesis and seek independent supporting or refuting evidence in the implementation, callers, and focused non-mutating diagnostics.
5. Consider benign explanations, including idempotence, duplicate setup, unreachable conditions, equivalent operations, nondeterminism, and persistent state. Do not treat a single rerun as proof that a flaky result is stable.
6. Before reporting a finding or lead, confirm that its recorded source location and removed text match the current checkout. If they do not, mark the result as stale and recommend rerunning Necessist. Otherwise, cite repository-relative locations and the evidence supporting the conclusion.

Do not infer that a passing removal is a production bug merely because Necessist reports it.

## Classify results

Classify a result as a finding only when all of the following are established:

- a specific intended contract, invariant, or behavior and its source;
- the production-code location and mechanism that violate it;
- the triggering conditions;
- evidence connecting the Necessist removal to the behavior; and
- consideration and rejection of reasonable benign explanations.

If any required element is missing, classify the result as a lead and state what evidence is missing. When uncertain, classify the result as a lead.

## Report

Order findings by likely impact. For each finding, report:

- removed code and source location;
- production-code location;
- intended behavior and its source;
- why the test still passes;
- triggering conditions and potential impact;
- supporting evidence;
- next action for reproducing, confirming, or remediating the production-code defect.

List leads separately. End with counts of passing removals examined, production-code findings, results for which no production bug was established, and stale results. Do not report test-only defects or recommend test-only changes as resolutions.

Use concise Markdown. Do not implement recommendations unless the user asks.
