# Verification record

Version 0.3.1 verified on 23 October 2026 on Linux x86-64 using Rust 1.99.1. This record concerns program behaviour. It contains no measured claim that the hybrid workflow produces better arguments than a single model.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` with `EAL_CORE` set | 119 passed, 0 failed, 4 external integration tests ignored |
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
| EAL/3 compiler and evaluator compatibility | Passed against eal-mcp `d311d5a4f9e294ba3228127bc5210b48c5de3a12`, using Rust 1.99.1 |
| Real EAL host and MCP server | 4 separately invoked external integration tests passed |
| EAL assumption-to-objection propagation | Passed against the actual evaluator using isolated synthetic observation fixtures |
| Release CLI with real host/server | Export, source/bundle validation, plan, assess, explain, packet, describe and invalid-source exit checks passed |
| Requested and returned service tiers | Exact `gpt-6.1-sol` / `ultrafast` request and differing returned tier retained in HTTP and six-stage CLI tests |
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

Reproduce the external checks after building or installing the reference EAL binaries:

```sh
EAL_CORE=/absolute/path/to/eal-core cargo test --locked --test eal_export
ENGARG_EAL_HOST=/absolute/path/to/eal-host cargo test --locked --test eal_live -- --ignored
```

No source retrieval, experiment execution, independent source authentication, statistical analysis or formal proof checking is performed by the argument workflow. A structural pass cannot certify truth, relevance, completeness or soundness. The next scientific validation step is the paired expert-reviewed evaluation described in EVALUATION.md.
