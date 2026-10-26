# Optional collector implementation

EngArg can generate and test missing collector implementations after argument planning.
The stage is opt-in. Existing collectors and verified cached packages remain reusable.

The [support-planning guide](support-planning.md) describes the graph mode variant:
parameterised operation contracts, separate consumer bindings, `tooling.capabilities`,
and function-test receipts that leave scientific qualification separate. This
document's per-need overrides describe the compatible flat planning path.

The graph capability path repairs malformed source, build failures and failed
frozen tests within its remaining `--tool-max-calls` allowance and stage deadline.
Each retry keeps the same frozen operation, schemas and fixture oracle. It receives
the previous candidate and bounded failure diagnostics, with configured secret
values removed. Failed attempts remain retained. Provider, infrastructure,
cancellation and exhausted-budget errors stop the stage rather than triggering
blind retries. A later invocation can still reuse the frozen contract.
With `--progress`, the next repair and its remaining allowance are reported
before another provider request.

## Prepare a language toolchain

The language switch names the implementation language.
An operator-owned profile selects the compiler or interpreter, image, literal build and run arguments, and resource limits.
The profile language must match the switch.
The model supplies source files and cannot select these execution settings.

The examples use Docker images which must already be installed:

```sh
docker pull python:3.13-slim
docker pull node:24-slim
docker build --tag engarg-rust-tools:1.99.1 examples/toolchains/rust
```

[Python](../examples/toolchains/python.json) and
[JavaScript](../examples/toolchains/javascript.json) and
[Rust](../examples/toolchains/rust.json) profiles are included.
The Rust image supplies pinned Serde, arbitrary-precision/raw-value JSON and blocking
Reqwest libraries. Its fixed compiler wrapper builds `/src/main.rs` offline
against those libraries and writes `/build/collector`.
Other languages use the same profile contract with an appropriate preloaded image and compiler commands.
Compiled outputs and compiler caches belong under `/build`.
Dependencies needed by an offline build must already be available in the image.

The [catalog sync authoring script](CATALOG_SYNC_AUTHORING.md) enables this Rust
profile with `--implement-tools`. It prepares the default image once and runs a
trusted toolchain probe before authoring. The probe uses the actual restricted
container execution path and temporary source and fixture mounts. Its receipt is
retained as `toolchain-preflight.json`. Operator-supplied profiles require an
already installed image. The switch also selects a persistent verified tool
store for reuse across runs. Graph configuration-only plans consume no coding
calls after this early execution check.

Check a prepared toolchain without a model or API key:

```sh
engarg eal toolchain-check --toolchain examples/toolchains/rust.json \
  --output toolchain-preflight.json
```

The stage resolves and records the immutable image ID before generation.
It also records the Docker executable, EngArg runner and supervisor digests.
Changes invalidate the corresponding verified reuse identity.

Mandatory metadata probes include huge numbers, escaped strings and literal
objects with Serde's private Number/RawValue key spellings. Those keys remain
ordinary data. Rust collectors should stream opaque input/context through
`serde_json::value::RawValue` and typed transport structs, avoiding a `Value`
intermediate that can reinterpret those literal objects. Test receipts use
`engarg.tool-tests/2`; older receipts cannot authorize registration or reuse.

## Generate an argument and its missing tools

```sh
engarg analyse --input request.json --model "$ENGARG_MODEL" \
  --api-key-env OPENAI_API_KEY \
  --implement-tools --tool-language python \
  --toolchain examples/toolchains/python.json \
  --tool-store .engarg-local/tool-store \
  --tool-output-dir .engarg-local/tool-runs/attempt-001 \
  --tool-max-calls 16 --tool-timeout-secs 3600 \
  --format json --output .engarg-local/analysis-with-tools.json
```

Omitting `--implement-tools` preserves the argument-only flow.
Implementation options require that switch during `analyse`.
The toolchain, persistent store and a new output directory are explicit.
The complete argument is retained before the first coding call.
A coding or verification failure preserves the argument, stage audit and any candidate or test results already produced.

The tooling budget is separate from the argument's `--max-calls`.
A new collector normally uses one contract/fixture call followed by one coding call.
A retry can reuse frozen fixtures.
A qualifying verified package consumes no coding calls.
The stage checks the required budget before starting its coding calls.

## Work from a saved analysis

Argument generation can be skipped when an analysis is already available:

```sh
engarg eal implement --input .engarg-local/analysis.json \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --tool-language javascript \
  --toolchain examples/toolchains/javascript.json \
  --tool-store .engarg-local/tool-store \
  --tool-output-dir .engarg-local/tool-runs/attempt-002
```

The dedicated command enables the implementation stage.
It writes `analysis.with-tools.json` in the new output directory.
The original canonical argument and argument transcript are preserved.
Tooling has its own metadata and audit.

This command's `--model`, `--reasoning-effort` and `--max-output-tokens` apply
only to tool generation. The graph authoring script exposes the same separation
through `ENGARG_TOOL_MODEL`, `ENGARG_TOOL_REASONING_EFFORT` and
`ENGARG_TOOL_MAX_OUTPUT_TOKENS`. Unset values inherit authoring settings when
the model is unchanged. A different model requires its own supported token
allowance. These options change generated responses while the toolchain profile
continues to determine isolated execution limits.

Infrastructure failures retain an `execution-failure.json` receipt beside the
failed source and contract. The receipt identifies the failed attempt and
records bounded redacted execution diagnostics. Correct the local execution
problem, then replay the retained candidate without model calls:

```sh
target/release/engarg eal tool-replay \
  --attempt .engarg-local/tool-store/attempts/FAILED-ATTEMPT \
  --tool-store .engarg-local/tool-store --output replay-result.json
```

Replay checks the retained files and runs the same frozen fixtures under the
current runner identity, preserving the original profile and immutable image.
Use the same release binary and sibling supervisor that the authoring script
will use so the executable identities match on reuse. The aggregate replay
deadline defaults to 3600 seconds and can be set with `--tool-timeout-secs`.
Replay preserves the original failed attempt. A passing
replay commits a verified package to the store. The authoring script can then
reuse the completed argument and that package:

```sh
bash scripts/run-catalog-sync-authoring.sh --simple-example \
  --replan-analysis engarg-runs/PREVIOUS-RUN/analysis.original.json
```

Verified package reuse spends no coding calls when the contract and runner
identity still match. Changed contracts or execution identities require fresh
verification or generation.

A later invocation can use the same store with a new output directory.
Reuse checks the exact contract, supplied request, language, source, fixtures and runner identity.
Changed or tampered packages cannot qualify through an old test receipt.
Known collector plans bypass code generation.
The stage writes the combined verified catalogue to `collectors.json` in its output directory.
Select that file through `--collector-catalogue` in later argument runs.

## Contract and verification gates

The contract and fixtures are frozen before the coding call.
They retain the evidence need's measurement, comparator and acceptance criterion.
The coding model receives the selected language, operator toolchain interface and the frozen contract.
Expected fixture values remain outside its coding prompt.

The stage executes the implementation and compares observed results with the recorded expectations.
It requires positive, measured-negative and acquisition-error cases.
Independent protocol probes exercise malformed input, missing identity and invalid context.
Successful observations must preserve the complete request and context, including large integers and escaped strings.
When the retained operation contract requires original observation time, an outer
`observed_at` UTC timestamp is mandatory during frozen verification. A fresh local
calculation records its own observation time. Upstream measurement clocks remain
separately preserved through the dependency graph. A generated tool cannot pass
verification by omitting a timestamp that its eventual native binding requires.
A measured negative remains a successful acquisition with its negative value.
An acquisition error produces no successful observation.

Test receipts bind the observed exits and output digests to the contract, source, fixture set and runner.
Registration is constructed only after every required check passes.
The registration path rechecks retained files and receipts.
An edited implementation requires fresh verification.

Each retained package exposes its `contract.json`, including the frozen fixtures,
the implementation files under `source/`, and the observed `test-results.json`.
Failed attempts retain the available review artefacts without adding a catalogue entry.

Passing these tests establishes conformance to the frozen fixtures.
Applicability, measurement truth and inferential sufficiency retain their separate review obligations.

## Isolated execution

Fixture containers use a read-only root filesystem and read-only source and fixture mounts.
EngArg streams bounded source and fixture archives into two fresh Docker-managed
volumes through a stopped staging container. The worker receives those volumes
read-only. This transport works with Docker Desktop and remote daemons without
sharing the client's temporary directories. The staging container never runs
generated code and receives no runtime credentials. Ownership-labelled staging
containers and their anonymous volumes are removed after execution or failure.
Fixture workers run as an unprivileged user with dropped capabilities, no new privileges, bounded processes, memory and CPU, and bounded temporary filesystems.
Fixture networking is disabled and production credentials are not forwarded.
The supervisor bounds elapsed time and captured output.
Explicit container cleanup also covers daemon-owned execution after timeout or cancellation.
Snapshot creation, upload and execution share the profile's execution deadline
with a two-second supervisor allowance. Cleanup has a separate bounded allowance.

Automatic preflight supplies one trusted source file for the selected language.
The bundled profiles and direct compiler or interpreter profiles are supported.
It uses the literal `/src/` source entry in the configured commands, or the
language's canonical filename for a wrapper command. Profiles that require extra
build manifests or several source entry points need a compatible direct compiler
profile before generation. A failed probe retains its receipt and stops the run.

Live collection is a later explicit action.
Its network setting and forwarded environment names come from the operator profile.
The default network setting is `none`.
Collector code is rebuilt and run from the verified source snapshot using the recorded image.
No live Kubernetes, Grafana or other service read is implied by a fixture test.

## Export matching EAL and registry files

Export the analysis carrying verified tooling metadata:

```sh
engarg eal export \
  --input .engarg-local/tool-runs/attempt-002/analysis.with-tools.json \
  --output .engarg-local/argument.eal \
  --tools-output .engarg-local/argument.tools.toml \
  --manifest .engarg-local/argument.bundle.json
```

The exporter validates the tooling receipt and builds matching source, context and registry bindings together.
Preparation applies the same verified registration path.
Generated registry commands invoke the trusted EngArg launcher with the verified package.
They grant the execution settings recorded in the operator profile.
Independent review tooling remains separately configured.

The implementation updates this pipeline.
It does not repair previously generated TOML or HTML files.
