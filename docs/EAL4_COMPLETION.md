# EAL/4 implementation and completion checks

This document maps the completion blockers to implementation and reproducible
checks. The verification record below separates repository checks from installed
execution. A fixture-validation test alone does not close an execution blocker.

## Implementation mapping

| Completion blocker | Current implementation | Verification boundary |
|---|---|---|
| Executable meaning depended on an extra selector | EAL/4 defines `bind`, `derive` and authored method queries directly. The `profile "execution/1"` selector and semantic/AST field are removed. Old roots and imports are rejected. | EAL core `executable_program` and `executable_method_inputs`; regenerated grammar recogniser |
| Parallel authored-only and support-gate compilation | EngArg package build and EAL export share `support_export::compile_direct`; the old public authored-only exporter and bundle-obligation model are removed. | `executable_package`, `executable_modes`, `eal_export`; moved-package execution |
| Direct report output was rejected by downstream consumers | Strict `engarg.run.v3` types include direct support, bindings, design review, runtime receipt and recovery metadata. Public consumers use `report::read_direct`. | `support_cli`, `direct_implementation`, `eal_cli`, `tooling_cli` |
| A revision could not be revised or built again | A revision emits the same typed report. Its bounded history retains exact parent snapshots and dissent; changed support is revalidated, bindings rebound or reset, and derived tooling/assessment metadata invalidated. | `argument_revision`; public author → revision → revision → build test and installed-host counterpart in `support_cli` |
| Typed reading lost nullable/default encodings in replanning history | Sparse encoding records restore the exact parent JSON value only when its complete typed meaning is unchanged. New current planning uses its typed encoding before its digest is recorded. | Nullable and nested repeated-roundtrip cases, precise-number wire tests, forged argument/support rejection and failed-admission state preservation in `replan_cli` and `report::encoding` |
| Edited source or stale metadata could bypass revision | Direct, discovery and argument-only reports check their declared origin and retained final completion at the original authoring cut. Adding support outside an explicit revision is rejected. Review, target-runtime receipt and revision identities are validated. | Tampered argument, origin relabelling, unsupported origin, injected support, missing/changed review, stale target and changed parent cases in `support_cli` |
| Tool implementation changed a revised report's source identity | Implementation checks its selected runtime before provider calls and preserves the immutable authoring-runtime receipt. | Revised-report implementation and incompatible-host admission in `tooling_cli` |
| Valid debate transcripts were rejected as checkpoint histories | Ordering, replay and compaction derive participant identities and rounds from the admitted debate configuration, retaining parallel arrival order and routed dissent. | Participant/round/order tampering and replay tests; installed tickets across all nine topologies |
| Missing-operation generation remained tied to registry scaffolding | `tooling::direct` derives requests from acquisition specifications, verifies generated implementations and merges direct TOML exports. Main authoring and `eal implement` share this stage. | `direct_implementation`; Python, JavaScript and Rust public chains in `tooling_docker` |
| Ticket execution bypassed direct generation | An explicit ticket `implementation` section selects the same generation stage, with pinned toolchain and bounded provider calls; resulting tools feed the same package builder. | Configuration admission tests and installed `ticket_native` cases |
| An incompatible target could be discovered after paid authoring | Selected runtime preflight checks EAL/4 features and method contracts before provider calls. Absent selection is explicitly an embedded-contract check; ticket execution probes its selected host. | `runtime_compatibility` tests and selected-host CLI/ticket admission tests |
| MCP stdio lifetime differed from supervised commands | Official SDK traffic uses the existing process supervisor's bounded duplex mode. Descendants are cleaned up before lease release on normal completion, timeout and owner death. | EAL `eal-process` tests `duplex`/`process` and runtime `mcp_lifecycle` |
| Interface preparation or pinning lost actual provenance | Optional EAL preparation performs bounded init/list only and writes a new private pinned manifest. Runtime checks pins before calls and retains actual interface identity on completed responses, including errors. | EAL runtime `mcp_preparation`, direct tool admission and portable executable-package cases |
| Packet/model/formal projections rejected current dependency outcomes | The pure core owns typed availability and predicate issue vocabularies. Native evaluation, packet projection and model-context admission share them. | Native assessment → packet → model-context and compile → formal-export regressions include absent/stale dependencies, failed derivation, unresolved/invalid predicates and malformed reviews |
| A late invalid binding could allow earlier acquisition | Preparation and collection share admission of the whole selected closure: configured versions, credential policy, local implementation pins and complete literal inputs are checked before external source IO. An absent binding remains a durable unavailable observation during collection. | `mcp_preparation::all_selected_bindings_are_admitted_before_any_source_or_observation` exercises preparation, collection and execution entry points |
| Copied local tool scripts could change without invalidating the package | Package building and generated-export merging pin admitted local implementation files referenced by command arguments, preserving operator pins. Source data remains observation input unless explicitly pinned. | Package mutation tests reject rebuilding a changed copied script and installed execution before that script runs |
| Reasoning modes lacked composed adverse execution | Seven EngArg modes use the same compiler/runtime across nine worlds, plus scoped-review adversities. Native methods have explicit work admission; practical custom policy has its own declared finite budget. | Installed `executable_modes` matrix and EAL `method_work`; see [EXECUTABLE_ACCEPTANCE.md](EXECUTABLE_ACCEPTANCE.md) |
| Predicates ignored configured limits and object construction could exceed them | Environments, evidence checks, reasoning predicates and proposition results use their compiled programme's limits. Pure object construction admits each field against an exact streaming canonical footprint before retaining it. | EAL `predicate_limits`, `expression_construction` and canonical encoding tests |
| Performance/generalisation claims exceeded observations | A frozen finite-snapshot before/after harness records source requests, observations, files, bytes, latency and source-change inventory. Actual-provider authoring quality/cost remains a separate study. | [EAL4_MEASUREMENT.md](EAL4_MEASUREMENT.md) and retained harness output |
| Legacy commands and hidden execution dependencies remained | Current packages contain EAL, TOML and declared tools. The old graph executor, source reconstruction, per-need generation, registration commands and unused generated-provider registration helpers are removed. | Full repository checks, moved-package tests, caller audit and current CLI help |

## Reproducible repository checks

Use the repository-pinned toolchain in each checkout. EAL currently pins Rust
1.90.0; EngArg pins 1.99.1. The paired EAL source revision, EngArg dependency pin,
lockfile and installed binaries must agree. Run from the named repository.

In EAL:

```sh
cargo fmt --all --check
bash tools/regenerate-parser.sh --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --workspace --locked --lib --bins
```

Focused EAL checks isolate language, computation and acquisition contracts:

```sh
cargo test --locked -p eal-core --test executable_program --test executable_method_inputs --test method_work
cargo test --locked -p eal-process --test duplex --test process
cargo test --locked -p eal-runtime --test mcp_lifecycle --test mcp_preparation
```

In EngArg:

```sh
cargo fmt --all -- --check
python3 -m unittest discover -s tests -p 'test_*.py'
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

Focused public-contract checks:

```sh
cargo test --locked --test argument_revision --test support_cli --test support_recovery_cli
cargo test --locked --test direct_implementation --test tooling_cli --test eal_cli --test eal_export
```

The following commands require the selected EAL host and its sibling
`eal-process-supervisor`; generated tools also use `eal-tool-run`, and tickets use
`eal-coordinate`. Set the absolute paths to the actual build under test.
The generated-tool tests also require Docker and the image/toolchain
prerequisites documented in [TOOL_IMPLEMENTATION.md](TOOL_IMPLEMENTATION.md).

```sh
export ENGARG_EAL_HOST=/absolute/path/to/eal-mcp/target/release/eal-host
export ENGARG_EAL_COORDINATE=/absolute/path/to/eal-mcp/target/release/eal-coordinate
cargo test --locked --test executable_package --test executable_modes -- --ignored
cargo test --locked --test support_cli revised_public_authoring_package_executes_after_original_sources_are_removed -- --ignored
cargo test --locked --test tooling_docker -- --ignored
cargo test --locked --test ticket_native -- --ignored
```

These tests have different evidential roles. A nonignored fixture test checks
authored shape and compiler admission. An ignored installed-host test performs
acquisition and assessment. Docker generation tests add actual build, frozen
verification, reuse and tamper checks. None establishes the truth of arbitrary
external premises or the quality of an untested language-model provider.

## Breaking cutover and bounded conclusions

Regenerate current source contracts and compile EAL/4 packages. Historical EAL/3
programs, execution-selector IR and older report schemas are rejected rather than
silently reinterpreted. Keep old observations and source files unchanged for
audit. Existing observation schema versions are independent of the language
version and do not change solely because of this cutover.

EngArg's `eal prepare`, `integrate` and `register-tools` commands and JSON execution
bundle path have been removed. Use `eal implement --input ... --tools ...` for
authorised missing-operation construction, then `build` or `eal export` for the
portable package. EAL's own `prepare` command is optional interface discovery and
pinning; it acquires no evidence.

The finite inventory claim concerns the named retrieved snapshot and
case-sensitive equality. It does not establish an external catalogue's complete
population. An unbound operation, unavailable source, missing applicability basis
or absent scoped review remains unresolved. Tool success, predicate acceptance,
computational completion and claim support are checked and reported separately.
The measurement harness establishes only its declared finite comparison; broader
latency, maintainability, authoring-quality and cost claims need separate data.

## Verification record

The completion review inspected merged EngArg `9cb47030e7a5895472bafa51baa82a3136dac2ad`
and EAL `c2193348c377422e1a7814ea9fb4826c6ac62bb9`. It reproduced predicate-budget,
intermediate-construction, revised-implementation, authoring-origin and nullable
history failures before applying their repairs. The implementation rows above
include those additional cases.

Current local checks use EAL `38c364b4319d84b916669836ead78004e44dbc61`, pinned
identically in EngArg's manifest, lockfile and producer/consumer workflow. No local
Cargo patch is used. The consumer source is the revision containing this record.
The declared toolchains remain Rust 1.90.0 for EAL and 1.99.1 for EngArg.

- EAL: 696 workspace tests, warnings-denied all-target Clippy, formatting and the
  release build passed. The grammar and checked-in recogniser are unchanged.
- EngArg: 686 standard Rust tests, 46 Python tests, warnings-denied all-target
  Clippy, formatting, shell admission checks and the release build passed.
- Installed EAL: all eleven application smoke groups passed, covering all
  seventeen operations, real command/MCP acquisition and transport admission.
- Installed EngArg packages: the 80 composed mode/review cases, all nine native
  ticket topologies, negative/unavailable source cases, public repeated revision,
  relocation and implementation-pin tamper checks passed. Native collector and
  generated-interface acquisition/reuse checks also passed.
- The [retained finite-snapshot measurement](../benchmarks/eal4-20261027-final/measurement.md)
  identifies its measured executable and source revisions, request logs and
  complete observation/assessment payloads. It remains a version-bound earlier
  comparison; the later correctness repairs are established by the current
  regression and execution checks. No later timing improvement is inferred.

The standard test run reports environment-dependent tests as ignored; installed
and Docker execution are separate checks. Hosted Linux/macOS, producer/consumer,
Python/JavaScript/Rust Docker public chains and native ticket acceptance must be
checked on the exact proposed revision. Their current results are linked in the
repair PRs. Historical green runs do not establish acceptance of later code.

Reports declare either `provider_calls` or `offline_synthetic_fixture`. Retained
completions constrain the original argument and authored support. Explicit
revision, support recovery and route-only replanning have separate checked
transitions. Sparse `previous_encoding` metadata preserves representation changes
without permitting semantic changes or manufacturing a missing historical
parent. Unsigned records establish consistency rather than authenticated origin.

These software checks use synthetic inputs and scripted authoring providers.
They establish the stated executable contracts, not arbitrary external premise
truth or unmeasured model-provider quality, cost or human comprehension. Those
empirical studies remain separate from implementation completion.
