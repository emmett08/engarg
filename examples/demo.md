Synthetic demonstration. No model calls, source verification or experiments were performed.

# Engineering Extended Toulmin Argument

## Argument context

| Field | Entry |
| --- | --- |
| Argument identity | Engineering case; current conclusion current_claim. Argument records: ARG-W1, ARG-W2, ARG-W3. Separate case revision: not supplied. |
| Engineering question | Parallelising the integration-test groups will reduce CI elapsed time by at least 40% without changing test outcomes. |
| Intended use | Evidence-bounded argument assessment and investigation planning. Supplied context: Synthetic demonstration only. A Linux CI runner executes four integration-test groups. The desired decision is whether to trial a parallel runner configuration. No production measurements are supplied. Reliance limits: Use this example to plan a trial. It supplies no production performance or correctness assurance. Agent agreement and successful schema validation do not establish the truth of the claim. |
| Claim type | Not separately classified. Declared inference types: deductive, inductive, practical. |
| Target identity | Integration-test execution stage, including fixture setup and teardown. Configuration: Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen. |
| Assessment boundary | Evidence considered: E1 Applicable period: The planned trial and identified repository revisions only. Operating conditions: Representative revisions, runner contention and cache states must be sampled and recorded. Exclusions and reliance limits: Use this example to plan a trial. It supplies no production performance or correctness assurance. Agent agreement and successful schema validation do not establish the truth of the claim. |

## Argument record — ARG-W1

| Component | Engineering entry |
| --- | --- |
| 1. Claim — S1 | The supplied synthetic description identifies sequential test execution and possible fixture sharing. |
| 2. Grounds — jointly required premises | G1: The synthetic workflow description reports sequential groups and a possible shared fixture database. Status: reported. E1: Synthetic workflow description. Source/location: synthetic:example-fixture. Kind: synthetic demonstration. User asserts checking: false. These premises are jointly required within ARG-W1. |
| 3. Warrant — W1 | An explicit supplied description supports a bounded report of what that description says. Given premises G1 and assumptions none recorded, this warrant connects them to S1. Inference type: deductive. Applicability: Applies only to the supplied synthetic text and does not establish actual production behaviour. |
| 4. Backing | No backing evidence supplied for W1. Its rule and applicability remain recorded reasoning requiring appraisal. |
| 5. Qualifier | A report of the supplied description only, with no production measurements. A separately calibrated numerical uncertainty estimate is not recorded. |
| 6. Rebuttal conditions | No specific rebuttal condition is recorded for W1. Applicability boundary: Applies only to the supplied synthetic text and does not establish actual production behaviour. Reassess the inference if evidence or analysis establishes that this boundary fails. |
| 7. Scope, definitions and criteria | System boundary: Integration-test execution stage, including fixture setup and teardown. Configuration: Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen. Operating conditions: Representative revisions, runner contention and cache states must be sampled and recorded. Measures and units: Elapsed time in seconds and paired relative reduction as a percentage. Applicable period: The planned trial and identified repository revisions only. Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions. |
| 8. Evidence provenance and adequacy | E1: Synthetic workflow description. Source/location: synthetic:example-fixture. Kind: synthetic demonstration. User asserts checking: false. Full findings and known shared uses are in the evidence register. Separate source dates, acquisition details and adequacy judgements are not inferred from identifiers. |
| 9. Critical assumptions and dependencies | No assumptions recorded for this inference. Required ground or subclaim identifiers: G1. |
| 10. Competing explanations or options | Alternative: Fixture setup or CPU contention may dominate elapsed time, leaving little benefit from parallel group scheduling. Discriminating check: Measure group, fixture and runner-resource timings in both configurations on the same revisions. Evidence: No supporting evidence reference supplied. An independently justified common decision criterion is not separately recorded. |
| 11. Challenges and responses | No targeted challenge recorded for this inference and its imported support. Completeness remains unassessed. |
| 12. Stronger target and evidence obligations | stronger_target: For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Qualifier: Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. Relevant obligations: No additional obligation is separately recorded for this inference. |
| 13. Current assessment and defensible claim | Record conclusion S1: The supplied synthetic description identifies sequential test execution and possible fixture sharing. Qualifier: A report of the supplied description only, with no production measurements. Separate subclaim verdict not recorded. Its stated qualifier and dependencies govern its use. This argument record is selected for its conclusion. Relationship: jointly_required. Selected warrants: W1. Model assessment rationale: The bounded description claim relies on the source-report warrant W1. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. Overall current formulation: current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Assessment: unsupported. Decisive reasoning and limitations: The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. |
| 14. Decision basis and revision triggers | Consequences: Faster feedback is valuable only when test coverage and outcome reliability remain acceptable. Reliance limits: Use this example to plan a trial. It supplies no production performance or correctness assurance. Agent agreement and successful schema validation do not establish the truth of the claim. Residual risks: Finite trials may miss rare interference or timing conditions. The synthetic record supplies no empirical performance evidence. A separate action authorisation, value criterion and action acceptance decision are not recorded. Report revision policy: reassess affected inferences when evidence, assumptions, target configuration or operating conditions change. |

## Argument record — ARG-W2

| Component | Engineering entry |
| --- | --- |
| 1. Claim — current_claim | The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. |
| 2. Grounds — jointly required premises | S1: The supplied synthetic description identifies sequential test execution and possible fixture sharing. Qualifier: A report of the supplied description only, with no production measurements. These premises are jointly required within ARG-W2. |
| 3. Warrant — W2 | A sequential workflow identifies a candidate for a controlled scheduling trial when interference and representativeness are investigated. Given premises S1 and assumptions A1, this warrant connects them to current_claim. Inference type: practical. Applicability: Supports planning the trial only. No quantitative benefit follows from sequential scheduling alone. |
| 4. Backing | No backing evidence supplied for W2. Its rule and applicability remain recorded reasoning requiring appraisal. |
| 5. Qualifier | Unsupported performance prediction. The only supplied record is a synthetic description. A separately calibrated numerical uncertainty estimate is not recorded. |
| 6. Rebuttal conditions | No specific rebuttal condition is recorded for W2. Applicability boundary: Supports planning the trial only. No quantitative benefit follows from sequential scheduling alone. Reassess the inference if evidence or analysis establishes that this boundary fails. |
| 7. Scope, definitions and criteria | System boundary: Integration-test execution stage, including fixture setup and teardown. Configuration: Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen. Operating conditions: Representative revisions, runner contention and cache states must be sampled and recorded. Measures and units: Elapsed time in seconds and paired relative reduction as a percentage. Applicable period: The planned trial and identified repository revisions only. Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions. |
| 8. Evidence provenance and adequacy | E1: Synthetic workflow description. Source/location: synthetic:example-fixture. Kind: synthetic demonstration. User asserts checking: false. Full findings and known shared uses are in the evidence register. Separate source dates, acquisition details and adequacy judgements are not inferred from identifiers. |
| 9. Critical assumptions and dependencies | A1: The test groups can execute concurrently without sharing mutable state that changes their outcomes. Critical: true. Status: untested. Applies to: W2, W3. Basis: No supporting evidence reference supplied. Validation/check: Inspect fixture ownership and run isolation and order-perturbation checks before relying on timing results.. If a critical assumption fails, reassess each declared dependent inference. Required ground or subclaim identifiers: S1. |
| 10. Competing explanations or options | Alternative: Fixture setup or CPU contention may dominate elapsed time, leaving little benefit from parallel group scheduling. Discriminating check: Measure group, fixture and runner-resource timings in both configurations on the same revisions. Evidence: No supporting evidence reference supplied. An independently justified common decision criterion is not separately recorded. |
| 11. Challenges and responses | No targeted challenge recorded for this inference and its imported support. Completeness remains unassessed. |
| 12. Stronger target and evidence obligations | stronger_target: For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Qualifier: Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. Relevant obligations: N2 targets A1 and would strengthen current_claim. Question: Do concurrent groups interfere through shared mutable fixtures? Method: Inspect fixture ownership, allocate isolated resources where necessary, perturb scheduling and repeat the same tests under sequential and parallel execution. Measurement: Recorded fixture interference and outcome discrepancies as counts. Comparator: Sequential execution and an isolated-fixture parallel configuration. Proposed acceptance criterion: No demonstrated cross-group state interference in the declared checks, with remaining coverage limits reported. Passing finite tests supports only the tested conditions. Favourable: reassess the enabled claim against that criterion and all its other dependencies. Adverse: Repair isolation or retain sequential execution for the affected groups. Inconclusive: leave the stronger target unresolved. Priority: 1. Estimated effort: Fixture engineering and repeated test execution. |
| 13. Current assessment and defensible claim | Record conclusion current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Current assessment: unsupported. The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. This argument record is selected for its conclusion. Relationship: jointly_required. Selected warrants: W2. Model assessment rationale: The present conclusion relies on W2, which connects the source description to a proposed controlled trial. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. Overall current formulation: current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Assessment: unsupported. Decisive reasoning and limitations: The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. |
| 14. Decision basis and revision triggers | Consequences: Faster feedback is valuable only when test coverage and outcome reliability remain acceptable. Reliance limits: Use this example to plan a trial. It supplies no production performance or correctness assurance. Agent agreement and successful schema validation do not establish the truth of the claim. Residual risks: Finite trials may miss rare interference or timing conditions. The synthetic record supplies no empirical performance evidence. A separate action authorisation, value criterion and action acceptance decision are not recorded. Report revision policy: reassess affected inferences when evidence, assumptions, target configuration or operating conditions change. |

## Argument record — ARG-W3

| Component | Engineering entry |
| --- | --- |
| 1. Claim — stronger_target | For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. |
| 2. Grounds — jointly required premises | S1: The supplied synthetic description identifies sequential test execution and possible fixture sharing. Qualifier: A report of the supplied description only, with no production measurements. G2: The parallel configuration achieves the specified timing and outcome criteria under representative conditions. Status: missing. No supporting evidence reference supplied. These premises are jointly required within ARG-W3. |
| 3. Warrant — W3 | Paired measurements on frozen, representative revisions can support a bounded performance comparison when outcome preservation and interference are checked separately. Given premises S1, G2 and assumptions A1, this warrant connects them to stronger_target. Inference type: inductive. Applicability: Requires representative sampling and explicit treatment of cache state, fixture overhead and runner contention. The warrant remains a proposed methodological justification. |
| 4. Backing | No backing evidence supplied for W3. Its rule and applicability remain recorded reasoning requiring appraisal. |
| 5. Qualifier | Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. A separately calibrated numerical uncertainty estimate is not recorded. |
| 6. Rebuttal conditions | D1 targets W3 by undercutting: A timing improvement could arise from different cache or runner conditions rather than the scheduling change. Basis: hypothetical. Material: true. Disposition: open. Response and remaining effect: Counterbalance run order and record cache state and contention in paired runs. Evidence: No supporting evidence reference supplied. D2 targets G2 by undermining: Shared fixture mutations could alter test outcomes under concurrent execution. Basis: hypothetical. Material: true. Disposition: open. Response and remaining effect: Investigate through the isolation checks for A1. This possibility is not an observed failure. Evidence: No supporting evidence reference supplied. Applicability boundary: Requires representative sampling and explicit treatment of cache state, fixture overhead and runner contention. The warrant remains a proposed methodological justification. An established exception requires reassessment or qualification of this inference. |
| 7. Scope, definitions and criteria | System boundary: Integration-test execution stage, including fixture setup and teardown. Configuration: Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen. Operating conditions: Representative revisions, runner contention and cache states must be sampled and recorded. Measures and units: Elapsed time in seconds and paired relative reduction as a percentage. Applicable period: The planned trial and identified repository revisions only. Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions. |
| 8. Evidence provenance and adequacy | E1: Synthetic workflow description. Source/location: synthetic:example-fixture. Kind: synthetic demonstration. User asserts checking: false. Full findings and known shared uses are in the evidence register. Separate source dates, acquisition details and adequacy judgements are not inferred from identifiers. |
| 9. Critical assumptions and dependencies | A1: The test groups can execute concurrently without sharing mutable state that changes their outcomes. Critical: true. Status: untested. Applies to: W2, W3. Basis: No supporting evidence reference supplied. Validation/check: Inspect fixture ownership and run isolation and order-perturbation checks before relying on timing results.. If a critical assumption fails, reassess each declared dependent inference. Required ground or subclaim identifiers: S1, G2. |
| 10. Competing explanations or options | Alternative: Fixture setup or CPU contention may dominate elapsed time, leaving little benefit from parallel group scheduling. Discriminating check: Measure group, fixture and runner-resource timings in both configurations on the same revisions. Evidence: No supporting evidence reference supplied. An independently justified common decision criterion is not separately recorded. |
| 11. Challenges and responses | D1 targets W3 by undercutting: A timing improvement could arise from different cache or runner conditions rather than the scheduling change. Basis: hypothetical. Material: true. Disposition: open. Response and remaining effect: Counterbalance run order and record cache state and contention in paired runs. Evidence: No supporting evidence reference supplied. D2 targets G2 by undermining: Shared fixture mutations could alter test outcomes under concurrent execution. Basis: hypothetical. Material: true. Disposition: open. Response and remaining effect: Investigate through the isolation checks for A1. This possibility is not an observed failure. Evidence: No supporting evidence reference supplied. |
| 12. Stronger target and evidence obligations | stronger_target: For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Qualifier: Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. Relevant obligations: N1 targets G2 and would strengthen stronger_target. Question: Does the identified parallel configuration meet the timing and outcome criteria? Method: Freeze revisions and runner configuration. Pilot the variance, then preregister a paired, counterbalanced trial with sample size chosen from that pilot. Retain complete timing and outcome records. Measurement: Elapsed time in seconds, relative reduction in percent, and recorded outcome/isolation failures as counts. Comparator: The existing sequential workflow on the same revisions and runner configuration. Proposed acceptance criterion: The preregistered interval for median relative reduction must support at least 40%. Outcome preservation must pass separately declared coverage, isolation and repeated-run criteria. Favourable: reassess the enabled claim against that criterion and all its other dependencies. Adverse: Reject the 40% target or narrow it to the revisions and conditions actually supported by the observations. Inconclusive: leave the stronger target unresolved. Priority: 1. Estimated effort: CI runner time and analysis effort, to be estimated from the pilot. N2 targets A1 and would strengthen current_claim. Question: Do concurrent groups interfere through shared mutable fixtures? Method: Inspect fixture ownership, allocate isolated resources where necessary, perturb scheduling and repeat the same tests under sequential and parallel execution. Measurement: Recorded fixture interference and outcome discrepancies as counts. Comparator: Sequential execution and an isolated-fixture parallel configuration. Proposed acceptance criterion: No demonstrated cross-group state interference in the declared checks, with remaining coverage limits reported. Passing finite tests supports only the tested conditions. Favourable: reassess the enabled claim against that criterion and all its other dependencies. Adverse: Repair isolation or retain sequential execution for the affected groups. Inconclusive: leave the stronger target unresolved. Priority: 1. Estimated effort: Fixture engineering and repeated test execution. N3 targets D1 and would strengthen stronger_target. Question: Could cache state or runner contention explain the timing difference? Method: Counterbalance configuration order, stratify cache state and log runner-resource contention in the paired trial. Measurement: Cache-state categories, runner contention measures and elapsed time in seconds. Comparator: Both configurations within each recorded cache and contention condition. Proposed acceptance criterion: The claimed reduction must survive the preregistered condition checks or be explicitly restricted to supported conditions. Favourable: reassess the enabled claim against that criterion and all its other dependencies. Adverse: Restrict the claim or repeat the study with better control of the identified confounder. Inconclusive: leave the stronger target unresolved. Priority: 2. Estimated effort: Additional repetitions and instrumentation. N4 targets D2 and would strengthen stronger_target. Question: Does shared fixture mutation change outcomes under concurrent execution? Method: Use the fixture inspection and scheduling perturbation checks specified for A1, retaining individual outcome records. Measurement: Cross-group state interference and outcome discrepancies as counts, with revision and scheduling identifiers. Comparator: Sequential execution and isolated-fixture parallel execution on the same revisions. Proposed acceptance criterion: No demonstrated cross-group interference in the preregistered checks, with finite coverage limits retained. Favourable: reassess the enabled claim against that criterion and all its other dependencies. Adverse: Retain sequential scheduling for affected groups or repair fixture isolation before making the stronger claim. Inconclusive: leave the stronger target unresolved. Priority: 1. Estimated effort: Shared with the A1 isolation investigation. |
| 13. Current assessment and defensible claim | Record conclusion stronger_target: For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Qualifier: Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. Conditional target. Its fulfilment has not been established by this report. This argument record is selected for its conclusion. Relationship: jointly_required. Selected warrants: W3. Model assessment rationale: The stronger target requires both the descriptive subclaim and the missing comparative measurements within W3. Its premises are jointly required. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. Overall current formulation: current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Assessment: unsupported. Decisive reasoning and limitations: The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. |
| 14. Decision basis and revision triggers | Consequences: Faster feedback is valuable only when test coverage and outcome reliability remain acceptable. Reliance limits: Use this example to plan a trial. It supplies no production performance or correctness assurance. Agent agreement and successful schema validation do not establish the truth of the claim. Residual risks: Finite trials may miss rare interference or timing conditions. The synthetic record supplies no empirical performance evidence. A separate action authorisation, value criterion and action acceptance decision are not recorded. Report revision policy: reassess affected inferences when evidence, assumptions, target configuration or operating conditions change. |

## Evidence register

| Evidence ID | Premise or warrant supported | Artefact, location and version/date | Method and relevant finding | Availability and verification performed | Adequacy, limitations and shared dependencies |
| --- | --- | --- | --- | --- | --- |
| E1 | G1: premise support | Synthetic workflow description Source/location: synthetic:example-fixture. Version/date: not separately supplied unless included in that provenance. | Kind: synthetic demonstration. Supplied finding/excerpt: The example workflow runs four integration-test groups in sequence. The groups may share a fixture database. No elapsed-time measurements or isolation checks have been recorded. | Available as supplied text. User asserts checking: false. Independent verification by this workflow: not performed. | Relevance, reliability, coverage and uncertainty require appraisal for the intended use. Known uses: 1. Reuse of one evidence identity supplies no independent replication. |

## Assumptions and dependencies

| ID | Assumption or required subclaim | Basis for reliance | Dependent inference or conclusion | Consequence of failure and relevant check |
| --- | --- | --- | --- | --- |
| A1 | The test groups can execute concurrently without sharing mutable state that changes their outcomes. Critical: true. Status: untested. | No supporting evidence reference supplied. | W2, W3 | Reassess the declared dependencies if a critical assumption fails. Check: Inspect fixture ownership and run isolation and order-perturbation checks before relying on timing results. |
| S1 | The supplied synthetic description identifies sequential test execution and possible fixture sharing. Qualifier: A report of the supplied description only, with no production measurements. | Declared supporting arguments: ARG-W1. Separate verdict not recorded. | ARG-W2 → current_claim; ARG-W3 → stronger_target | Reassess each inference importing this proposition if its support or qualifier changes. |

## Challenge register

| Attack type | Target and interpretation |
| --- | --- |
| Undermining | Challenges a premise's truth or support. |
| Undercutting | Challenges the applicability or reliability of an inference while its premises may hold. |
| Rebutting | Supports an incompatible conclusion or subclaim. |

| Challenge ID | Target ID and attack type | Objection and supporting evidence | Evidential status | Response or discriminating check | Remaining effect |
| --- | --- | --- | --- | --- | --- |
| D1 | W3: undercutting | A timing improvement could arise from different cache or runner conditions rather than the scheduling change. No supporting evidence reference supplied. | hypothetical | Counterbalance run order and record cache state and contention in paired runs. | Disposition: open. Material to target: true. |
| D2 | G2: undermining | Shared fixture mutations could alter test outcomes under concurrent execution. No supporting evidence reference supplied. | hypothetical | Investigate through the isolation checks for A1. This possibility is not an observed failure. | Disposition: open. Material to target: true. |

## Evidence obligations for the stronger target

**Current formulation — current_claim:** The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes.

**Stronger target formulation — stronger_target:** For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria.

Plans retain the claim each investigation would enable, including obligations needed for the current formulation.

| Gap and affected inference | Required evidence or reasoning | Method, scope and comparison | Assessment criterion and its justification | Effect of favourable, adverse or inconclusive results | Priority and next action |
| --- | --- | --- | --- | --- | --- |
| N1 targets G2; enables stronger_target. | Does the identified parallel configuration meet the timing and outcome criteria? | Method: Freeze revisions and runner configuration. Pilot the variance, then preregister a paired, counterbalanced trial with sample size chosen from that pilot. Retain complete timing and outcome records. Measurement: Elapsed time in seconds, relative reduction in percent, and recorded outcome/isolation failures as counts. Comparator: The existing sequential workflow on the same revisions and runner configuration. | Proposed criterion: The preregistered interval for median relative reduction must support at least 40%. Outcome preservation must pass separately declared coverage, isolation and repeated-run criteria. Its independent justification must be established before decisive interpretation. | Favourable: reassess stronger_target with all other dependencies. Adverse: Reject the 40% target or narrow it to the revisions and conditions actually supported by the observations. Inconclusive: leave the stronger target unresolved. | Priority 1. Next action: Freeze revisions and runner configuration. Pilot the variance, then preregister a paired, counterbalanced trial with sample size chosen from that pilot. Retain complete timing and outcome records. Estimated effort: CI runner time and analysis effort, to be estimated from the pilot. |
| N2 targets A1; enables current_claim. | Do concurrent groups interfere through shared mutable fixtures? | Method: Inspect fixture ownership, allocate isolated resources where necessary, perturb scheduling and repeat the same tests under sequential and parallel execution. Measurement: Recorded fixture interference and outcome discrepancies as counts. Comparator: Sequential execution and an isolated-fixture parallel configuration. | Proposed criterion: No demonstrated cross-group state interference in the declared checks, with remaining coverage limits reported. Passing finite tests supports only the tested conditions. Its independent justification must be established before decisive interpretation. | Favourable: reassess current_claim with all other dependencies. Adverse: Repair isolation or retain sequential execution for the affected groups. Inconclusive: leave the stronger target unresolved. | Priority 1. Next action: Inspect fixture ownership, allocate isolated resources where necessary, perturb scheduling and repeat the same tests under sequential and parallel execution. Estimated effort: Fixture engineering and repeated test execution. |
| N4 targets D2; enables stronger_target. | Does shared fixture mutation change outcomes under concurrent execution? | Method: Use the fixture inspection and scheduling perturbation checks specified for A1, retaining individual outcome records. Measurement: Cross-group state interference and outcome discrepancies as counts, with revision and scheduling identifiers. Comparator: Sequential execution and isolated-fixture parallel execution on the same revisions. | Proposed criterion: No demonstrated cross-group interference in the preregistered checks, with finite coverage limits retained. Its independent justification must be established before decisive interpretation. | Favourable: reassess stronger_target with all other dependencies. Adverse: Retain sequential scheduling for affected groups or repair fixture isolation before making the stronger claim. Inconclusive: leave the stronger target unresolved. | Priority 1. Next action: Use the fixture inspection and scheduling perturbation checks specified for A1, retaining individual outcome records. Estimated effort: Shared with the A1 isolation investigation. |
| N3 targets D1; enables stronger_target. | Could cache state or runner contention explain the timing difference? | Method: Counterbalance configuration order, stratify cache state and log runner-resource contention in the paired trial. Measurement: Cache-state categories, runner contention measures and elapsed time in seconds. Comparator: Both configurations within each recorded cache and contention condition. | Proposed criterion: The claimed reduction must survive the preregistered condition checks or be explicitly restricted to supported conditions. Its independent justification must be established before decisive interpretation. | Favourable: reassess stronger_target with all other dependencies. Adverse: Restrict the claim or repeat the study with better control of the identified confounder. Inconclusive: leave the stronger target unresolved. | Priority 2. Next action: Counterbalance configuration order, stratify cache state and log runner-resource contention in the paired trial. Estimated effort: Additional repetitions and instrumentation. |

## Assessed argument

> current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Assessment: unsupported. Decisive reasoning and limitations: The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. Declared grounds: G1, G2 Declared warrants: W1, W2, W3 Consequential recorded challenges: D1, D2 The stronger target is For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Additional support is specified by evidence obligations N1, N2, N3, N4.


### The description justifies planning a trial — current_claim

The supplied description identifies a candidate for a controlled parallelisation trial. It provides no empirical basis for predicting a 40% reduction or for claiming that test outcomes will be preserved. (References: current_claim, E1.)

The synthetic workflow description reports four integration-test groups running in sequence and possible sharing of a fixture database. These details establish what the example says about its workflow, while leaving actual production timing and interference unmeasured. (References: G1, S1, E1.)

Sequential scheduling gives a reason to investigate concurrent execution because some work may overlap. The practical warrant therefore supports a trial decision, while the stronger performance claim requires the comparative measurements represented by G2. (References: W2, S1, G2, stronger_target.)

The proposed trial must test whether mutable fixtures change outcomes when groups overlap. Cache state and runner contention also need control because they could explain a timing difference independently of the scheduling change. The isolation and counterbalanced measurement plans address these unresolved conditions. (References: A1, D1, D2, N1, N2, N3, N4.)

The immediate next step is to freeze the runner configuration and investigate isolation before running the paired timing trial. Retain the 40% improvement as a conditional target, then narrow or reject it if the recorded observations fail its declared criteria. (References: current_claim, stronger_target, N1, N2.)


### What the supplied description establishes — S1

The supplied synthetic description reports sequential integration-test groups and possible fixture sharing. This subclaim concerns the contents of that record and carries no production performance result. (References: S1, E1.)

That bounded finding follows through W1 from the explicit report G1. It can inform the design of an investigation, with the excerpt retained so a reviewer can inspect the source of the interpretation. (References: S1, W1, G1, E1.)


### The conditional performance target — stronger_target

The stronger target asks whether a specified parallel configuration achieves at least a 40% median reduction while meeting separately declared outcome and isolation criteria. It remains conditional because the comparative measurements are missing. (References: stronger_target, G2.)

A paired, counterbalanced trial can supply evidence for that bounded comparison after fixture interference has been investigated. Passing the stated criteria would justify reassessing the target within the tested configuration, with residual uncertainty retained. (References: stronger_target, W3, N1, N2, N3, N4.)


## Composition overview for a complex case

**Case question:** What formulation of the requested claim is justified? Requested claim: Parallelising the integration-test groups will reduce CI elapsed time by at least 40% without changing test outcomes.

**Top-level claim — current_claim:** The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description.

**Shared context:** System boundary: Integration-test execution stage, including fixture setup and teardown. Configuration: Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen. Operating conditions: Representative revisions, runner contention and cache states must be sampled and recorded. Measures and units: Elapsed time in seconds and paired relative reduction as a percentage. Applicable period: The planned trial and identified repository revisions only. Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions.

**Composition warrant:** Each declared warrant identifies jointly required premises and its conclusion. Explicit composition records identify joint, cumulative or alternative support and the selected routes. Route selection is a model judgement whose evidential adequacy requires appraisal.

**Required argument records:** ARG-W1, ARG-W2, ARG-W3

### Claim register

| Claim ID | Exact proposition and applicable scope | Supporting argument IDs | Relevant challenges | Current assessment |
| --- | --- | --- | --- | --- |
| current_claim | The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Shared engineering scope applies. | ARG-W2 | No direct challenge recorded; dependency attacks remain in the challenge register. | Current assessment: unsupported. The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. |
| S1 | The supplied synthetic description identifies sequential test execution and possible fixture sharing. Qualifier: A report of the supplied description only, with no production measurements. Shared engineering scope applies. | ARG-W1 | No direct challenge recorded; dependency attacks remain in the challenge register. | Separate subclaim verdict not recorded. Its stated qualifier and dependencies govern its use. |
| stronger_target | For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Qualifier: Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. Shared engineering scope applies. | ARG-W3 | No direct challenge recorded; dependency attacks remain in the challenge register. | Conditional target. Its fulfilment has not been established by this report. |

### Composition register

| Argument ID | Conclusion ID | Ground claims or premises | Warrant ID | Required relationship among inputs | Scope and required assumptions |
| --- | --- | --- | --- | --- | --- |
| ARG-W1 | S1 | G1 | W1 | Premises jointly required within this route. Relationship: jointly_required. Selected warrants: W1. Model assessment rationale: The bounded description claim relies on the source-report warrant W1. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. | Applicability: Applies only to the supplied synthetic text and does not establish actual production behaviour. Assumptions: None recorded.. |
| ARG-W2 | current_claim | S1 | W2 | Premises jointly required within this route. Relationship: jointly_required. Selected warrants: W2. Model assessment rationale: The present conclusion relies on W2, which connects the source description to a proposed controlled trial. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. | Applicability: Supports planning the trial only. No quantitative benefit follows from sequential scheduling alone. Assumptions: A1. |
| ARG-W3 | stronger_target | S1, G2 | W3 | Premises jointly required within this route. Relationship: jointly_required. Selected warrants: W3. Model assessment rationale: The stronger target requires both the descriptive subclaim and the missing comparative measurements within W3. Its premises are jointly required. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. | Applicability: Requires representative sampling and explicit treatment of cache state, fixture overhead and runner contention. The warrant remains a proposed methodological justification. Assumptions: A1. |

### Relationship meanings

| Relationship | Representation and assessment |
| --- | --- |
| Serial support | S1 → ARG-W2 → current_claim; S1 → ARG-W3 → stronger_target |
| Joint support | Each warrant's premises are jointly required. Missing support leaves that declared route unresolved. |
| Alternative routes | No alternative-route composition recorded. |
| Cumulative support | Combination requires an explicit warrant and adequate treatment of shared dependencies. No quantitative score combination is inferred. |
| Shared support or assumptions | Canonical identifiers are reused across branches. Evidence and assumption registers show their declared uses. |
| Attack | Challenges retain their premise, inference or conclusion target and recorded disposition. |

### Composition checks

- **Coverage:** The declared graph identifies its premises and conclusions. Completeness of requirements, interfaces and interactions is not separately assessed.
- **Compatibility:** Shared scope: System boundary: Integration-test execution stage, including fixture setup and teardown. Configuration: Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen. Operating conditions: Representative revisions, runner contention and cache states must be sampled and recorded. Measures and units: Elapsed time in seconds and paired relative reduction as a percentage. Applicable period: The planned trial and identified repository revisions only. Separate term definitions and established property criteria: not supplied unless explicitly stated in the foregoing assessment. Additional transfer justification is required for other configurations or operating conditions. Individual transfer arguments are not separately recorded.
- **Support conditions:** S1: Relationship: jointly_required. Selected warrants: W1. Model assessment rationale: The bounded description claim relies on the source-report warrant W1. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. current_claim: Relationship: jointly_required. Selected warrants: W2. Model assessment rationale: The present conclusion relies on W2, which connects the source description to a proposed controlled trial. Selection records a judgement; its adequacy requires the referenced reasoning and evidence. stronger_target: Relationship: jointly_required. Selected warrants: W3. Model assessment rationale: The stronger target requires both the descriptive subclaim and the missing comparative measurements within W3. Its premises are jointly required. Selection records a judgement; its adequacy requires the referenced reasoning and evidence.
- **Shared dependencies:** Evidence and assumption registers retain canonical identities and uses. Shared identities supply no independent replication.
- **Conflict assessment:** D1 → W3 (open) D2 → G2 (open)
- **Circularity check:** Structural and reporting checks passed, including the acyclic support graph. This supplies no proof of semantic adequacy.
- **Change impact:** Use the composition register to follow an altered premise through each consuming warrant to current_claim or stronger_target. Reassess affected assumptions and challenges as well.
- **Whole-case assessment:** current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Assessment: unsupported. Decisive reasoning and limitations: The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations.

### Whole-case conclusion

> current_claim: The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes. Qualifier: Unsupported performance prediction. The only supplied record is a synthetic description. Assessment: unsupported. Decisive reasoning and limitations: The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations. The stronger target is For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria. Its recorded qualification is Conditional target requiring paired measurements, declared uncertainty and independent isolation checks. The next investigation is N1: Does the identified parallel configuration meet the timing and outcome criteria? The possible results affect G2 and the support for stronger_target.


## Interpretation notes

- Source identification and structural integrity support traceability. Evidence adequacy and inferential soundness require separate appraisal.
- Proposed investigation criteria remain proposed until adopted and tested. No investigation is represented as completed merely because it appears in this report.
- The top-level status assesses current_claim. Subclaims and stronger_target retain their own wording and qualifications; a separate verdict is not inferred.
- Residual risks: Finite trials may miss rare interference or timing conditions. The synthetic record supplies no empirical performance evidence.
- Decision limits: Use this example to plan a trial. It supplies no production performance or correctness assurance. Agent agreement and successful schema validation do not establish the truth of the claim.


## Complete audit record

```json
{
  "argument": {
    "adjudication": "The 40% claim exceeds the supplied support. Retain it as a testable target, prioritise fixture isolation and paired measurements, and reassess against the retained observations.",
    "alternatives": [
      {
        "distinguishing_test": "Measure group, fixture and runner-resource timings in both configurations on the same revisions.",
        "evidence_ids": [],
        "explanation": "Fixture setup or CPU contention may dominate elapsed time, leaving little benefit from parallel group scheduling."
      }
    ],
    "assumptions": [
      {
        "applies_to": [
          "W2",
          "W3"
        ],
        "critical": true,
        "evidence_ids": [],
        "id": "A1",
        "statement": "The test groups can execute concurrently without sharing mutable state that changes their outcomes.",
        "status": "untested",
        "validation": "Inspect fixture ownership and run isolation and order-perturbation checks before relying on timing results."
      }
    ],
    "compositions": [
      {
        "conclusion_id": "S1",
        "rationale": "The bounded description claim relies on the source-report warrant W1.",
        "relationship": "jointly_required",
        "selected_warrant_ids": [
          "W1"
        ]
      },
      {
        "conclusion_id": "current_claim",
        "rationale": "The present conclusion relies on W2, which connects the source description to a proposed controlled trial.",
        "relationship": "jointly_required",
        "selected_warrant_ids": [
          "W2"
        ]
      },
      {
        "conclusion_id": "stronger_target",
        "rationale": "The stronger target requires both the descriptive subclaim and the missing comparative measurements within W3. Its premises are jointly required.",
        "relationship": "jointly_required",
        "selected_warrant_ids": [
          "W3"
        ]
      }
    ],
    "current_claim": {
      "qualifier": "Unsupported performance prediction. The only supplied record is a synthetic description.",
      "text": "The supplied workflow description identifies a candidate for a controlled parallelisation trial; it does not establish a 40% reduction or preservation of test outcomes."
    },
    "decision_limits": [
      "Use this example to plan a trial. It supplies no production performance or correctness assurance.",
      "Agent agreement and successful schema validation do not establish the truth of the claim."
    ],
    "defeaters": [
      {
        "basis": "hypothetical",
        "challenge": "A timing improvement could arise from different cache or runner conditions rather than the scheduling change.",
        "evidence_ids": [],
        "id": "D1",
        "kind": "undercutting",
        "material": true,
        "response": "Counterbalance run order and record cache state and contention in paired runs.",
        "status": "open",
        "target_id": "W3"
      },
      {
        "basis": "hypothetical",
        "challenge": "Shared fixture mutations could alter test outcomes under concurrent execution.",
        "evidence_ids": [],
        "id": "D2",
        "kind": "undermining",
        "material": true,
        "response": "Investigate through the isolation checks for A1. This possibility is not an observed failure.",
        "status": "open",
        "target_id": "G2"
      }
    ],
    "evidence_plan": [
      {
        "acceptance_criterion": "The preregistered interval for median relative reduction must support at least 40%. Outcome preservation must pass separately declared coverage, isolation and repeated-run criteria.",
        "comparator": "The existing sequential workflow on the same revisions and runner configuration.",
        "cost": "CI runner time and analysis effort, to be estimated from the pilot.",
        "enables_claim": "stronger_target",
        "id": "N1",
        "if_failed": "Reject the 40% target or narrow it to the revisions and conditions actually supported by the observations.",
        "measurement": "Elapsed time in seconds, relative reduction in percent, and recorded outcome/isolation failures as counts.",
        "method": "Freeze revisions and runner configuration. Pilot the variance, then preregister a paired, counterbalanced trial with sample size chosen from that pilot. Retain complete timing and outcome records.",
        "priority": 1,
        "question": "Does the identified parallel configuration meet the timing and outcome criteria?",
        "target_id": "G2"
      },
      {
        "acceptance_criterion": "No demonstrated cross-group state interference in the declared checks, with remaining coverage limits reported. Passing finite tests supports only the tested conditions.",
        "comparator": "Sequential execution and an isolated-fixture parallel configuration.",
        "cost": "Fixture engineering and repeated test execution.",
        "enables_claim": "current_claim",
        "id": "N2",
        "if_failed": "Repair isolation or retain sequential execution for the affected groups.",
        "measurement": "Recorded fixture interference and outcome discrepancies as counts.",
        "method": "Inspect fixture ownership, allocate isolated resources where necessary, perturb scheduling and repeat the same tests under sequential and parallel execution.",
        "priority": 1,
        "question": "Do concurrent groups interfere through shared mutable fixtures?",
        "target_id": "A1"
      },
      {
        "acceptance_criterion": "The claimed reduction must survive the preregistered condition checks or be explicitly restricted to supported conditions.",
        "comparator": "Both configurations within each recorded cache and contention condition.",
        "cost": "Additional repetitions and instrumentation.",
        "enables_claim": "stronger_target",
        "id": "N3",
        "if_failed": "Restrict the claim or repeat the study with better control of the identified confounder.",
        "measurement": "Cache-state categories, runner contention measures and elapsed time in seconds.",
        "method": "Counterbalance configuration order, stratify cache state and log runner-resource contention in the paired trial.",
        "priority": 2,
        "question": "Could cache state or runner contention explain the timing difference?",
        "target_id": "D1"
      },
      {
        "acceptance_criterion": "No demonstrated cross-group interference in the preregistered checks, with finite coverage limits retained.",
        "comparator": "Sequential execution and isolated-fixture parallel execution on the same revisions.",
        "cost": "Shared with the A1 isolation investigation.",
        "enables_claim": "stronger_target",
        "id": "N4",
        "if_failed": "Retain sequential scheduling for affected groups or repair fixture isolation before making the stronger claim.",
        "measurement": "Cross-group state interference and outcome discrepancies as counts, with revision and scheduling identifiers.",
        "method": "Use the fixture inspection and scheduling perturbation checks specified for A1, retaining individual outcome records.",
        "priority": 1,
        "question": "Does shared fixture mutation change outcomes under concurrent execution?",
        "target_id": "D2"
      }
    ],
    "grounds": [
      {
        "evidence_ids": [
          "E1"
        ],
        "id": "G1",
        "statement": "The synthetic workflow description reports sequential groups and a possible shared fixture database.",
        "status": "reported"
      },
      {
        "evidence_ids": [],
        "id": "G2",
        "statement": "The parallel configuration achieves the specified timing and outcome criteria under representative conditions.",
        "status": "missing"
      }
    ],
    "original_claim": "Parallelising the integration-test groups will reduce CI elapsed time by at least 40% without changing test outcomes.",
    "reading": [
      {
        "claim_id": "current_claim",
        "paragraphs": [
          {
            "purpose": "answer",
            "reference_ids": [
              "current_claim",
              "E1"
            ],
            "text": "The supplied description identifies a candidate for a controlled parallelisation trial. It provides no empirical basis for predicting a 40% reduction or for claiming that test outcomes will be preserved."
          },
          {
            "purpose": "finding",
            "reference_ids": [
              "G1",
              "S1",
              "E1"
            ],
            "text": "The synthetic workflow description reports four integration-test groups running in sequence and possible sharing of a fixture database. These details establish what the example says about its workflow, while leaving actual production timing and interference unmeasured."
          },
          {
            "purpose": "connection",
            "reference_ids": [
              "W2",
              "S1",
              "G2",
              "stronger_target"
            ],
            "text": "Sequential scheduling gives a reason to investigate concurrent execution because some work may overlap. The practical warrant therefore supports a trial decision, while the stronger performance claim requires the comparative measurements represented by G2."
          },
          {
            "purpose": "complication",
            "reference_ids": [
              "A1",
              "D1",
              "D2",
              "N1",
              "N2",
              "N3",
              "N4"
            ],
            "text": "The proposed trial must test whether mutable fixtures change outcomes when groups overlap. Cache state and runner contention also need control because they could explain a timing difference independently of the scheduling change. The isolation and counterbalanced measurement plans address these unresolved conditions."
          },
          {
            "purpose": "conclusion",
            "reference_ids": [
              "current_claim",
              "stronger_target",
              "N1",
              "N2"
            ],
            "text": "The immediate next step is to freeze the runner configuration and investigate isolation before running the paired timing trial. Retain the 40% improvement as a conditional target, then narrow or reject it if the recorded observations fail its declared criteria."
          }
        ],
        "title": "The description justifies planning a trial"
      },
      {
        "claim_id": "S1",
        "paragraphs": [
          {
            "purpose": "answer",
            "reference_ids": [
              "S1",
              "E1"
            ],
            "text": "The supplied synthetic description reports sequential integration-test groups and possible fixture sharing. This subclaim concerns the contents of that record and carries no production performance result."
          },
          {
            "purpose": "conclusion",
            "reference_ids": [
              "S1",
              "W1",
              "G1",
              "E1"
            ],
            "text": "That bounded finding follows through W1 from the explicit report G1. It can inform the design of an investigation, with the excerpt retained so a reviewer can inspect the source of the interpretation."
          }
        ],
        "title": "What the supplied description establishes"
      },
      {
        "claim_id": "stronger_target",
        "paragraphs": [
          {
            "purpose": "answer",
            "reference_ids": [
              "stronger_target",
              "G2"
            ],
            "text": "The stronger target asks whether a specified parallel configuration achieves at least a 40% median reduction while meeting separately declared outcome and isolation criteria. It remains conditional because the comparative measurements are missing."
          },
          {
            "purpose": "conclusion",
            "reference_ids": [
              "stronger_target",
              "W3",
              "N1",
              "N2",
              "N3",
              "N4"
            ],
            "text": "A paired, counterbalanced trial can supply evidence for that bounded comparison after fixture interference has been investigated. Passing the stated criteria would justify reassessing the target within the tested configuration, with residual uncertainty retained."
          }
        ],
        "title": "The conditional performance target"
      }
    ],
    "residual_risks": [
      "Finite trials may miss rare interference or timing conditions.",
      "The synthetic record supplies no empirical performance evidence."
    ],
    "scope": {
      "configuration": "Linux runner and four test groups; exact runner size, revision and database configuration remain to be frozen.",
      "consequences": "Faster feedback is valuable only when test coverage and outcome reliability remain acceptable.",
      "operating_conditions": "Representative revisions, runner contention and cache states must be sampled and recorded.",
      "system_boundary": "Integration-test execution stage, including fixture setup and teardown.",
      "time_horizon": "The planned trial and identified repository revisions only.",
      "units": "Elapsed time in seconds and paired relative reduction as a percentage."
    },
    "status": "unsupported",
    "stronger_target": {
      "qualifier": "Conditional target requiring paired measurements, declared uncertainty and independent isolation checks.",
      "text": "For the frozen runner configuration and representative test revisions, parallel test groups reduce median CI elapsed time by at least 40%, while preserving the preregistered outcome and isolation criteria."
    },
    "subclaims": [
      {
        "id": "S1",
        "qualifier": "A report of the supplied description only, with no production measurements.",
        "text": "The supplied synthetic description identifies sequential test execution and possible fixture sharing."
      }
    ],
    "warrants": [
      {
        "applicability": "Applies only to the supplied synthetic text and does not establish actual production behaviour.",
        "backing_evidence_ids": [],
        "conclusion_id": "S1",
        "id": "W1",
        "inference": "deductive",
        "premise_ids": [
          "G1"
        ],
        "rule": "An explicit supplied description supports a bounded report of what that description says."
      },
      {
        "applicability": "Supports planning the trial only. No quantitative benefit follows from sequential scheduling alone.",
        "backing_evidence_ids": [],
        "conclusion_id": "current_claim",
        "id": "W2",
        "inference": "practical",
        "premise_ids": [
          "S1"
        ],
        "rule": "A sequential workflow identifies a candidate for a controlled scheduling trial when interference and representativeness are investigated."
      },
      {
        "applicability": "Requires representative sampling and explicit treatment of cache state, fixture overhead and runner contention. The warrant remains a proposed methodological justification.",
        "backing_evidence_ids": [],
        "conclusion_id": "stronger_target",
        "id": "W3",
        "inference": "inductive",
        "premise_ids": [
          "S1",
          "G2"
        ],
        "rule": "Paired measurements on frozen, representative revisions can support a bounded performance comparison when outcome preservation and interference are checked separately."
      }
    ]
  },
  "execution_mode": "offline_synthetic_fixture",
  "limitations": [
    "No model calls, evidence verification or experiments were performed. This is a synthetic demonstration."
  ],
  "request": {
    "claim": "Parallelising the integration-test groups will reduce CI elapsed time by at least 40% without changing test outcomes.",
    "context": "Synthetic demonstration only. A Linux CI runner executes four integration-test groups. The desired decision is whether to trial a parallel runner configuration. No production measurements are supplied.",
    "evidence": [
      {
        "checked_by_user": false,
        "excerpt": "The example workflow runs four integration-test groups in sequence. The groups may share a fixture database. No elapsed-time measurements or isolation checks have been recorded.",
        "id": "E1",
        "kind": "synthetic demonstration",
        "source": "synthetic:example-fixture",
        "title": "Synthetic workflow description"
      }
    ]
  },
  "schema_version": "engarg.demo.v2"
}
```

