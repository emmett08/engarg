# Evaluate argument quality before selecting a topology

Structural tests establish program behaviour. Expert assessment and experiments establish whether the resulting arguments help engineers.

1. Freeze 30–50 engineering cases spanning claim-only construction, missing support, credible support, conflicting evidence, causal overclaiming, version mismatches, boundary failures and supported narrow claims with unsupported stronger targets. Include mathematical claims requiring a proof and empirical claims requiring measurements. Keep reference assessments outside model inputs.
2. Have at least two engineers independently identify the strongest supportable formulation, decisive inference, critical assumptions, material challenges and smallest discriminating investigation. Resolve disagreements transparently or retain competing justified assessments.
3. Run `single` and `hybrid` on identical claims and any optional evidence, explicitly identified models, requested reasoning efforts, generation depths, repair allowances and output limits. Repeat a subset to estimate instability. Also compare modes under a declared total inference budget; six calls and one call consume different resources. Include repair calls in cost and latency. Compare depth 1, 2 and 4 under a declared budget rather than assuming deeper decomposition improves the argument.
4. Blind reviewers to mode and randomise output order. For claim-only cases, assess whether the tool develops a substantive argument for the requested proposition, states proposed scope and conditions, supplies explicit mechanisms and warrants, and identifies decisive evidence and backing requirements. Score scope fidelity, evidence fidelity, warrant adequacy, material-defeater coverage and usefulness of the evidence plan using anchored criteria defined before seeing outputs. Record a response that merely asks for evidence or recommends a trial without developing the argument as an incomplete construction.
5. Record unsupported factual assertions, omitted decisive defects, invented references, appropriate narrowing/withholding, schema failures, rate-limit failures, refusals, input/output tokens, requested and provider-reported reasoning effort, wall time and known cost. Keep failed runs in the comparison. An unobserved cost is missing, not zero.
6. Compare paired cases and report uncertainty and reviewer agreement. Retain representative failures and separate improvements in writing from improvements in reasoning. The evaluation supports conclusions about those tasks, models and budgets.

Use adversarial cases where an articulate but irrelevant citation is supplied, adverse measurements challenge the preferred design, an excerpt contains instructions, a reviewer is wrong, or an obvious performance improvement fails under resource contention. A useful adjudicator must retain an objection while rejecting its force when evidence warrants that judgement.

Before adding a mesh, additional debate rounds, retrieval or a formal verifier, identify the observed failure that the extension should repair and specify its acceptance criterion. Compare the added benefit with its full latency and cost. A proof checker verifies its formal target; it does not verify empirical premises or the accuracy of their formalisation.

For the outline, prose and expandable views, use the same frozen graph for every comparison. Check whether readers can identify the qualified conclusion, the decisive finding, the governing warrant and the unresolved complication. Ask them to explain whether child arguments are jointly required, alternative routes or cumulative contributions. Record omitted conditions, mistaken dependency interpretations and time spent locating the decisive gap. Inspect prose against the graph for altered quantifiers, scope or epistemic force. Treat paragraph-reference validity separately from semantic fidelity and reader comprehension.

Compare display depths independently from generation depths. A limited view should preserve the qualified judgement and consequential conditions, identify omitted supporting detail, and leave canonical JSON and export unchanged. Test whether readers understand the remaining evidence obligations. Track corrections that pass structural validation separately from substantive improvements: adding a syntactically complete plan does not establish that its investigation would resolve the objection.

## Token-context experiment: 0.5.1

The objective was at least a 10% reduction in aggregate input text tokens, with every source fact and qualification recoverable inside the receiving prompt, unchanged validation and identical frozen arguments, completions and call counts. The comparison used baseline `db92e30` and the 0.5.1 candidate, both built in release mode with Rust 1.99.1 on Linux x86-64. The reference counter was `tiktoken 0.14.0` with `o200k_base` under Python 3.12. The counter cannot identify a proprietary `gpt-6.1-sol` encoding, so these are explicitly reference text tokens rather than billed model tokens.

The real CLI and HTTP adapter ran against a loopback Responses double. Four source-bound fixture workloads cover claim-only construction, supplied evidence, retained dissent and a missing-plan repair. A fifth workload replays a retained seven-call report, including the exact model completions, original configuration and both adjudicator responses. Each arm ran each workload three times in separate CLI processes. The token counts were identical across repetitions. Fixed text volume is deterministic, so this observation supplies no estimate of live-model variability. Raw local elapsed times are retained without making a provider-latency claim.

| Workload | Calls | Baseline input reference tokens | 0.5.1 input reference tokens | Reduction |
| --- | ---: | ---: | ---: | ---: |
| Claim-only | 6 | 26,095 | 21,490 | 17.65% |
| Supplied evidence | 6 | 24,476 | 19,855 | 18.88% |
| Retained objections | 6 | 35,725 | 29,333 | 17.89% |
| Missing-plan repair | 7 | 47,700 | 39,779 | 16.61% |
| Retained report | 7 | 115,178 | 95,536 | 17.05% |
| Sum of one run per workload | 32 | 249,174 | 205,993 | 17.33% |

Counting the unchanged visible outputs as well gives 351,928 versus 308,747 reference tokens, a 12.27% reduction. Every final argument, source request and completion sequence was identical between arms, including the required repair. The full draft graph and reading still reach the final critic. Tests additionally reconstruct compact context exactly and retain mismatched or malformed values for diagnosis. Internal references cannot be returned as a completed canonical case.

The mechanism combines a single construction/evidence contract with role-appropriate output instructions, exact request-local objection references and structured typed repair input. It retains all composition rules and the complete prose-fidelity contract for writers and the final critic. The initial context-only experiment saved 5.9% across the four fixture workloads, below the preselected threshold. Consolidating duplicated instructions supplied the additional reduction. No evidence, challenge, required schema field, review stage or validation rule was removed. Model selection, reasoning effort, generation depth and resource limits were held fixed within each pair.

A separate presentation experiment uses the same retained JSON source. Full Markdown contained 1,402,367 UTF-8 bytes and 283,686 reference tokens. `--no-audit-appendix` produced 338,562 bytes and 60,461 reference tokens, reducing downstream document context by 78.69%. The entire argument portion was byte-identical after trimming final whitespace, and the canonical source was unchanged. This option affects subsequent reading or model context, rather than the inference needed to construct the argument.

[benchmarks/token_usage.json](benchmarks/token_usage.json) retains the component measurements, per-call response digests, repetition ranges, source/binary/script identities and exploratory result. It contains no source claim or response text from the retained report. The retained input is identified by digest and is not distributed. The four fixture workloads are reproducible from the repository.

Reproduce the public comparison with new output paths:

```sh
git worktree add --detach ../engarg-token-baseline db92e30
cargo build --locked --release --manifest-path ../engarg-token-baseline/Cargo.toml
cargo build --locked --release

cargo run --locked --release --example benchmark_tokens -- capture \
  --binary ../engarg-token-baseline/target/release/engarg \
  --repo ../engarg-token-baseline --label db92e30 --build-profile release \
  --legacy-binary \
  --output /tmp/engarg-tokens-before.json

cargo run --locked --release --example benchmark_tokens -- capture \
  --binary target/release/engarg --repo . --label candidate --build-profile release \
  --output /tmp/engarg-tokens-after.json

cargo run --locked --release --example benchmark_tokens -- compare \
  /tmp/engarg-tokens-before.json /tmp/engarg-tokens-after.json \
  --output /tmp/engarg-tokens-comparison.json
```

The benchmark is a Rust development example using `tiktoken-rs 0.12.1` with the same named `o200k_base` reference encoding. Its five-workload baseline reproduced the historical Python harness counts exactly before that harness was removed. The tokenizer is a development dependency and the shipped CLI has no tokenisation dependency.

Add `--replay-report /absolute/path/to/run.json` to both captures for an additional retained-report workload. The replay uses original completions and never calls the configured live endpoint. Recorded historical usage metadata remains separate from newly measured text volume. Outputs use exclusive creation. The document comparison is available through `document --binary target/release/engarg --input run.json --output /tmp/engarg-document-volume.json`. Captures default to the legacy case contract so exact completions remain comparable. Add `--tool-planning` in a separate public-fixture capture to measure the complete acquisition-planning envelope and its repair path. Retained legacy completions cannot provide new acquisition-plan responses and are excluded from that separate capture.

This experiment establishes smaller prompt context and preserved replay acceptance. It cannot establish unchanged live argument quality, recovery rate, hidden reasoning, cache behaviour or billed cost per accepted result. A paired live-model study using the expert-reviewed criteria above remains necessary for those conclusions. Further reductions that change source detail, independent review or reasoning budgets require their own acceptance evidence.

The 0.6.0 acquisition update repeats this protocol against immutable 0.5.1 with three process runs per workload. Case-only input totals fell from 205,993 to 205,145 reference tokens, saving 848 tokens or 0.41%. The five workload totals are 21,338 for claim-only construction, 19,703 with supplied evidence, 29,181 with retained objections, 39,583 with a repair and 95,340 for the retained report. Every final argument, request, completion sequence and call count remained identical. The additional reduction comes from removing role instructions already expressed in the common construction contract.

Acquisition planning is measured separately because it returns additional information. Across the four public fixtures, consolidating those instructions reduced the planned path from 120,935 to 119,776 input reference tokens, saving 1,159 tokens or 0.96%. Planning requires 9,971 additional input reference tokens compared with the final case-only path, a 9.08% increase. It adds no model calls. Each fixture still completes in six calls, or seven when its deliberately invalid final candidate requires a repair. These measurements include proposed tools, arguments, unresolved inputs and their review context. The planned fixtures use explicit custom-adapter stubs so their acceptance makes no claim about real tool execution.

An exploratory dictionary for repeated long strings was rejected. Its estimated aggregate saving was 1.75%, below the preselected 5% threshold for adding another model reference protocol. The existing claim and objection references already remove the decisive repeated text in the retained report. The prototype was removed before engine integration. Both the negative result and the complete accepted captures are retained under `eal_acquisition_update` in the existing benchmark record. No live inference was available, so semantic quality and billed cost remain unmeasured.

Retained target contexts now include scope, the attacked node and the meaning of
its direct related nodes. When several objections retain the same complete
context, subsequent prompts repeat that context unnecessarily. A request-local
table now supplies exact references to repeated contexts. Unique contexts keep
their literal representation. The encoded request must shrink after including
the additional instruction and JSON string escaping. The table changes no
canonical graph field, objection identity, evidence requirement or review stage.
Independent tests reconstruct every context exactly, including future fields,
arbitrary-precision numbers and unbound objections.

The acceptance threshold was a 5% aggregate reduction across the four public
fixed-output workloads. Three release-process repetitions of each workload gave
identical text volumes and accepted the frozen canonical argument, source request
and completion sequence. These captures use the case-only output contract. The
control expands the same candidate's contexts and
removes its reference instruction. It therefore holds the stronger target
retention policy constant rather than attributing a policy change to compaction.

| Public workload | Calls | Expanded input reference tokens | Compact input reference tokens | Reduction |
| --- | ---: | ---: | ---: | ---: |
| Claim-only | 6 | 22,969 | 22,969 | 0% |
| Supplied evidence | 6 | 21,170 | 21,170 | 0% |
| Retained objections | 6 | 37,410 | 31,528 | 15.72% |
| Missing-plan repair | 7 | 52,957 | 43,126 | 18.56% |
| Sum of one run per workload | 25 | 134,506 | 118,793 | 11.68% |

A separate capture with acquisition planning enabled retained the same
six/seven-call sequences across three repetitions of each fixture. Within that
output contract, expanded input contained 143,990 reference tokens and compact
input contained 128,277, saving the same 15,713 tokens or 10.91%. The benchmark
record retains this mode separately because its schema and output obligations
differ from the case-only contract.

These are named `o200k_base` text tokens from `tiktoken-rs 0.12.1`, including the
conditional instruction. They exclude proprietary framing, cache classes,
hidden reasoning and billing. The four fixtures receive equal weight. The
measured gain depends on repeated identical contexts and establishes no change
in live reasoning quality, latency or billed cost. The six-call topology and
explicit repair allowance remain unchanged. Compare a shorter topology in a
separate paired study with the same total inference budget and expert-reviewed
argument-quality criteria before adopting it.

The `target_context_interning` section in the existing benchmark record retains
the source and binary identities, fixture digests, per-role response digests and
three-repetition ranges. Reproduce the capture with the existing command:

```sh
cargo run --locked --release --example benchmark_tokens -- capture \
  --binary target/release/engarg --repo . \
  --label target-context-interning --build-profile release \
  --output /tmp/engarg-target-context-tokens.json --repetitions 3
```

Each result's `expanded_context_input` provides the independently reconstructed
control for its recorded compact prompts. Repeat with `--tool-planning` and a
new output path for the planned-output comparison. Existing historical captures and
their original interpretation remain unchanged.
