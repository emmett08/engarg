# Evaluate whether the workflow improves engineering decisions

[Documentation](index.md) · [Implementation verification](VALIDATION.md)

Passing software checks establishes that the declared workflow behaves as tested.
It leaves a practical question: does the workflow help engineers identify the
right claim, obtain decisive evidence and make a better decision at an acceptable
cost? For the CI example, a polished argument is useful only if it exposes such
issues as resource contention, omitted tests or incomparable timing boundaries
before the team relies on the proposed speed-up.

Evaluate that question with identified cases, independent judgements and a
resource budget fixed before inspecting the outputs. Keep failures in the study.
A method that quietly excludes difficult cases or weakens their requirements can
appear effective while leaving the original engineering problem unresolved.

## Freeze cases and reference judgements

Choose engineering cases that exercise the intended use. A practical initial
study can freeze 30–50 cases spanning claim-only construction, missing support,
credible support, conflicting observations, causal overclaiming, version
mismatches and boundary failures. Include a supported narrow claim with an
unsupported stronger target, mathematical claims needing proof and empirical
claims needing measurements. Keep reference assessments outside model inputs.

Have at least two engineers independently identify each case's strongest
supportable formulation, decisive inference, critical assumptions and material
challenges. Specify the smallest investigation that could resolve each decisive
gap. Record disagreements and their reasons, then reconcile them transparently
or retain competing justified assessments.

Adversarial cases should probe particular errors. An articulate but irrelevant
citation tests source relevance. A measured slowdown under resource contention
tests willingness to retain adverse findings. Instructions embedded in a source
excerpt test the data boundary. A mistaken reviewer tests whether the final
argument can preserve an objection while rejecting its force for a stated reason.

## Compare modes under an explicit budget

Run `single` and `hybrid` on the same requests and evidence with exact model
identifiers, requested reasoning effort, generation depth, repair allowance and
output limits. Repeat a subset to estimate instability. Compare both the chosen
operational configurations and a separately declared equal-resource condition.
Six calls and one call consume different resources, and deeper decomposition
can change both cost and failure opportunities.

Blind reviewers to mode and randomise output order. For claim-only inputs, assess
whether the result constructs a substantive argument, states its proposed scope,
explains its mechanism and identifies the evidence needed for its premises and
warrants. Merely requesting evidence or proposing an experiment leaves this
construction unfinished.

Use anchored criteria defined before the comparison for scope fidelity, evidence
fidelity, warrant adequacy, coverage of material challenges and usefulness of the
evidence plan. Record unsupported assertions, invented references, omitted
defects and appropriate qualification or withholding. Distinguish improvements
in prose from improvements in reasoning.

Every attempted run contributes an outcome. Retain schema errors, refusals,
truncation, timeouts and rate-limit failures alongside successful outputs.
Record actual input/output usage, requested and returned reasoning effort,
elapsed time and known cost. Missing usage or price information stays unknown.
Compare paired cases, report reviewer agreement and uncertainty, and retain
representative failures. The conclusion applies to the tested tasks, models and
budgets.

Before adding debate rounds, retrieval or another verifier, identify the observed
failure the addition should repair. Define its acceptance criterion and account
for its full cost. A proof checker establishes its formal target under its
premises. Formalisation fidelity and empirical premise truth still need support.

## Evaluate the presentation separately

Use the same frozen graph when comparing Markdown, outline, prose or expandable
views. Ask readers to identify the qualified conclusion, decisive observation,
governing warrant and unresolved complication. Ask them to distinguish jointly
required support, an adequate alternative route and cumulative contributions.

Record mistaken dependency interpretations, omitted conditions and time spent
finding the decisive gap. Compare every view with the graph for changes in
quantifier, scope or evidential force. Structural reference validity and reader
comprehension are separate outcomes.

Display depth and authored argument depth answer different questions. A shorter
view should keep the judgement and its consequential conditions visible while
identifying omitted supporting detail. It should leave canonical JSON and export
unchanged. A structurally valid correction is useful only when its proposed
investigation can address the substantive objection.

## Include construction and collection when measuring the full workflow

Authoring-only trials cannot determine the cost of reaching an evidence-based
decision. Missing tool implementation, measurement qualification, source failures
and independent reviews can dominate the remaining work. Use the
[complete-workflow study](support-study.md) to freeze those phases, their operator
assets and their outcome assertions.

The study retains every attempted phase, call accounting, failures and independent
semantic reviews. A successful command supplies protocol evidence. Its criteria
still need an assessment of whether they answer the engineering question. No live
domain study has been retained for the current implementation, so broad claims of
better reasoning, lower total cost or improved delivery remain unmeasured.

## Read retained measurements within their original scope

Historical records preserve useful observations without becoming performance
claims about later revisions. The [benchmark directory](../benchmarks) retains
source identities, workload definitions and raw measurements. Use those identities
when reproducing or interpreting a result.

| Record | What it measures | What remains outside its scope |
|---|---|---|
| [Token context](../benchmarks/token_usage.json) | Reference text-token counts for frozen completions and exact-context replay. | Live argument quality, hidden reasoning, cache behaviour and billed cost. |
| [Planner](../benchmarks/planner_profile.json) | Complete deterministic compilation of identified synthetic planning shapes. | External discovery, model work and general production latency. |
| [Installer](../benchmarks/installer_profile.json) | Controlled runtime installation at the recorded revisions and cache conditions. | Unmeasured machines, networks and future source revisions. |
| [Contract verification](../benchmarks/contract_profile.json) | Paired synthetic contract admission and complete fixture verification, including rejected inputs and durable receipt writes. | Model calls, containers and general production latency; admission-only gains were not uniform. |
| [JSON admission](../benchmarks/render_admission_profile.json) | Fresh debug CLI processes at the declared JSON size boundaries, with matching errors and byte limits. | Release performance, cold compilation, whole-suite or hosted-CI savings. |
| [Runtime cache](../benchmarks/runtime_cache_profile.json) | Workflow coverage, cache identity independence and local warm identity/admission overhead for all 14 companions. | Hosted cache RPC time and future CI elapsed or invoice savings. |
| [Route selection](../benchmarks/selection_profile.json) | The recorded finite selection-kernel workloads. | End-to-end investigation cost or general maintainability. |
| [Finite EAL comparison](../benchmarks/eal4-20261027-final/measurement.md) | Source requests, retained observations, package/store bytes and elapsed execution for one synthetic snapshot claim. | Provider authoring, an external catalogue's completeness and general performance superiority. |

The contract and JSON records retain alternating baseline/candidate process runs,
preselected thresholds, binary and source identities, ranges and negative results.
Their runners are `scripts/benchmark-contracts.py` and
`scripts/benchmark-render-admission.py`. The contract comparison keeps both arms'
dependency profiles equal; the JSON comparison isolates the development profile
of the checked admission dependency. Assertions and overflow checks remain enabled.
The runtime action adds measured warm preflight overhead in exchange for shared
revision-specific payload identity. Its hosted CI benefit remains to be measured.

The finite EAL comparison asks whether `fixture-v1` contains exactly three
pairwise distinct case-sensitive strings. Its two local command sources use
identical implementations and fixed observation times in both variants. The
recorded acceptance conditions are two source requests, four retained
observations, support of that bounded claim and matching normalised values.
Two observations are acquired and two are derived. Execution uses no model calls.

To reproduce that historical comparison, build the baseline EAL host and its
supervisor at `cf9d43d42e6983c69f7c09fc72cf2881fe7deef9`, and a selected comparison
host in another target directory. The harness reads historical source from
EngArg `d181276f39f2189cbadf2654c1d0af2233e5e71e` and checks that the current finite
example differs only in its language header. A changed question needs a new
reviewed comparison. From EngArg:

```sh
python3 scripts/measure-eal4-completion.py \
  --baseline-host /path/to/baseline/eal-host \
  --current-host /path/to/current/eal-host \
  --eal-repo ../eal-mcp \
  --output benchmarks/eal4-comparison-new \
  --repetitions 5
```

Each repetition uses a new package and store. Order alternates. The output retains
binary hashes, exact commands, requests, observation/assessment payloads, file and
byte counts, elapsed samples and the separate static source inventory. SQLite
stores and their local integrity keys are excluded from Git after their sizes
are measured. Source-file change counts describe change extent rather than
runtime work or maintainability. A few repetitions of one local scenario supply
no population tail estimate or general superiority claim.

The [26 October method proposal](argument-driven-engineering/README.md) provides
historical research context and a prospective study template. Its account of the
repositories belongs to its cited revisions. The maintained [engineer workflow](USAGE.md)
is the source for current commands and execution contracts.
