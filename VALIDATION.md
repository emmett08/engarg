# Verification record

Version 0.5.1 verified on 24 October 2026 on Linux x86-64 using Rust 1.99.1. This record concerns program behaviour. It contains no measured claim that the hybrid workflow produces better arguments than a single model.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 142 passed, 0 failed, 4 external integration tests ignored |
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
| EAL/3 compiler and evaluator compatibility | Retained 0.5.0 verification against eal-mcp `d311d5a4f9e294ba3228127bc5210b48c5de3a12`, using Rust 1.99.1 |
| Real EAL host and MCP server | Retained 0.5.0 verification: 4 external tests passed. Not re-run for this context-only change |
| EAL assumption-to-objection propagation | Retained 0.5.0 verification against the actual evaluator using isolated synthetic observation fixtures |
| Release CLI with real host/server | Retained 0.5.0 verification: export, source/bundle validation, plan, assess, explain, packet, describe and invalid-source exit checks passed |
| Requested and returned service tiers | Exact `gpt-6.1-sol` / `ultrafast` request and differing returned tier retained in HTTP and six-stage CLI tests |
| Optional reasoning effort | Correct nested Responses and root Chat fields, omission when unset, bounded identifiers and explicit HTTP400 failure without fallback passed |
| Reasoning configuration and receipts | CLI overrides environment, environment-only configuration works, requested/reported values remain separate, and old reports deserialize with unknown effort |
| Release reasoning-effort workflow | Single-call release CLI passed against a local HTTP double, retaining requested high and provider-reported medium in its audit and report |
| Reported missing `auditor-5` plan | Reproduced in hybrid and single fixtures; exact diagnostic reaches adjudication and explicit bounded repair; corrected plan must target the objection directly |
| Repair integrity and bounds | Disabled by default; allowance reserved before calls; success uses no unused allowance; exhaustion, deletion and materiality downgrade fail; provider failures never retry |
| Generation depth | Depth 1 rejects a two-layer fixture; depth 2 accepts it with unchanged conditional status; every role receives the limit; longest and shared shortest routes checked |
| Display depth | Outline, prose and HTML limit detailed claim levels without changing canonical JSON; qualifiers and consequential conditions remain visible; existing indentation-only behaviour passes |
| Depth/repair release workflow | Real release executable completed a seven-call hybrid run against a local HTTP double, repaired the targeted gap, retained depth/settings/receipts and rendered three limited views without modifying the report |
| Persistence compatibility | Old run/configuration records deserialize with unknown historical run configuration, unbounded generation depth and zero repairs as applicable |
| Token-context preservation | Exact graph/reading/claim/objection reconstruction, source order and roles, mismatch visibility, short-literal fallback and rejection of reference-bearing final outputs passed |
| Malformed repair input | Duplicate-key candidate text remains verbatim rather than being reparsed into a lossy object |
| Frozen complete-path token comparison | Five workloads, three repetitions per arm, unchanged requests/arguments/completions/call counts, with 17.33% fewer input reference tokens |
| Markdown without audit appendix | Full argument records/registers byte-identical, canonical source unchanged, and 78.69% less downstream reference-token context on the retained report |
| Benchmark helper | Public fixture and retained-report captures, comparison, document measurement and Python compilation passed. Both measured binaries used release builds |
| Remote MCP deployment | Configuration and credential handling tested locally, no real remote service contacted |
| Live model execution | Not performed: no provider API key was available |
| Comparative argument-quality evaluation | Not performed; see EVALUATION.md |
| macOS build | Not executed here; a macOS CI job is supplied |

The tests cover optional input defaults, required-claim schema, no-source conditional arguments, preserved empirical status rules, generation completeness, claim-only rendering and export, argument-reference integrity, unsupported versus refuted status, current-claim dependency closure, hypothetical material challenges, cycles, unattached support, independent first-pass context, retained dissent, call/byte limits, oversized-output audit receipts, refusal, truncation, rate limits, response size limits, credential-safe redirects, CLI inputs and private file creation. They also cover EAL source and bundle identity, conjunction and selected-alternative mapping, review obligations, exact JSON integers, stdin EOF, private export rollback, process input/output/deadline limits, host error envelopes, credential redaction, requested/returned service tiers, alternative-route selection, joint and cumulative support, narrative section organisation and references, template records, shared-node presentation, HTML escaping and depth-bounded outlines. Simulated HTTP success establishes protocol integration rather than vendor/model compatibility or useful reasoning quality.

Latest non-yanked stable direct crate releases were checked against the crates.io sparse index before resolving Cargo.lock:

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
