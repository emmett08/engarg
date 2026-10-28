# Controlled Bayesian diagnosis with a generated pgmpy tool

[Browse the argument in HTML](argument.html). The presentation keeps the authored
conditional state beside the dated EAL result. Rendering preserves the original
observation and review times.

This investigation computes a consequence of the stipulated **PS-DIAG/1** model.
It makes no claim about hardware failure rates, a diagnostic population or
empirical causal validity. The [qualified native run](recorded/positive/execution.json)
returned `valid: true`, `complete: true`, and **supported** for both the current
claim and its bounded reusable-qualification target at
`2026-10-28T15:58:14.098575Z`. All 46 collected records have status `ok`.
The actual selected-run observation is `posterior_fault=0.756218905472637`,
`evidence_normalizer=0.0201` and `calculation_complete=true`. Support depends on
the separately admitted actual scoped AI assessments, not those numbers alone.

The graph is `Fault -> Alarm` and `Fault -> HeartbeatLost`, with sensor flags
conditionally independent given Fault. Fault states 0/1 mean healthy/faulty;
sensor states 0/1 mean false/true. The selected input uses prior 0.02, alarm
likelihoods 0.95/0.05 and heartbeat-loss likelihoods 0.8/0.1 for faulty/healthy,
with both observed flags true. These probabilities are model definitions.

| Frozen successful case | Exact posterior | Exact evidence mass | Recorded representation |
|---|---|---|---|
| Selected: both flags true | `152/201` | `201/10000` | `0.756218905472637`, `0.0201` |
| Adverse: heartbeat loss false | `38/479` | `479/10000` | `0.0793319415448852`, `0.0479` |
| Different valid parameters | `6/7` | `7/25` | `0.857142857142857`, `0.28` |

The first case exceeds `3/4` by exactly `5/804`. The adverse host run actually
returned the second row with completion true; it is a successful calculation
below the threshold for a different event. The third row is a native frozen
qualification case, rather than another selected-claim observation.

## What actually ran

Public `engarg analyse` authoring used two real model calls. Final public
`engarg eal implement` generation used two further calls and passed all twelve
native cases: the three successful rows, malformed/type/range/nonfinite inputs,
impossible zero-mass evidence and protocol controls. The generated collector uses
pgmpy's public `DiscreteBayesianNetwork`, `TabularCPD` and
`VariableElimination.query` APIs. No collector or frozen oracle was manually
repaired. Source matching is a separate assessment from finite test success.
Including the unsuccessful attempts, this investigation used nine provider calls:
two for argument authoring and seven for implementation attempts.

Earlier generation/verification attempts did not qualify. A public numerical
revision made uniform fifteen-significant-digit representation part of the
operation before fresh final generation. The exact rational references and
`1e-12` bound were retained; generated source, frozen oracles and receipts were
never manually edited to obtain a pass. The
[initial authoring input](recorded/authoring-request.json) remains preserved.
The fresh starter `request.json` includes the accepted representation-policy
appendix, so it is distinct from that historical initial input.

The prepared runtime supplies pgmpy 1.1.2 with its NumPy backend, without Torch.
Its immutable image is
`sha256:5d317440653dd5a188223baff22644397cc2a1bd071183a12e9c4cb2f3688f7f`.
The [package](recorded/package/case.eal), native receipts and operator binding
retain the actual source, contract, profile and execution identities. The
recorded EngArg source is `6fcb124fdf78f8d8c9b35b7753945a8525790832`; the paired
EAL runtime source is `030d4514e9c5e01c95690e0d921a5da2cf6525af`.
EngArg was built before that commit: the original freeze receipt retains its
base HEAD, while all 360 frozen source-file hashes match the cited commit.

The fractions above are exact mathematical references. Successful library
outputs are validated, then represented uniformly using fifteen significant
decimal digits, and checked again. Selected-case reference and normalization
bounds remain absolute `1e-12`. This is neither fixed-place rounding nor a claim
of exact binary-float equality, unrestricted input accuracy or arbitrary
near-threshold decision stability.

## Qualification and controls

The [argument](revision/argument.json), [support](revision/support.json) and
[history](revision/history.json) retain all fifteen original requirements and add
two explicit conditional-premise obligations: seventeen requirements in total.
Three numerical/completion predicates remain native observations; qualification
uses fourteen actual scoped AI review records. Model transcription,
rational arithmetic, analytic consequence, implementation correspondence,
binding, reusable qualification and the material mapping/substitution challenges
retain their own criteria.

The arithmetic and stronger-target conditionals have separate records from their
retained warrant checks. Related records may share the same reviewer and
calculation; they are not independent confirmations. Reviewers disclose their AI
roles, code/image contributions and shared task context. No human approval or
institutional independence is claimed. The three native `deductive/1` bindings compose
accepted premises; they do not prove arithmetic, premise truth or collector
algorithm correctness.

The [final audit](verification/final-audit.json) passed 447 checks. The pinned
`actual-artifacts/host_record.json` preserves the earlier baseline, including its
then-missing review errors. The completed positive execution has 46 successful
records. Likewise, `build.json.pending_reviews` is the static request list;
actual fulfilment appears in the execution and retained review records.

- [Positive execution](recorded/positive/execution.json): complete qualified
  support under the selected scope; required qualification and entailment gates
  pass. The two positive challenge-activation criteria are rejected, with separate
  supported resolution assessments.
- [Adverse execution](recorded/adverse/execution.json): the changed-heartbeat
  input produces a valid low posterior, so its threshold predicate fails. This is
  a measured-predicate control with a changed scope and unresolved applicable
  reviews, not an isolated full-claim negative using the selected-case reviews.
- [Missing review binding](recorded/missing-review-binding/execution.json): a
  deliberate removal of only the operator review importer makes both claims
  unsupported while the numerical acquisition succeeds unchanged. Source,
  generated code, receipts and original review values/times remain identical.

Incomplete collection, unavailable reviews, a measured negative and acquisition
failure are different outcomes. A zero evidence mass makes conditioning undefined
and must produce acquisition error. Neither an unsupported claim nor incomplete
coverage licenses inventing a review or treating a collector's completion flag
as independent qualification.

## Start a fresh investigation or replay the exact package

From the repository root, set `case_dir` to this example's directory. A fresh
investigation obtains its own generated artifacts and later actual assessments:

```sh
case_dir=examples/assess-claim/bayesian-diagnosis
bash scripts/assess-claim.sh --input "$case_dir/request.json" \
  --toolchain "$case_dir/toolchain.json" --tool-language python \
  --eal-host "$EAL_HOST" --model "$ENGARG_MODEL" \
  --api-key-env OPENAI_API_KEY --output-dir work/psdiag-fresh-001
```

With the exact local image and paired EAL companions available, replay the native
package without reauthoring:

```sh
"$EAL_HOST" run "$case_dir/recorded/package/case.eal" \
  --tools "$case_dir/recorded/package/tools.toml" \
  --workspace work/psdiag-replay-001 \
  --claim claim_63757272656e745f636c61696d --timeout 300
```

To replay a control, copy `recorded/package/` into a new directory. For the
missing-binding control, overlay its `tools.toml`. For the changed-heartbeat
control, overlay its `case.eal`, `tools.toml` and `build.json`. The generated
implementation files are byte-identical across all three executions. Run the
copied package with a new workspace.

The [image preparation guide](../../../docs/library-selection.md#prepare-the-dependencies)
explains trusted dependency installation. A rebuilt image is a new target:
use its actual immutable ID in a new request and toolchain profile and obtain
fresh applicable assessments.

The qualification baseline retains `2026-10-28T15:39:22.773512Z`; the adverse
host record retains `2026-10-28T15:43:21.480876Z`. The positive query was newly
observed at `2026-10-28T15:58:11.819870Z`. Each review retains its actual review
time, and the qualified proposition interval stays anchored to the baseline.
The authorized maximum age is **86400 seconds**; reading or copying records
does not refresh their clocks. Later replay may correctly become unsupported
through staleness. A new image identity, source or model/evidence scope requires
fresh applicable records, rather than relabelling these results.
