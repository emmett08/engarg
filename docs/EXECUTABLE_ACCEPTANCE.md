# Executable acceptance matrix

`tests/executable_modes.rs` compiles all seven EngArg inference modes with the
same production compiler, moves only `case.eal`, `tools.toml` and concrete tool
implementations to a new workspace, destroys the authoring workspace, and runs
the installed `eal-host`. It checks acquired records, actual native calculation
details and final claim status separately. No mocked host, support-graph executor,
registry service or semantic sidecar participates.

The suite's deterministic input facts are synthetic, identified records. They
establish the finite test bench's behaviour; they do not verify a live scientific
study or assert that an arbitrary model is applicable. Each mode has its own
explicit design record rather than a generic always-true applicability field.
Source failure, changed design and expired observations are separate test worlds.

| EngArg mode | Independent positive result | Measured negative result | Acquired applicability basis |
|---|---|---|---|
| Deductive | `p` and `p implies q` entail `q` | The same premises have a countermodel for `r` | Identified finite snapshot, case-sensitive equality and explicit interpretation of count/distinctness as premises |
| Inductive | 90/100 Bernoulli successes give a 95% Wilson lower bound `0.8256343384950865`, above `0.8` | 50/100 gives a lower bound below `0.5`, failing the same `0.8` claim | Identified independent Bernoulli sample and fixed 100-trial stopping design |
| Abductive | Equal priors and likelihoods `0.9, 0.1` yield posterior `P(A)=0.9` | Swapping likelihoods yields `P(A)=0.1` | Identified two-candidate hypothesis space, equal-prior basis and calibration record |
| Causal | Treatment `[7,9]` minus control `[1,3]` has mean contrast `6` | Reversing groups gives `-6` | Explicit randomised assignment record and no-interference fixture condition |
| Counterfactual | `X=1`, `Y=2X+1`, intervention `X=4` gives difference `6` | Intervention `X=1` gives difference `0` | Identified affine model, fixed exogenous values and seconds as outcome unit |
| Analogical | Both selected interface/load features match, so match fraction is `1`; the separate scoped transfer review is accepted | A changed relevant interface gives fraction `0.5` | Identified source/target, observed source duration, recorded region difference, and explicit relevance/transfer judgement conditional on matching interface/load |
| Practical | Under cost cap £10, minimum observed duration chooses B; feasible ordering is `[B,A]` | Raising B's observed cost to £20 makes A the policy's choice | Three acquired options with duration consequences, cost and availability; identified objective/trade-off review and separately executed decision policy |

Every row has each of these composed executions:

| World | Required observation/computation outcome | Claim expectation |
|---|---|---|
| Positive | Fresh acquired evidence, applicable design, completed calculation/policy | Supported |
| Measured negative | Tool records remain `ok`; native method completes with the independently specified adverse result, or the policy selects A | Unsupported; the negative result remains visible |
| Unavailable | The measurement source fails and required downstream inputs cannot be acquired | Unsupported |
| Stale | The source retains its original 2020 observation time at the fixed 2026 assessment clock | Unsupported |
| Inapplicable design | The acquired sampling/model/decision design differs from the exact required design record | Unsupported |
| Material assumption challenge | An acquired audit activates an undercutting objection against the critical applicability assumption; resolution is false | Unsupported |
| Resolved challenge | Acquired audit records the challenge inactive and its resolution true | Supported |
| Challenge unavailable | The audit source fails; the required material resolution remains unavailable | Unsupported |
| Exhausted calculation | Standard native method work allowance is insufficient, or the concrete decision policy exhausts its explicit option-visit allowance | Unsupported; no successful partial calculation |

For six standard native methods, `method_work=1` produces an actual `incomplete`
method result with empty details, and the assessment reports incomplete execution.
The charge is a conservative structural admission budget; its formulas and
limits are specified in EAL's `docs/native-method-work.md`. This is not acquisition
failure substituted for computational exhaustion. The practical policy is custom
code, so it implements its own declared one-unit-per-option visit budget. All
three options must be visited before a choice exists. With a one-unit allowance,
the actual loop visits one option and retains a calculation record with status
`incomplete`, allowance/performed/required counts `1/1/3`, and no selection or
feasible-order result. Acquisition of that record succeeds; decision acceptance
requires both status `complete` and selection B. This keeps acquisition status,
policy completion and claim support distinct without exposing private tool stderr.
The acquired options remain retained.

The practical row is not a new universal optimiser or a generic `practical/1`
solver. `structured/1` preserves the explicit scoped judgement and the measured
policy result. A native solver-work cell for purely structured judgement is
mathematically inapplicable: it performs no such calculation. The concrete policy
has a real finite computation and therefore a meaningful exhaustion case.

Analogical and practical reviews also run rejected, inconclusive, wrong-scope,
missing-reviewer, missing-review-time, stale nested review time, malformed reasons
and blank-reviewer worlds. An additional analogical adverse review rejects the
transfer because of a relevant disanalogy. These reviews cannot be replaced by
`match_fraction` or by a generic Boolean approval.

EAL's `temporal/1` is not an EngArg inference label. Its native work admission is
tested in EAL alongside all six native EngArg methods, including exact budget
boundary equality. Optional engineering and external method extensions have their
own focused suites and bounds; this composed matrix does not imply universal
coverage of every user-installed method. File, MCP, HTTP, generated-tool,
source-tampering, exact-number and external worker resource contracts are covered
by their dedicated integration suites.

To run this matrix after installing the selected EAL build:

```sh
cargo test --test executable_modes
ENGARG_EAL_HOST=/absolute/path/to/eal-host \
  cargo test --test executable_modes -- --ignored
```

The first command validates all 63 authored mode/world fixtures. The installed
suite executes 63 mode/world combinations plus 17 scoped-review adversities. A
successful run, rather than fixture validation alone, is required for acceptance.
