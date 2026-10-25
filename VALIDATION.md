# Verification record

Version 0.9.0 verified on 24 October 2026 on Linux x86-64 using Rust 1.99.1. This record concerns program behaviour. It contains no measured claim that the hybrid workflow produces better arguments than a single model.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked --all-targets` | 252 passed, 0 failed, 9 explicitly external integration tests ignored by this command |
| `cargo build --locked --release` | Passed |
| Offline demo and fixture validation | Passed, including `demo --claim-only` with no source records |
| Minimal input contract | `--claim TEXT` and JSON containing only `claim` succeed without context or evidence |
| Claim-only provider workflow | Single and six-stage hybrid paths passed against deterministic HTTP doubles, including all output views and EAL export |
| Generation completeness | Empty assessment-only graphs rejected in both modes, while previously saved assessment cases remain valid |
| Claim-only release CLI and real MCP | Positive conditional fixture with ten evidence requirements rendered in all four views, then validated, planned, assessed and retrieved through the actual server |
| CLI through HTTP adapter through all six stages | Passed against a deterministic local API double |
| Invalid final case retains completion audit and produces failure | Passed against the same API double |
| One saved graph renders template, outline, prose and expandable HTML | Passed without provider credentials or further model calls |
| Template fidelity | All 14 components per warrant and shared registers checked |
| Shared nodes, explicit compositions and depth limits | Passed |
| HTML escaping and visible collapsed qualifications | Passed by structural tests |
| Browser visual inspection | Not executed: headless-browser download returned an invalid archive |
| EAL/3 compiler and evaluator compatibility | Passed against actual eal-mcp `32366396b72baa01301eba058f4c65ea735441c1`, including every generated registry variant |
| Real EAL host and MCP server | All installed host, core, native and scientific collector checks passed with the actual host and companion binaries |
| EAL assumption-to-objection propagation | Passed against the actual evaluator using isolated synthetic observation fixtures |
| Release CLI with real host/server | Retained 0.5.0 verification: export, source/bundle validation, plan, assess, explain, packet, describe and invalid-source exit checks passed |
| Requested and returned service tiers | Exact `gpt-6.1-sol` / `ultrafast` request and differing returned tier retained in HTTP and six-stage CLI tests |
| Optional reasoning effort | Correct nested Responses and root Chat fields, omission when unset, bounded identifiers and explicit HTTP400 failure without fallback passed |
| Reasoning configuration and receipts | CLI overrides environment, environment-only configuration works, requested/reported values remain separate, and old reports deserialize with unknown effort |
| Release reasoning-effort workflow | Single-call release CLI passed against a local HTTP double, retaining requested high and provider-reported medium in its audit and report |
| Reported missing `auditor-5` plan | Reproduced in hybrid and single fixtures; exact diagnostic reaches adjudication and explicit bounded repair; corrected plan must target the objection directly |
| Repair integrity and bounds | Disabled by default; allowance reserved before calls; success uses no unused allowance; exhaustion, deletion and materiality downgrade fail; the repair loop does not retry provider failures |
| Synthesis-to-adjudication integrity | A synthesis-only material objection cannot disappear; a specialist objection marked material during synthesis cannot be downgraded; an explained scope revision can change disposition while retaining the challenge |
| Generation depth | Depth 1 rejects a two-layer fixture; depth 2 accepts it with unchanged conditional status; every role receives the limit; longest and shared shortest routes checked |
| Display depth | Outline, prose and HTML limit detailed claim levels without changing canonical JSON; qualifiers and consequential conditions remain visible; existing indentation-only behaviour passes |
| Depth/repair release workflow | Real release executable completed a seven-call hybrid run against a local HTTP double, repaired the targeted gap, retained depth/settings/receipts and rendered three limited views without modifying the report |
| Persistence compatibility | Old run/configuration records deserialize with unknown historical run configuration, unbounded generation depth and zero repairs as applicable |
| Token-context preservation | Exact graph/reading/claim/objection reconstruction, source order and roles, mismatch visibility, short-literal fallback and rejection of reference-bearing final outputs passed |
| Malformed repair input | Duplicate-key candidate text remains verbatim rather than being reparsed into a lossy object |
| Frozen complete-path token comparison | Five workloads, three repetitions per arm, unchanged requests/arguments/completions/call counts, with 17.33% fewer input reference tokens |
| Markdown without audit appendix | Full argument records/registers byte-identical, canonical source unchanged, and 78.69% less downstream reference-token context on the retained report |
| Benchmark helper | Rust development example reproduced all five historical reference-token counts exactly and passed capture/comparison checks. Both measured binaries used release builds |
| Acquisition planning | Six calls retained, every need bound exactly once, missing binding uses the existing repair budget, legacy direct-case mode remains available |
| Paired EAL export | Source and native tools TOML always written, explicit companion paths and stdout covered, aliases and failed output transactions preserve existing files |
| Observation and acceptance | Native EAL collection made synthetic JSON available while the claim stayed unsupported without independent reviews |
| Collector boundaries | Fixed read operations and literal argv, safe all-namespace name selectors, relative GitHub paths, bounded strict JSON, duplicate response-key rejection, exact integer preservation, exact HTTPS credential origin and explicit environment forwarding passed |
| Collector catalogue | Neutral default adapters and operator-selected scientific schemas checked. Typed arrays/integers, local schema references, nulls, unresolved inputs, reserved IDs and bounded windows covered |
| Preparation | Each need retains its adapter and scope. Seven private files and exact structured reviews replace source reparsing, comment substitutions and global infrastructure configuration |
| Versioned identity | Actual v1 custom/native/HTTP bundles admitted through the real host. V2 identity remains stable across producer-only version changes and changes when selected contracts change |
| Credential and file boundaries | Registry-declared custom environment names work only when requested. Known sensitive values in semantic responses fail safely. Explicit public configuration preserves its exact value. FIFOs and untrusted leaf aliases fail regular-file admission. Operator registry aliases select a canonical target |
| Producer–consumer CI | Pinned EAL producer catalogue compared as JSON. Every external test fails on missing prerequisites when selected by the mandatory compatibility job |
| Plain argument view | Sentence-line records preserve claim qualifiers, composition, nested conditions, inference types, investigations, shared arguments and bounded indentation |
| Schema preservation | Canonical request/case schemas preserved. RunConfig adds an optional selected catalogue. EAL export v2 separates producer and projection identity, while v1 source-bound bundles remain readable |
| Remote MCP deployment | Configuration and credential handling tested locally, no real remote service contacted |
| Live model execution | Not performed: no provider API key was available |
| Comparative argument-quality evaluation | Not performed; see EVALUATION.md |
| macOS build | Not executed here; a macOS CI job is supplied |

The tests cover optional input defaults, required-claim schema, no-source conditional arguments, preserved empirical status rules, generation completeness, claim-only rendering and export, argument-reference integrity, unsupported versus refuted status, current-claim dependency closure, hypothetical material challenges, cycles, unattached support, independent first-pass context, retained dissent, call/byte limits, oversized-output audit receipts, refusal, truncation, rate limits, response size limits, credential-safe redirects, CLI inputs and private file creation. They also cover EAL source and bundle identity, conjunction and selected-alternative mapping, review obligations, exact JSON integers, stdin EOF, private export rollback, process input/output/deadline limits, host error envelopes, credential redaction, requested/returned service tiers, alternative-route selection, joint and cumulative support, narrative section organisation and references, template records, shared-node presentation, HTML escaping and depth-bounded outlines. Simulated HTTP success establishes protocol integration rather than vendor/model compatibility or useful reasoning quality.

The following direct crate versions are fixed by Cargo.lock for this verification. Earlier release-selection checks are retained below:

| Crate | Selected version |
| --- | --- |
| clap | 4.6.7 |
| reqwest | 0.13.5 |
| tokio | 1.54.1 |
| serde | 1.0.229 |
| serde_json | 1.0.154 |
| schemars | 1.2.3 |
| sha2 | 0.11.1 |
| thiserror | 2.0.21 |
| tempfile (tests) | 3.27.0 |
| jsonschema | 0.46.10 |
| toml | 0.9.12+spec-1.1.0 |
| rustix | 1.1.5 |

Rust 1.99.1 was checked against the official stable channel manifest. The supplied CI workflow pins actions/checkout v7.0.1 to its resolved commit and runs the pinned toolchain on Ubuntu and macOS. Hosted results belong to the commit checks. macOS was not executed locally.

Release ThinLTO failed to link with undefined hidden symbols in this environment. The committed release profile uses ordinary code generation (`lto = "off"`), for which the release build and executable smoke checks passed. This is a build-compatibility choice, with no measured performance claim.

The real MCP tests start the official host and server against isolated temporary SQLite databases. Missing observations remain unavailable and claims remain unsupported. Synthetic accepted observations appear only in a temporary evaluator regression fixture, where they test whether a challenge to an assumption defeats a dependent objection. Production conversion never creates observation records.

Version 0.5.0 preserves the six-stage workflow, independent first passes, complete-call observer, canonical argument schema, evidence-status rules, EAL mapping and toolchain. It deliberately revises the previous no-repair policy only for an explicit `max_repairs` allowance, reserved within `max_calls`. Failed content is never accepted merely because a repair was attempted. No new dependency or named pattern was introduced.

`EXTRACTED`: generation construction, retained-objection and depth checks now live in `output_validation`, so draft feedback and final validation share one definition. The bounded correction loop lives in `engine/repair`, apart from the fixed topology. `argument_depth` owns one iterative graph index reused by generation checks and presentation selection. Longest routes constrain construction; shortest routes let a shared shallow claim remain inspectable. This index runs in time and space proportional to graph size and uses no recursive traversal.

`RETAINED`: the engine's `prompt` composes fixed role instructions with a numeric construction limit at its existing request-construction boundary. `hybrid` still owns dependency ordering and context disclosure, adding draft diagnostics without selecting another topology. Presentation methods retain format-specific output and call the shared depth index; they do not prune or revalidate a modified graph. The CLI composition root builds one typed run configuration for the engine, audit and report. Rust 2024, unsafe prohibition, rustfmt and the existing all-target Clippy warning profile remain in force.

The new public Rust configuration/report fields require source updates for callers using complete struct literals. Saved JSON compatibility is tested. Deeper reruns remain independent stochastic assessments, and no comparative quality improvement or guaranteed vendor-model recovery rate has been measured. The actual user's completion payload was not supplied; the exact reported cross-reference failure was reproduced with a synthetic conditional argument.

Reproduce the external checks after building or installing the reference EAL binaries:

```sh
EAL_CORE=/absolute/path/to/eal-core cargo test --locked --test eal_export
ENGARG_EAL_HOST=/absolute/path/to/eal-host cargo test --locked --test eal_live -- --ignored
```

No source retrieval, experiment execution, independent source authentication, statistical analysis or formal proof checking is performed by the argument workflow. A structural pass cannot certify truth, relevance, completeness or soundness. The next scientific validation step is the paired expert-reviewed evaluation described in EVALUATION.md.


Version 0.5.1 preserves the canonical schemas, public Rust record fields, provider wire formats, evidence-status validation, fixed topology and repair/resource budgets. `EXTRACTED`: `engine/context` owns exact request-local references and their encoded-byte guard, so draft review and typed repair share one serialization policy. Assessment parsing now feeds both its compact context and retained objection register once, replacing the previous repeated parse and duplicated challenge text. `RETAINED`: `prompt` selects fixed role instructions and the reference primer, with the numeric depth rule at the existing construction boundary. `system` composes the shared core and the applicable output/review contracts. `validate_or_repair` retains ownership of the bounded correction loop and delegates candidate encoding. `report_content` selects the existing complete template with or without its audit appendix. No new production dependency or design pattern was added.

The first untouched baseline test run encountered two transient EAL mock-host spawn failures before request exchange. Four independent focused reruns passed all nine host tests, and the final full suite passed all 142 tests. The underlying OS error was not retained by that adapter, so its cause remains unknown. No retry, timeout relaxation or weaker assertion was introduced. External EAL executables were unavailable in this fresh workspace, so the four ignored live integrations and optional external-compiler checks were not re-run. The EAL export and host source remain unchanged, and deterministic export/CLI regression tests passed.

No live provider key was available for this task. Frozen-output replay establishes preserved context and deterministic acceptance, while comparative live argument quality and billed cost remain unmeasured. See EVALUATION.md and benchmarks/token_usage.json for the measured boundaries and raw evidence.

Version 0.6.0 starts from `5d60f33fde151a809196e3a3239310a2bbdb39aa`. Its preservation contract covers the canonical schemas, supplied evidence identity and status rules, retained dissent, explicit composition, graph and prose correspondence, independent first passes, complete-call receipts and all resource and repair budgets. A separate internal provider envelope identifies tools during construction without another model call. The unchanged transcript retains that envelope for deterministic export. Known collector choices are proposals whose arguments still need review against the intended investigation.

`EXTRACTED` the `acquisition` module owns catalogue policy and binding validation, `eal_acquisition` owns collector projection and source-archive identity, `eal_observe` owns bounded read adapters and `presentations/plain` owns sentence-line inspection. Export destinations and supported credential names have focused policies. `RETAINED` the engine owns its fixed dependency ordering, `tool_prompt` adapts role context, the repair loop corrects the case and bindings together, and the EAL builder keeps one declaration sequence and reference map. Rust 2024, the pinned toolchain, unsafe prohibition, rustfmt and the existing Clippy profile remain governing. No new production dependency or generic agent framework was introduced. The only new crate is the development tokenizer, `tiktoken-rs 0.12.1`, replacing the Python benchmark dependency.

Request and case schema output was compared byte-for-byte with the immutable 0.5.1 binary. The case schema SHA-256 is `ffe4f82380fa2bf2f1a0f231a094d2bf072d0efe926dcca13d7ee0e963763b9c` in both arms. Existing direct-case Rust APIs and saved reports remain supported. Local `HostConfig` struct literals need the new registry and collector-environment fields. Tool planning changes the internal model response contract and raw transcript contents while keeping the public report schema unchanged.

The performance pass reproduced the prior baseline, consolidated role instructions already supplied by the common contract and measured complete accepted paths with three repetitions. The same-case comparison saved a further 848 input reference tokens across five workloads, from 205,993 to 205,145. The four-workload planning path saved 1,159 input reference tokens against its pre-consolidation version. Planning adds measured context and output volume, while retaining six calls and the same bounded repair path. No new paid inference or billing measurement was performed. A generic long-string reference table was rejected after its 1.75% saving missed its predeclared 5% threshold. See EVALUATION.md and the existing benchmark record for counts and limits.

A full-suite run exposed the old HTTP fixture's direct-case-only schema detection after the CLI default changed to planned envelopes. The fixture now handles both schemas, verifies retained plan identity and has a bounded accept deadline. A later run encountered the previously observed transient mock-host spawn failure. The immediate focused rerun passed. No production retry or weaker assertion was added. Concurrent incremental builds also produced a local invalid-session linker error during development, so final local checks used `CARGO_INCREMENTAL=0`.

The plain view preserves authored values and technical syntax. Its punctuation and sentence-style requirements are generation instructions rather than a semantic rewriting or certification pass over historical reports. Custom investigation stubs and independent review contracts require an operator-provided adapter or admissible matching observations. One collector is selected per evidence need. A custom adapter can combine operations for a more complex investigation.

The first 0.6.0 macOS CI run caught a registry-path test expectation using the `/var` spelling while the host correctly passed its canonical `/private/var` path. The assertion now compares the canonical path and the fixture explicitly supplies a symlink, exercising the same distinction on Linux. Production path resolution is unchanged.

The push run at `40086848025` exposed a separate macOS socket-mode defect in the local HTTP double. Accepted connections could retain the nonblocking listener mode and return `WouldBlock` before the request arrived. Both the end-to-end fixture and the benchmark reader now explicitly select blocking mode before their existing read deadlines. Two regression tests force a nonblocking accepted socket and send fragmented requests after a delay. CI runs all targets so the benchmark regression executes on both operating systems. The provider, argument workflow, schemas and token-counting policy remain unchanged.

Version 0.7.0 starts from EngArg `34b360823845b88e03b2a2b83f60bb6b4627e9c3` and verifies native collection against eal-mcp `48fe2186974f2281a45914c3b41e1cdcb16a5147`. The supplied example contained 33 custom acquisition choices and one unresolved Kubernetes choice. The previous exporter rendered every choice as a missing-file import. Export now preserves known commands and emits explicit custom command implementation stubs. The caller must still configure executable paths and unresolved scope before collection.

`EXTRACTED` `native_collectors` owns the declarative native contract table, strict source input projection and native command templates. `collector_registry` owns binding selection and separates native, generic and custom renderers. `RETAINED` acquisition validation enforces exact evidence-need binding and now checks native time arguments. The EAL builder retains ownership of graph projection and obligation order. The CLI boundary reads bounded operator overrides and delegates merge/validation to acquisition policy. No new production crate, model call, arbitrary shell execution or canonical case field was introduced. Rust 2024 and Rust 1.99.1 remain the EngArg profile. The current EAL tools were built with their own pinned Rust 1.90.0 profile.

The native integration test invokes the real host, server, collector and command supervisor. Local CLI doubles simulate a successful empty Kubernetes response and an exact GitHub commit response. A loopback HTTP server simulates an empty Tempo search. Native records are collected successfully and their acquisition evidence is available. Kubernetes retains `all_present = false`. Tempo retains `all_matched = false`, `search_exhaustive = false` and unavailable negative inference. Each engineering claim remains unsupported while its independent review obligations are open. No production cluster, GitHub API or Tempo service was contacted.

Operator overrides are checked for exact target identity and duplicate entries. The CLI test verifies that re-exporting a saved report retains its canonical argument and source report bytes. Request and case schema outputs remain byte-identical to 0.6.0. The internal planning catalogue expands to native Kubernetes, GitHub and Tempo contracts. These additions increase planning context, with no token-cost reduction claimed for this feature.

Native Tempo credentials stay opt-in at the operator registry boundary. The generated native command contains no token or tenant flags until an operator approves its exact endpoint. Generic HTTP collection retains its explicit HTTPS credential-origin check. Scope and configuration placeholders remain visible and fail acquisition before any missing observation can become accepted evidence.

Version 0.8.0 starts from `b72a958c8b11f29cdd5bc2dd4d02cd7838aea155`. Its transport policy follows the current official OpenAI rate-limit and error-code guidance. The previous CLI stopped every 429 response immediately and discarded the distinction between throttling and permanent credit or quota failures. CLI recovery now has explicit count, delay, concurrency and total-call bounds. The no-retry `HttpModel::new` constructor remains available for existing consumers.

`EXTRACTED` `provider/http` owns bounded HTTP attempts, admission, receipts and response-body limits. `provider/rate_limit` owns safe error classification and retry timing. `RETAINED` the model adapter owns the wire format and completion decoder. The CLI composition root configures transport and audit writing while the engine retains its existing call and repair policies. No case/request/report record field or model request body changed. HTTP attempts are additional transport operations and their finite upper bound is documented separately from logical calls. New settings appear in audit start metadata and existing report limitations. Request timeouts now include admission and backoff intentionally.

The new production dependency is `httpdate 1.0.3`, verified against the crates.io index, for standard HTTP-date Retry-After parsing. Jitter uses the standard library's independently seeded hasher and a sequence counter. No random-number crate or provider SDK was added. Reqwest's own retries and redirects remain disabled, avoiding nested attempt multiplication and credential forwarding.

Two policy tests and thirteen HTTP/CLI integration tests exercise 429 recovery, minimum delay, delay overflow and date hints, cap/deadline refusal, quota/type/code precedence, exact repeated payloads, finite exhaustion, audit failure, cancellation while waiting or queued, and concurrency. The real CLI test recovers from one 429 with two physical attempts under `--max-calls 1`, produces exactly one canonical completion and retains the configured transport policy plus both HTTP receipts. Sensitive error text stays outside diagnostics and audit metadata. No live provider request was performed. Rust 2024, Rust 1.99.1, the unsafe prohibition, rustfmt and all-target Clippy remain governing.

The 0.9 boundary change extracts collector contract admission into
`collector_catalogue`, typed generic bindings into `collector_registry`, and
bounded POSIX file reading into `bounded_file`. Export owns structured review
requests before rendering. `eal_prepare` retains per-need plans and only
configures the resulting registry. `legacy_native_collectors` retains the frozen
v1 source projection rules so earlier evidence-bound source can be reconstructed.
New native descriptions come from the pinned producer JSON contract checked in CI.

The domain graph, provider and argument-presentation responsibilities remain in
EngArg. Generic EAL request/observation contracts and optional collector discovery
are owned by the EAL workspace. These changes preserve measured negatives,
failed collection and unavailable conclusions as distinct results.

## File admission policy

Public input boundaries check an opened regular file and enforce their byte
allowance before parsing. On POSIX, untrusted leaf aliases are rejected with
non-following, non-blocking opens. Parent-directory aliases resolve to a selected
canonical parent. The operator tools registry has an explicit compatibility
exception. Its leaf alias resolves once and the canonical target is passed to
the host. Retargeting the original alias does not change that selected path.

| Input | Byte allowance |
| --- | --- |
| Analysis request | Explicit `--max-input-bytes`, positive and configurable |
| Validation request and case | 1 MiB each |
| Saved rendering report | 16 MiB |
| EAL report, source, acquisition-plan override and context | 16 MiB each |
| Collector catalogue at every CLI boundary | 1 MiB |
| Operator tools registry | 1 MiB |
| Scoped inputs and retained import/review inputs | 1 MiB each |
| Delegated retained artifacts | 1 MiB each with a separate 4 MiB aggregate allowance |

`tests/file_admission.rs` exercises public CLI boundaries with sparse oversized
files, exact limits, malformed and non-UTF-8 inputs, directories, missing files,
FIFOs and FIFO aliases. Direct reader checks cover empty files and parent aliases.
Its parent watchdog terminates a blocked test process so a regression cannot
stall the test runner indefinitely.

The analysis deadline begins before file admission and the remaining budget is
checked before provider execution. Regular-file admission bounds allocation and
rejects unsupported special files. It cannot pre-empt every operating-system
filesystem stall. Path-identity checks also do not establish immutable contents
during a concurrent in-place write. Observation pins and correspondence checks
remain separate requirements.

## Workflow and retry performance

The 25 October 2026 inventory found one workflow in each repository and no
existing local actions or reusable workflows. EngArg's `ci.yml` checks each PR
merge revision and each main push, with independent Ubuntu/macOS `checks` jobs
and `EAL producer and consumer contracts`. Its dependencies are the pinned
checkout action, the Rust Docker toolchain image, both authoring scripts and the
pinned EAL workspace. EAL's `rust.yml` remains manually dispatched, with
independent Rust/application/coordination, context-experiment and parser
regeneration jobs. Regeneration uses the existing hash-checked ANTLR generator.
The pinned checkout and cache actions run Node 24 and call no reusable workflow.
Job names, events, platform coverage, read-only permissions and test selections
are retained. Administrative required-check settings were unavailable.

The complete cold baseline is [run 40189894069](https://github.com/emmett08/engarg/actions/runs/40189894069)
at `95d2b263a626329afafbd67fa7a3ee4ecb925c99`, Rust 1.99.1 for EngArg and
1.90.0 for the pinned EAL runtime. All three jobs succeeded. GitHub job/step
timestamps give these observations; they are one run, not independent repeated
performance trials:

| Workload | Elapsed seconds |
| --- | ---: |
| Ubuntu job 121158447679 | 1207 |
| Ubuntu isolated toolchains | 434 |
| Ubuntu release build | 125 |
| Ubuntu cold runtime install, cache-hit startup and probe | 403 |
| macOS job 121158447673 | 694 |
| macOS runtime installation and verification | 312 |
| EAL contract job 121158447480 | 140 |

The jobs total 2041 unweighted runner-seconds. Workflow wall time was 1210
seconds, including about two seconds before the first jobs started. Runner
classes differ, so the sum is capacity usage rather than an invoice estimate.
Earlier same-implementation runs 40188241422 and 40188244465 executed both push
and PR jobs for `ff6e5a9`; both Ubuntu jobs reached the former 20-minute limit.
The existing main-only push filter removes that duplicate PR invocation while
preserving accepted-main verification. Superseded revisions of the same PR now
cancel together; main pushes keep independent concurrency groups.

Runtime logs show four package builds taking 10.47, 121, 237 and 24.58 seconds.
Within those installs, `serde` compiled three times and `libc` twice because the
selected dependency features differed. The installer now verifies a temporary
checkout of the same admitted URL and exact revision, builds only the four
runtime packages together, and stages exactly the same six executable names.
It retains Rust 1.90.0, `--locked`, private ownership, inherited process locking,
binary hashes and the success receipt. A valid receipt still avoids compilation.
No installed prefix or runtime target is restored by the workflow cache, so the
real cold-install and subsequent cache-hit checks remain exercised.

EngArg shares one cache action between the platform and compatibility workloads.
Keys separate OS, architecture, workload, toolchains, compiler/runner image and
build configuration; Cargo still executes every required check. EAL retains its
workspace cache, adds an isolated experiment cache, and builds the release
workspace and coordination worker together. Its last measured manual dispatch,
40105751849 at `7bd9948`, took 365 seconds for Rust and 44 for the experiment;
the former separate worker build rounded to zero seconds. Consolidating that
invocation is a maintenance reduction, not a demonstrated speed-up.

The implementation verifies four installer compiler invocations become one,
and a script replanning attempt invokes no authoring command or provider key.
Focused failures cover the source revision, missing/outside build artifacts,
interrupted locks, tampered companions, changed requests and failed replanning.
The source-level shell/Python suite and unique-key YAML/event coverage checks
pass. A Rust 1.90 temporary-workspace probe confirms combined library, binary
and named-example selection. `actionlint` was unavailable.

Hosted candidate cache-hit rates, cache transfer overhead, hosted elapsed-time
improvement and billed savings remain unmeasured hypotheses. Docker toolchain
execution remains the largest observed hosted stage; no test was removed or
sharded. The checks matrix now uses the same line-table debug information and
disabled incremental compilation as the compatibility and EAL jobs. This keeps
filename/line backtraces while dropping variable/parameter debugger detail and
incremental-session cache growth. A tiny Rust 1.99.1 contract probe retained
`opt_level=0`, debug assertions and overflow checks, passed a caught-overflow
test, and created no incremental files. No hosted cache-size reduction is claimed.

### Controlled local runtime installation

Verified local improvement: the final installer took 126.511 seconds against
216.755 seconds for the original, a 90.245-second reduction (41.63%, 1.71 times
as fast). Both arms used EAL `cb5013c37ef79331e79ee0337862aec58d1ee5cb`, Rust
1.90.0, the same Linux container and shared warm Cargo downloads, with separate
empty owned installation prefixes and compiler targets. EngArg code was frozen
at `f86103ca57c7bb9e2c42b20d32a3a3eabd422ecc`. This exceeds the predeclared 10%
elapsed-time threshold for this workload.

| Observation | Original installer | Combined build |
| --- | ---: | ---: |
| Cold-prefix installation, seconds | 216.755 | 126.511 |
| Cargo compilation commands | 4 | 1 |
| Non-version/print rustc invocations | 382 | 292 |
| Included `<unattributed>` compiler calls | 13 | 10 |
| Cached startup, seconds | 0.145 | 0.127 |
| New compilations on cached startup | 0 | 0 |
| Real EAL describe, seconds | 0.044 | 0.052 |

The compiler count fell by 90 (23.56%). An identical forwarding wrapper counted
both arms, retained unattributed calls explicitly, and separated version/print
probes. `serde` compiled three times in the original and once in the candidate.
Both installations retained the same six executable names, passed their own
revision-bound binary-hash receipts, and advertised EAL/3 and `structured/1`
through the real installed host. Each task-owned compiler target was removed
only after retaining binaries, receipts, logs and metrics.

The original source is from `6e9f9f7`, SHA-256
`35ff6e34ef7afe12fecae3a3ca9611d77392b1d2408fadd1909355a8c7655afc`.
The measured candidate SHA-256 is
`2d4360a9ca0ad7f0dc7a6265094eb441384d760e079c453c0afb35576cca4bef`.
Only the temporary runners' source-path argument changed so both inferred the
same final EngArg Cargo pin. The self-contained counts, phase times, source and
input hashes, binary hashes and raw-log checksums are retained in
[`benchmarks/installer_profile.json`](benchmarks/installer_profile.json).

This is one baseline-first pair, not an estimate of variation or tail latency.
The later arm may benefit from warmer download and OS caches, and both elapsed
times include wrapper overhead. A reporting error after the completed baseline
checks paused bookkeeping for 173.743 seconds before the candidate; no baseline
was repeated or calls discarded. Millisecond cache/startup differences do not
establish speed changes. Hosted CI timing and invoice effects remain unmeasured.

## Route selection kernel

The 25 October 2026 selection check isolates beam expansion and sorting from
capability validation, schema compilation, argument materialisation, discovery,
provider execution and model calls. It compares the previous string/prefix-copy
loop with the production backpointer/interned-ID beam on the same synthetic
workload: 64 adequate candidates per acquisition, distinct request groups on
each row, three cost dimensions and a 32-state frontier. Both modes assert the
same selected path, cost vector, source-group count and pruning flag.

| Acquisitions | Previous kernel, ms | Current kernel, ms |
| ---: | ---: | ---: |
| 64 | 592.884 | 17.266 |
| 128 | 2475.757 | 31.340 |
| 256 | 11299.975 | 79.993 |
| 4096 | Not run | 3113.388 |

These are single local observations using Rust 1.99.1 and `-O`; they do not
measure complete planning latency, model token use or comparative argument
quality. Raw output, compiler identity and measured source hashes are retained
in `benchmarks/selection_profile.json`. Reproduce from the repository root:

```sh
rustc --edition 2024 -O benchmarks/selection_profile.rs -o /tmp/engarg-selection-profile
/tmp/engarg-selection-profile previous 256 64
/tmp/engarg-selection-profile current 256 64
```

The focused `cargo test --locked --lib route_planning` check passed 29 tests.
An independent exhaustive small-plan definition checks unpruned cost minima
and lexical ties; other tests preserve sharing, bounded-search reporting,
arithmetic bounds and explicit configuration preference. Dynamic fan-out costs
remain unknown rather than charging one invocation for an unknown number of
IDs. Per-step source-operation invocation bounds exclude transport admission and
provider-internal requests. They do not assume reuse, since changing freshness
or a later assessment can require a refresh. Actual runtime metrics remain the
source of measured requests, bytes and elapsed time.

## Integrated closure checks

EAL `cb5013c37ef79331e79ee0337862aec58d1ee5cb`, using Rust 1.90.0, passed
formatting, workspace/all-target Clippy, 584 reported workspace tests and the
installed application and coordination smoke checks. EngArg code
`f86103ca57c7bb9e2c42b20d32a3a3eabd422ecc`, using Rust 1.99.1, passed formatting,
all-target Clippy, 514 default Rust tests, 64 native/cross-repository tests with
`--include-ignored` including batch/replanning, and 86 Python tests. The
workflow profile change above has its separate small contract probe; the full
hosted checks remain the verification of that runner configuration.

Local Docker was unavailable, so the integrated generated-tool Docker test
remains pending hosted CI. No live OpenAI key was available. Live domain-harness
execution, comparative argument quality and live token-cost comparison remain
unmeasured. These checks do not establish globally optimal model topology or
universal planning, runtime or monetary-cost improvement.
