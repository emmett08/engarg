# Optional tool implementation

Tool authoring is a separate, explicit stage. An existing implementation can be
bound directly in `tools.toml`; no generation or qualification catalogue is
required. When an operation is missing, retain its input/output contract and
acceptance tests before generating or writing an implementation.

EngArg's optional implementation stage supports operator-selected toolchains
and bounded model calls. The configuration fixes the compiler or interpreter,
container image, literal build/run arguments, environment grants, network policy
and resource limits. Generated source cannot replace those execution settings.
[Python](../examples/toolchains/python.json),
[JavaScript](../examples/toolchains/javascript.json),
[Rust](../examples/toolchains/rust.json),
[Go](../examples/toolchains/go.json) and
[TypeScript](../examples/toolchains/typescript.json) toolchains are included. Their
dependencies and compilers must already be available in the selected image.

A missing acquisition with an explicit parameterised `operation_spec` can be
implemented from the canonical report. Start with operator-owned bindings, even
if the initial manifest contains only `[tools]`:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model configured-model --format json --output analysis.json \
  --eal-host /opt/eal/bin/eal-host

engarg eal implement --input analysis.json --tools tools.toml \
  --model configured-coding-model --eal-host /opt/eal/bin/eal-host \
  --tool-language python --toolchain examples/toolchains/python.json \
  --tool-store .engarg-tools --tool-output-dir implemented \
  --tool-max-calls 16 --tool-timeout-secs 3600

engarg build --input implemented/analysis.with-tools.json \
  --tools implemented/bindings/tools.toml --output-dir investigation

eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

The selected host's EAL/4 capabilities are checked before implementation model
calls. `implemented/analysis.with-tools.json` uses the same typed report contract
as authoring, revision and build. Its `tooling.tools` field identifies the merged
manifest; `tooling.operations` retains the exact tested package per acquisition.
Original bindings and their environment grants are preserved. Missing operation
semantics remain explicit gaps, and missing source access or credentials cannot
be supplied by code generation.

`analyse --implement-tools` runs this same stage after authoring when `--tools`,
`--format json`, `--output` and the toolchain/store/budget options above are
supplied. `engarg ticket` can opt into the same stage through an explicit
`implementation` configuration. Neither command uses per-need custom stubs or a
second registration workflow.

## Rust, Go and TypeScript generation

Build the trusted images once from the repository root. This is the stage that
may download compilers and dependencies; generated collectors build and run
without dependency installation or fixture network access.

```sh
docker build --tag engarg-rust-tools:1.99.1 examples/toolchains/rust
docker build --tag engarg-go-tools:1.25.14 examples/toolchains/go
docker build --tag engarg-typescript-tools:5.9.3 examples/toolchains/typescript

engarg eal toolchain-check --toolchain examples/toolchains/rust.json --output rust-check.json
engarg eal toolchain-check --toolchain examples/toolchains/go.json --output go-check.json
engarg eal toolchain-check --toolchain examples/toolchains/typescript.json --output typescript-check.json
```

Each preflight actually compiles or checks its host-owned probe, reads stdin and
a fixture, and verifies that source and fixture mounts are read-only. Inspect the
receipt's `passed` field before spending model calls. The runner resolves a local
tag to an immutable image ID and retains that identity; it never pulls an image.
For reproducible deployment, select the inspected image ID in a copy of the
operator JSON before preflight, then keep that copy with the run:

```sh
docker image inspect --format '{{.Id}}' engarg-go-tools:1.25.14
```

Use the following alternatives against the same admitted `analysis.json` and
operator `tools.toml`. Select one language per implementation attempt and a new
output directory. The model authors the frozen fixtures and source; the runner
compiles and verifies them before exporting a usable binding.

```sh
engarg eal implement --input analysis.json --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --tool-language rust --toolchain examples/toolchains/rust.json \
  --tool-store .engarg-tools/rust --tool-output-dir implemented-rust \
  --tool-max-calls 16 --tool-timeout-secs 3600 --request-timeout-secs 600

engarg eal implement --input analysis.json --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --tool-language go --toolchain examples/toolchains/go.json \
  --tool-store .engarg-tools/go --tool-output-dir implemented-go \
  --tool-max-calls 16 --tool-timeout-secs 3600 --request-timeout-secs 600

engarg eal implement --input analysis.json --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --tool-language typescript --toolchain examples/toolchains/typescript.json \
  --tool-store .engarg-tools/typescript --tool-output-dir implemented-typescript \
  --tool-max-calls 16 --tool-timeout-secs 3600 --request-timeout-secs 600
```

| Language | Entry source | Available dependencies | Build and runtime |
|---|---|---|---|
| Rust | `main.rs` | Rust standard library, pinned `serde`, `serde_json`, `reqwest` | Fixed `rustc` wrapper; `/build/collector` |
| Go | `main.go` | Precompiled pure-Go standard library | Fixed `go tool compile`/`link` wrapper; `/build/collector` |
| TypeScript | `collector.ts` | TypeScript 5.9.3, Node 24 API declarations | Strict `tsc` compile; Node executes `/build/collector.js` |

The Go wrapper does not load Go modules or compile cgo. The TypeScript image
installs the checked-in npm lockfile during its trusted build; generated source
cannot install packages. Supply a deliberately extended trusted image and
operator toolchain when an investigation needs another dependency. Runtime
network access remains `none` in these examples; a real external collector needs
an explicit operator network and credential grant. See [USAGE.md](../USAGE.md)
for the complete authoring, implementation, evidence and assessment sequence.

## Verify and retain

The measurement contract and fixtures are frozen before coding. Verification
checks positive, measured-negative and acquisition-error cases, malformed
requests, request identity, output schema and bounded execution. Original source
timestamps remain necessary for observations; a calculator's execution time
cannot refresh its inputs. Passing fixtures demonstrate the recorded cases,
not general scientific adequacy or source completeness.

Repairs retain the frozen contract, previous candidate, bounded failure
diagnostics and remaining budgets. Source/build/test defects can receive another
explicitly budgeted coding call. Provider errors, cancellation, infrastructure
failure and exhausted budgets remain failures. Retained packages and receipts
preserve the original execution identity. Changed or tampered source cannot
reuse an old passing receipt.

An infrastructure failure can be retried without model calls:

```sh
engarg eal tool-replay --attempt PATH/FAILED-ATTEMPT \
  --tool-store PATH/STORE --output replay-result.json
```

Replay keeps the original failed attempt and repeats the frozen verification
under the declared runner identity. Use a new output path and inspect the result.

## Export a direct executable binding

Export a verified package using its recorded package manifest:

```sh
engarg eal tool-export --package PATH/package.json \
  --tool-id measurement --output-dir tools/measurement
```

The new directory contains the source, pinned input/output schemas,
`verification.json`, `tool.toml`, a direct `tools.toml` binding and execution
instructions. It invokes EAL's `eal-tool-run`, which enforces the package's pinned
image, schemas, build/run arguments and resource policy. EAL executes this tool
without the EngArg authoring executable, a capability registry or a model.
Install the matching `eal-tool-run` and `eal-process-supervisor` companions,
Docker and the already verified immutable image. The runner does not pull images.

Keep the exported directory intact when deploying it. Incorporate its concrete
binding and referenced files into the investigation's operator manifest, then:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model configured-model --format json --output analysis.json
engarg build --input analysis.json --tools tools.toml --output-dir investigation
eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

Independent scoped reviews remain separate evidence obligations. Neither tool
verification nor a successful acquisition establishes the claim by itself. See
[executable packages](executable-packages.md) for package movement and runtime
identity, and [support planning](support-planning.md) for authored requirements.
