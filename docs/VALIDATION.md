# Verify the implementation at its public boundaries

[Documentation](index.md) · [Architecture](EXECUTABLE_ARCHITECTURE.md)

The CI example needs two kinds of assurance. Software checks establish that
EngArg preserves the authored investigation and EAL executes its declared
contracts. Engineering evidence establishes whether parallel execution improves
the selected pipeline while preserving outcomes. This chapter covers the first
question. [Evaluation](EVALUATION.md) describes the empirical studies needed for
the second and for claims about the authoring system itself.

## Run the repository checks

Use the Rust version in `rust-toolchain.toml` and retain `Cargo.lock`. From the
EngArg checkout:

```sh
cargo fmt --all -- --check
python3 -m unittest discover -s tests -p 'test_*.py'
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

`cargo test` runs tests concurrently by default. Give an active test run ownership
of its executable artifacts: another Cargo invocation must not rebuild those
same paths while tests are spawning the CLI. For development builds alongside
verification, use a separate target directory for the verification lane:

```sh
CARGO_TARGET_DIR=target/verification cargo test --locked --all-targets
```

Keep that directory exclusive to the active verification run. Independent CI
jobs already have separate workspaces and remain parallel. The shared installed
runtime cache is keyed by the pinned EAL source and release build configuration;
every cache hit still passes the installer's complete binary admission checks.

The default test run reports environment-dependent tests as ignored. It therefore
needs the installed checks below before it can establish acquisition, generated
code execution or native assessment. Read the result for the selected tests
rather than counting a successful command that matched no tests.

The producer/consumer workflow builds the exact EAL revision pinned in EngArg's
manifest and lockfile. Its descriptor comparisons detect disagreement at the
collector and method boundaries. EAL's own workspace checks use its separately
pinned toolchain:

```sh
cargo fmt --all --check
bash tools/regenerate-parser.sh --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --workspace --locked --lib --bins
```

Run that block from the EAL checkout. Parser regeneration checks belong to EAL,
whose normal builds use the checked-in generated recogniser. EAL workflows remain
manually triggered. EngArg's Linux/macOS and producer/consumer jobs provide the
paired integration coverage.

## Select checks by the contract that changed

| Boundary | What must remain true | EngArg checks |
|---|---|---|
| Provider transport | Bounded requests, safe failure diagnostics, explicit retry policy and retained usage. Assistant commands also preserve environment selection and descendant cleanup. | `http_provider`, `http_retries`, command-provider tests and provider unit tests. |
| Report admission and recovery | Original completions constrain the argument. Each authoring or repair stage retains its own call and byte budgets. | `support_cli`, `support_recovery_cli`, `authoring_checkpoint` unit tests. |
| Explicit revision | Parent identity, dissent and changed requirement reasons survive repeated revisions. Derived execution claims are invalidated. | `argument_revision`, `replan_cli`, installed repeated-revision check. |
| Target runtime | Selected language and method contracts are checked before model calls. | `runtime_compatibility`, `tooling_cli`, ticket configuration tests. |
| Tool generation | Frozen cases, source identity and verification receipts constrain exported implementations. | `direct_implementation`, `tooling_cli`, `tooling_docker`. |
| Portable package | Only admitted resources are copied. Context, local pins and relative paths survive movement. | `executable_package`, `eal_export`. |
| Review import | Criterion, target, scope and final operator/compiler context match the actual acquisition. | `executable_package` and composed review adversities. |
| Native reasoning | Acquired values reach the selected method and its actual result governs support. | `executable_modes`, `eal_live`, `eal_export`. |
| Coordination | Participant/round identity, routing, budgets and durable ticket stages remain distinct. | Debate tests and `ticket_native`. |
| Presentation | Views preserve the canonical graph, qualifications and resource limits. | Presentation unit tests and CLI rendering tests. |
| ASPIC snapshot export | Exact retained source, observations, context, time and method profile reproduce the authored assessment before checked native export. | `aspic_view` admission tests and installed positive, negative, changed-input, bound and no-overwrite controls. |

Focused commands help isolate a failure before the whole run:

```sh
cargo test --locked --test http_provider --test http_retries
cargo test --locked --test argument_revision --test support_cli --test support_recovery_cli
cargo test --locked --test direct_implementation --test tooling_cli --test eal_cli --test eal_export
```

EAL separately checks pure evaluation limits, method admission, observation
identity, retention, tool protocol admission and process lifetime. A transport
substitution must preserve failures and original observation age as well as the
happy-path value. The
[EAL verification guide](https://github.com/emmett08/eal-mcp/blob/main/docs/development.md#installed-acceptance)
gives its installed checks.

## Execute the paired applications

Install the pinned runtime from the EngArg checkout, then select all companions
needed by the test targets:

```sh
export ENGARG_EAL_HOST="$(bash scripts/install-eal-runtime.sh)"
export EAL_BIN="$(dirname "$ENGARG_EAL_HOST")"
export ENGARG_EAL_COORDINATE="$EAL_BIN/eal-coordinate"
export EAL_COLLECTOR="$EAL_BIN/eal-collector"
export EAL_CORE="$EAL_BIN/eal-core"
export ENGARG_EAL_CORE="$EAL_CORE"
export PATH="$EAL_BIN:$PATH"

cargo test --locked --test executable_package --test executable_modes --test aspic_view -- --ignored
cargo test --locked --test support_cli revised_public_authoring_package_executes_after_original_sources_are_removed -- --ignored
cargo test --locked --test eal_live --test eal_export --test native_collectors --test collector_catalogue_live -- --ignored
cargo test --locked --test ticket_native -- --ignored
```

The package tests move the EAL, TOML and implementation files to a fresh directory
and remove their authoring inputs before execution. This verifies the public
package contract, including exact context, resource pins and relative paths.
A retained analysis or build receipt cannot stand in for actual execution.

Generated-tool acceptance additionally needs Docker and the trusted images and
operator toolchains described in [tool implementation](TOOL_IMPLEMENTATION.md).
After preparing those prerequisites, run:

```sh
cargo test --locked --test tooling_docker -- --ignored
```

The Docker suite covers the complete Python, JavaScript, Rust, Go and TypeScript
flows, acquisition/reuse and adverse identity/resource cases. Protocol fixtures
are deliberately synthetic. Their passing result qualifies the recorded
implementation behaviour, while real source access and scientific adequacy
remain separate obligations.

## Exercise reasoning in adverse worlds

`tests/executable_modes.rs` compiles seven EngArg modes using the production
compiler, moves the package and executes the installed host. It inspects the
retained observation, completed method calculation and claim status separately.
Each mode has an explicit applicability/design record.

| Mode | Independently specified positive result | Negative result under the same criterion |
|---|---|---|
| Deductive | `p` and `p implies q` entail `q`. | Those premises have a countermodel for `r`. |
| Inductive | 90/100 successes give a 95% Wilson lower bound of `0.8256343384950865`, above `0.8`. | 50/100 fails the same `0.8` bound. |
| Abductive | Equal priors and likelihoods `0.9, 0.1` give `P(A)=0.9`. | Reversed likelihoods give `P(A)=0.1`. |
| Causal | Treatment `[7,9]` minus control `[1,3]` gives mean contrast `6`. | Reversed groups give `-6`. |
| Counterfactual | `X=1`, `Y=2X+1`, with intervention `X=4`, gives difference `6`. | Intervention `X=1` gives difference `0`. |
| Analogical | Both selected interface/load features match, and the separate transfer review supports applicability. | A relevant interface mismatch gives match fraction `0.5`. |
| Practical | Under cost cap £10, the declared minimum-duration policy selects B and orders feasible options `[B,A]`. | Raising B's cost to £20 makes the same policy select A. |

Each mode runs in nine worlds: positive, measured negative, unavailable source,
stale observation, inapplicable design, material assumption challenge, resolved
challenge, unavailable challenge audit and exhausted calculation. The complete
matrix has 63 mode/world combinations. Seventeen additional review cases cover
rejected, inconclusive, wrong-scope, absent or blank reviewer, missing or stale
review time, malformed reasons and a material disanalogy.

For the six native computational methods, `method_work=1` produces an actual
incomplete method result with empty details. The practical fixture has its own
explicit one-unit-per-option policy budget. Its exhausted result retains the
performed and required work without claiming a choice. Thus successful
acquisition, completed computation and supported conclusion remain separate.
A purely structured judgement has no solver-work claim to test.

The nonignored mode test validates the 63 fixtures. Only the installed suite
executes all 80 cases. Other suites cover transport substitutions, exact numbers,
local source tampering and external worker resources. This finite matrix makes
no claim of universal coverage for arbitrary installed methods.

## State what the verification establishes

The checks above constrain current software contracts. They preserve an
unsupported result when evidence is absent, adverse, stale or inapplicable.
They do not establish the truth of an external premise, independence of a
reviewer, correctness of a natural-language formalisation or quality of an
untested provider's arguments.

Use the checks on the proposed revision when reviewing a change. Earlier green
runs and historical benchmark records retain their original revision scope.
The next empirical step is the [evaluation design](EVALUATION.md), followed by a
frozen [complete-workflow study](support-study.md) when the intended comparison
includes tool construction and evidence acquisition.
