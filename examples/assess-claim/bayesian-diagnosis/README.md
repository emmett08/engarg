# Controlled Bayesian diagnosis with a generated pgmpy tool

[Browse the argument in HTML](argument.html) to inspect the authored argument
beside its dated EAL assessment. Both views preserve the original observation
and review times.

Does an alarm indication together with a heartbeat-loss indication raise the
probability of a power-supply fault to at least `0.75`? In the stipulated
**PS-DIAG/1** model, the answer is yes: the exact conditional probability is
`152/201`, approximately `0.7562`. This example qualifies a generated pgmpy
calculation of that result. Its conclusion applies to the defined model and
selected evidence. Applying it to a physical power supply would require evidence
that the model represents that supply and its diagnostic indications.

## What PS-DIAG/1 defines

PS-DIAG/1 is a finite Bayesian network with three binary random variables. It
represents a power-supply condition and two diagnostic indications:

| Variable | State `0` | State `1` |
| --- | --- | --- |
| `Fault` | Power supply healthy | Power supply faulty |
| `Alarm` | Alarm indication false | Alarm indication true |
| `HeartbeatLost` | Heartbeat-loss indication false | Heartbeat-loss indication true |

`Alarm` and `HeartbeatLost` are evidence variables whose values are supplied to
the calculation. The model specifies their probabilities, but leaves the
physical means of producing either indication undefined. It therefore supplies
no sensor design, alarm trigger or heartbeat timeout.

The graph has edges `Fault -> Alarm` and `Fault -> HeartbeatLost`. It stipulates
that the two indications are conditionally independent given `Fault`: once a
fault state is fixed, their joint likelihood is the product of their individual
likelihoods. This gives the factorisation

```text
P(Fault, Alarm, HeartbeatLost)
  = P(Fault) P(Alarm | Fault) P(HeartbeatLost | Fault).
```

The [selected model parameters](model.json) define the prior fault probability
and the conditional probability distributions:

| Probability | Value |
| --- | ---: |
| `P(Fault=1)` | `0.02` |
| `P(Alarm=1 given Fault=1)` | `0.95` |
| `P(Alarm=1 given Fault=0)` | `0.05` |
| `P(HeartbeatLost=1 given Fault=1)` | `0.8` |
| `P(HeartbeatLost=1 given Fault=0)` | `0.1` |

Each complementary probability defines the corresponding state-`0` probability.
All probabilities are dimensionless model definitions. They were stipulated for
this investigation, rather than estimated from hardware observations. The
selected evidence sets `Alarm=1` and `HeartbeatLost=1`.

## What is claimed, and why the posterior exceeds 0.75

The current claim is that a faithful, complete finite calculation in the pinned
pgmpy 1.1.2 runtime gives

```text
P(Fault=1 | Alarm=1, HeartbeatLost=1) = 152/201 > 3/4,
```

with positive evidence probability `201/10000`. The fractions are exact model
consequences. The returned decimal values must agree with them within absolute
`1e-12` under the declared fifteen-significant-digit representation policy.

The [independent rational reference](oracle.json) derives this result by
considering the two possible fault states. For the selected evidence, the joint
probability of a fault and both indications is
`0.02 × 0.95 × 0.8 = 152/10000`. The joint probability of a healthy supply and
both indications is `0.98 × 0.05 × 0.1 = 49/10000`. Adding these mutually
exclusive cases gives the unconditional evidence probability
`P(Alarm=1, HeartbeatLost=1) = 201/10000`. This is the **evidence normaliser**,
returned as `evidence_normalizer`. Dividing the faulty joint probability by this
sum gives `152/201`, which exceeds `3/4` by exactly `5/804`.

The software claim also requires evidence that the prescribed library
calculation ran for this model and evidence event. A plausible number could
otherwise result from a state-order error, a different conditional event or a
constant answer. The native acceptance predicates require
`posterior_fault >= 0.75`, a finite positive `evidence_normalizer` and
`calculation_complete == true`. Completion means that the declared finite
inference and validation steps finished. Source assessments and execution-binding
checks establish whether those outputs describe the required computation.

The [qualified native run](recorded/positive/execution.json) returned
`posterior_fault=0.756218905472637`, `evidence_normalizer=0.0201` and
`calculation_complete=true`. At `2026-10-28T15:58:14.098575Z`, EAL reported
`valid: true`, `complete: true` and **supported** for the current claim and its
stronger target. All 46 collected records had status `ok`. These are dated
results under the admitted review, runtime and record-age conditions.

The stronger target additionally requires source assessment and qualification
of the reusable `finite_bayes_diagnosis/1` operation against independently frozen
cases. The operation accepts the five probability parameters and two Boolean
evidence values for this fixed graph. Qualification tests parameter handling,
state mapping and error behaviour. Its finite set of cases supports that bounded
qualification, with unrestricted input accuracy and universal algorithm
correctness remaining outside the established result.

| Successful calculation case | Exact posterior | Exact evidence probability | Recorded posterior, evidence normaliser |
| --- | --- | --- | --- |
| Selected: `Alarm=1`, `HeartbeatLost=1` | `152/201` | `201/10000` | `0.756218905472637`, `0.0201` |
| Adverse: `Alarm=1`, `HeartbeatLost=0` | `38/479` | `479/10000` | `0.0793319415448852`, `0.0479` |
| Different valid parameters | `6/7` | `7/25` | `0.857142857142857`, `0.28` |

Changing only `HeartbeatLost` to `0` produces a posterior below the threshold.
The adverse host run returned that result with `calculation_complete=true`,
showing that a completed calculation can fail the posterior acceptance
predicate. It concerns a different evidence event. The third row is a frozen
qualification case that checks whether the operation responds to changed
parameters.

## What actually ran

`engarg analyse` used two provider calls to author the argument. The final
`engarg eal implement` attempt used two further calls to generate the collector,
which passed all twelve native cases. These cover the three successful
calculations above, invalid inputs, impossible evidence and protocol controls.
Invalid inputs include malformed structures, incorrect types, out-of-range
probabilities and non-finite values. Impossible evidence has zero probability,
so conditioning on it is undefined and must produce an acquisition error.

The generated collector uses pgmpy's public `DiscreteBayesianNetwork`,
`TabularCPD` and `VariableElimination.query` APIs. Source inspection separately
assesses whether it performs the specified library computation. Across all
implementation attempts, including those that failed qualification, the
investigation used nine provider calls: two for argument authoring and seven for
implementation. The generated collector and independently frozen oracle files
retain their original contents.

Earlier generation and verification attempts failed qualification. Before the
final generation, an explicit numerical revision made uniform
fifteen-significant-digit representation part of the operation while retaining
the exact rational references and absolute `1e-12` bound. The
[initial authoring input](recorded/authoring-request.json) remains preserved.
The fresh-investigation starter, [request.json](request.json), includes the
subsequently accepted representation policy.

The prepared runtime supplies pgmpy 1.1.2 with its NumPy backend, without Torch.
Its immutable image is
`sha256:5d317440653dd5a188223baff22644397cc2a1bd071183a12e9c4cb2f3688f7f`.
The [package](recorded/package/case.eal), native receipts and operator binding
identify the source, contract, runtime profile and execution. The
recorded EngArg source is `6fcb124fdf78f8d8c9b35b7753945a8525790832`; the paired
EAL runtime source is `030d4514e9c5e01c95690e0d921a5da2cf6525af`.
EngArg was built before that commit: the original freeze receipt retains its
base HEAD, while all 360 frozen source-file hashes match the cited commit.

After validating the raw library outputs, the collector represents successful
probabilities using fifteen significant decimal digits and checks their ranges
again. The selected-case reference comparisons and posterior normalisation
checks use absolute `1e-12` bounds. Significant digits govern the whole decimal
representation, so the number of decimal places varies with magnitude.
Agreement within these bounds establishes the declared numerical checks.
Decision stability for arbitrary inputs close to a threshold would require
further numerical analysis.

## Qualification and controls

The [argument](revision/argument.json), [support](revision/support.json) and
[history](revision/history.json) retain all fifteen original requirements and add
two explicit conditional-premise obligations: seventeen requirements in total.
Three requirements check the acquired posterior, evidence normaliser and
completion result through native predicates. The other fourteen use recorded AI
assessments with explicit criteria. These assess the model transcription and
arithmetic, then connect the analytic result to the implementation and its bound
execution. Additional assessments qualify the reusable operation and address the
state-mapping and implementation-substitution challenges.

The arithmetic and stronger-target conditionals have records separate from the
checks on their inference rules. Related records may share a reviewer and a
calculation, so their contribution is the fulfilment of distinct criteria.
Reviewers disclose their AI roles, code and image contributions, and shared task
context. These are AI assessments with overlapping evidence. The three native
`deductive/1` bindings check the propositional consequences of accepted premises.
Arithmetic correctness, premise truth and implementation correspondence depend
on the separately assessed evidence.

The [final audit](verification/final-audit.json) passed 447 checks. Within the
recorded package, `actual-artifacts/host_record.json` preserves the earlier
baseline, including the errors caused by reviews that were then missing.
`build.json.pending_reviews` lists the requested reviews. Their eventual
fulfilment appears in the execution and retained review records.

- [Positive execution](recorded/positive/execution.json): the selected input
  passes the required qualification and entailment checks. Both criteria for
  activating a challenge are rejected, and separate assessments support the
  resolution of each challenge.
- [Adverse execution](recorded/adverse/execution.json): changing the heartbeat-loss
  evidence value makes the posterior threshold predicate fail. Applicable
  reviews also remain unresolved under this changed scope. The control therefore
  demonstrates the numerical predicate's response to a changed input, while the
  effect on the full claim also depends on those review conditions.
- [Missing review binding](recorded/missing-review-binding/execution.json): a
  deliberate removal of only the operator review importer makes both claims
  unsupported while the numerical acquisition succeeds unchanged. The source,
  generated code, receipts, review values and review times remain identical.

These controls distinguish a failed acceptance predicate from unavailable
reviews and acquisition failure. The Boolean `calculation_complete` result
reports completion of the declared calculation. Acceptance of the claim also
requires the applicable qualification records and complete evidence collection.
An unavailable review remains an unresolved requirement.

## Start a fresh investigation or replay the exact package

From the repository root, set `case_dir` to this example's directory. A fresh
investigation generates its own artefacts and requires subsequent assessments:

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

The qualification baseline is dated `2026-10-28T15:39:22.773512Z`, and the
adverse host record is dated `2026-10-28T15:43:21.480876Z`. The positive query
was observed at `2026-10-28T15:58:11.819870Z`. Each review retains its review
time, and the qualified proposition interval stays anchored to the baseline.
The authorised maximum age is **86400 seconds**, measured from the original
record times. Later replay may therefore become unsupported through staleness.
A changed image identity, source or model and evidence scope requires fresh
applicable records.

The retained requests and argument records preserve their original wording and
hashes. Where those records use “sensor” or “flag” for `Alarm` and `HeartbeatLost`,
the terms denote the Boolean diagnostic indications defined above.
