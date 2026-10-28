# Completed qualification of the real metadata claim

The [recorded EAL result](result.json) supports the current metadata claim for
the exact immutable Python image and roots. On 28 October 2026 at
12:26:39 UTC, the generated tool read 50 distributions, checked 93 applicable
dependency constraints and found zero violations. EAL returned `valid: true`,
`complete: true`, and current-claim status `supported`. All 43 collected records
have status `ok`.

The result relies on the declared trusted host and actual **AI source and
formalisation assessments**. The reviewers identify their roles and shared AI
orchestration/context. These records claim no independent human or institutional
approval. The stronger runtime-smoke target retains its separate unavailable
requirements and remains unsupported.

## What changed

The earlier investigation generated and ran a real collector but left binding,
implementation and challenge requirements authored as `unavailable`. It also
left the warrant's reasoning method unresolved. Supplying affirmative review
files could not discharge those placeholders.

The public `engarg eal revise` workflow admitted the explicit
[argument](revision/argument.json) and [support](revision/support.json).
[Revision history](revision/history.json) records parent identities and
invalidation of derived metadata; [preview](revision/preview.json) retains the
dissent change context. Original acceptance clauses remain attached. R1–R5 keep
their original predicates and the one-hour source limit remains in force.

| Obligation | Completed evidence route |
|---|---|
| R6 / A1: host correspondence | 22 native identity comparisons over actual profile, verification, manifest and successful host-record fields, plus the native receipt's `passed` value |
| R7 / G3: implementation correspondence | Actual scoped AI review of the pinned source, operation, frozen cases and runtime assumptions; supported |
| R9 / D1 activation | Actual scoped review rejected the positive counterexample criterion; no retained omission witness was found |
| R10 / D1 resolution | Separate positive coverage/invariant assessment and frozen verification; supported |
| R12 / W1: formal correspondence | A different AI review role assessed the exact warrant/query and preserved dependencies; supported |

G3 makes the implementation conditional explicit. Native `deductive/1` checks
modus ponens from the accepted audit and that separately qualified conditional.
It does not prove the collector's algorithm. Host applicability, implementation
qualification, challenge resolution and formal correspondence remain mandatory
gates. D1 stays material and open in the authored source, so its resolution gate
is preserved rather than removed by an authored `resolved` label.

R9's raw review is a valid `ok` observation with verdict `rejected`; its positive
activation predicate therefore does not pass. This neither proves that no
counterexample can exist nor supplies R10 resolution. The actual R10 assessment
provides that separate evidence.

## Native generation and source correction

Fresh generation and verification ran through `engarg eal implement`, with real
`gpt-6.1-sol` calls. The final strict attempt used two model calls and passed all
11 native cases. The operator [toolchain](toolchain.json) pins the exact image
and asserts Python 3.13 and Packaging 26.3 in its build command. The same pinned
guard runs before the deployed collector.

An earlier fresh candidate passed nine finite cases, but source review found a
gap: `Distribution.read_text` can suppress metadata read errors and fall back
to `PKG-INFO`. The source-error requirement was clarified through another public
revision, and EngArg regenerated the source and frozen cases. The final reader
falls back only when `METADATA` is absent. Its frozen directory-valued
`METADATA` case expects acquisition failure. Generated source and frozen oracles
were never manually repaired. [Source-review notes](source-review/strict-reader.json)
and the [native receipt](package/tools/generated/dependency_audit/verification.json)
record the distinction between finite tests and implementation assessment.

## Inspect the observations and controls

- [Positive collection](positive/collection.json) and
  [assessment](positive/assessment.json) contain the completed run.
- [Missing-binding control](missing-review-binding/assessment.json) retains the
  same executable source and removes only the operator review binding. The
  current claim becomes unsupported. EAL now reports the primary retained
  `Tool 'engarg_review_contract' is not in the operator registry` failure before
  its secondary digest/status/value diagnostics.
- [Wrong-scope control](wrong-review-scope/assessment.json) deliberately alters
  only the R12 review value's image selector in a labelled copy and updates that
  control file's operator pin. The expected request, source, code, predicates and
  original review time remain unchanged. The native importer rejects the copy
  and the current claim becomes unsupported. The altered copy is not a genuine
  reviewer decision.

The raw host artifact and its embedded packet copy retain the earlier
pre-qualification baseline, including that baseline's missing-review errors.
R6 selects its successful audit record using the recorded JSON pointer. These
historical errors are not failed top-level observations in the completed run.
Likewise, `build.json.pending_reviews` is the static request list; actual
fulfilment is recorded in the execution assessment.

## Execute and preserve the clocks

The executable [package](package/case.eal) and [bindings](package/tools.toml)
are exact native build outputs. With the exact local image available and the
paired EAL companions on `PATH`, run from the repository root:

```sh
"$EAL_HOST" run \
  examples/assess-claim/python-dependencies/qualified/package/case.eal \
  --tools examples/assess-claim/python-dependencies/qualified/package/tools.toml \
  --workspace work/qualified-metadata-replay-001 \
  --claim claim_63757272656e745f636c61696d --timeout 300
```

The artifact envelope retains its conservative original time of 12:05:45 UTC;
the host audit retains 12:11:25 UTC; reviews retain their actual 12:22–12:23 UTC
times. Reading or copying them does not refresh them. A later replay can
correctly be unsupported because the original one-hour limit has expired.
For a new investigation, obtain fresh native artifacts and actual scoped
assessments; do not change these clocks or reuse these reviews for another
source, scope or image. A rebuilt image with a different ID is a new target.

To replay a recorded adverse world, first copy `package/` into a new directory
and overlay that world's `tools.toml` there. For the wrong-scope world, also
overlay `altered-review-inputs.json` as `review-inputs.json`. Resource paths are
relative to the manifest directory. The recorded controls ran within the same
freshness window; later replay may additionally report staleness.

The admitted reports, provider audits and exact command vectors remain in the
investigation workspace. Published canonical source, revision identities,
native raw artifacts, exact review decisions and execution extracts support
inspection without reauthoring the investigation or inventing missing reviews.
