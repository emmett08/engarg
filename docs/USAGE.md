# From a claim to an executable investigation

[Documentation](index.md) · [How the argument works](DOMAIN_MODEL.md)

Consider a proposed CI change: run independent test groups in parallel to shorten
the pipeline while preserving their outcomes. A plausible timing argument gives
us a reason to investigate. To justify the change, we also need observations of
the specified workload and evidence that concurrent groups do not interfere.

The workflow below follows that distinction through four stages. `engarg analyse`
authors the argument and its evidence obligations. You bind those obligations to
actual tools and build an executable package. `eal-host run` then executes the EAL
through `eal-mcp`, acquires observations and assesses the claim against its declared
criteria. The final assessment can justify the bounded claim or explain what
prevents support.

The commands below use the public CLIs directly. No catalogue, tool registration
or catalogue-authoring wrapper is required. Run them in Bash on Linux or macOS.
Use new output paths for another attempt. Reports, audits, checkpoints and
package directories preserve the earlier investigation.

For a single entry point that retains each stage's commands, logs and results,
use [the claim-assessment script](assess-any-claim.md). It runs these same public
CLIs, preserves missing evidence and unresolved reviews, and reports the resulting
claim status separately from successful tool execution.

## 1. Install the paired applications

From an EngArg checkout, with Git, Python 3, jq, a C compiler and rustup installed:

```sh
cargo install --path . --locked
export ENGARG_REPO="$(pwd -P)"
export EAL_HOST="$(bash scripts/install-eal-runtime.sh)"
export ENGARG_EAL_HOST="$EAL_HOST"
export EAL_BIN="$(dirname "$EAL_HOST")"
export PATH="$EAL_BIN:$PATH"

engarg --version
engarg eal describe --eal-host "$EAL_HOST" --workspace "$ENGARG_REPO" \
  --output runtime-description.json
```

The installation helper builds the exact EAL revision pinned by this EngArg
checkout. It installs the host, MCP server, method worker, process supervisor,
collector and generated-tool runner together. Reuse the printed absolute host
path and retain its companion directory. Copying only `eal-host` leaves required
companions missing. The helper is
an installation step and performs no argument authoring or evidence collection.
The repositories declare their Rust toolchains in `rust-toolchain.toml`.

`EAL_HOST` selects the runtime checked before authoring and tool generation.
`ENGARG_EAL_HOST` supplies the default for `engarg eal collect`, `assess` and
other runtime commands. Every example also supplies `--eal-host` explicitly.
The default host starts its sibling `eal-mcp` over stdio for each invocation.
That connection is established automatically within the selected installation.

## 2. State the question and available sources

Choose an exact model supported by your provider. The commands below use the
default HTTP provider and OpenAI Responses protocol. Set `OPENAI_API_KEY` through
your secret manager or shell environment; the commands pass the variable's name,
never its secret value. For a compatible Chat Completions service, add
`--api chat --endpoint https://YOUR_HOST/v1/chat/completions`.

To use an authenticated local assistant instead, select `--provider claude`,
`codex`, `copilot`, `gemini`, `cursor` or `augment` and omit
`--api-key-env OPENAI_API_KEY` from the authoring command. The
[assistant guide](AI_ASSISTANTS.md) covers the supported CLI profiles,
environment selection, recovery and custom adapters for other assistants.

Create a new investigation directory outside the source checkout:

```sh
cd "$ENGARG_REPO/.."
mkdir ci-investigation-001
cd ci-investigation-001
```

A claim alone is accepted, but a bounded question gives the author and evidence
tools a testable target. For the CI example, create `request.json` and replace
the repository, revision and measurement scope with the actual intended case:

```json
{
  "claim": "For the identified repository revision and Linux runner configuration, parallelising the specified independent CI test groups reduces median pipeline duration by at least 20%, with the same test identities and outcomes in the paired runs.",
  "context": "Identify the repository, commit SHA, workflow version, runner size, test groups and measurement interval before collecting data. Compare sequential and parallel configurations on the same workload using repeated paired runs; retain raw run IDs, timing, outcomes, failures and exclusions. Check fixture isolation, shared resources and scheduling effects rather than assuming independence. Define the estimand, uncertainty calculation and acceptance threshold explicitly. Existing production data, source credentials and independent reviews have not yet been supplied. Missing information must remain an evidence obligation. Where an acquisition can be implemented, supply a parameterised operation_spec with input/output contracts and positive, measured-negative and acquisition-error cases. Do not generate observations or accepted reviews.",
  "evidence": []
}
```

Supplied evidence excerpts help author the argument. Fetching their source URLs
requires an explicit acquisition. Source access, measurement design and reviewer judgement
still need concrete evidence. A successful argument about this finite experiment
would not establish that parallelisation improves every CI pipeline.

Create an operator manifest. An empty manifest permits authoring while making
missing operations explicit:

```sh
printf '[tools]\n' > tools.toml
export ENGARG_MODEL='YOUR_EXACT_MODEL_ID'
```

If tools already exist, bind them in this manifest before authoring so their
names, versions and supplied interface schemas inform the plan. For example,
after the report identifies an acquisition named `ci_runs`, its command binding
could be:

```toml
[tools.ci_runs]
kind = "command"
version = "1"
argv = ["python3", "tools/read_ci_runs.py"]
inherit_env = ["PATH", "CI_READ_TOKEN"]
timeout_seconds = 120
max_output_bytes = 1048576
```

Use the actual acquisition ID, source adapter and permissions. This fragment is
configuration, not an implementation of `read_ci_runs.py`. A command reads the
EAL acquisition request on stdin and emits an observation envelope such as
`{"value": {...}, "observed_at": "ORIGINAL_UTC_MEASUREMENT_TIME"}` on stdout.
The request contains `evidence_id`, `environment`, `tool`, `tool_version`,
`input` and `context`. Keep diagnostics on stderr. Preserve original source
timestamps for historical runs so their age remains visible when they are read
again.

Direct MCP tools also work. For an existing server, use the following shape,
with the actual acquisition ID and operation name:

```toml
[tools.ci_runs]
kind = "mcp_stdio"
version = "1"
argv = ["python3", "tools/ci_server.py"]
inherit_env = ["PATH", "CI_READ_TOKEN"]
timeout_seconds = 120
max_output_bytes = 1048576

[tools.ci_runs.direct]
tool = "read_ci_runs"
response_format = "structured"
value_pointer = "/value"
observed_at_pointer = "/observed_at"
```

Commands and MCP stdio servers inherit only manifest-selected environment
variables. The authoring model sees operation contracts, not command lines or
credential values. Relative paths resolve from the manifest's directory. See
[private tools](PRIVATE_COLLECTORS.md) for the transport boundary.

## 3. Author and inspect the argument

```sh
engarg analyse --input request.json --tools tools.toml \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --eal-host "$EAL_HOST" --format json \
  --request-timeout-secs 600 --timeout-secs 2400 \
  --max-calls 7 --max-repairs 1 \
  --audit analysis.audit.jsonl --checkpoint analysis.checkpoint.json \
  --progress --output analysis.json

engarg render --input analysis.json --format markdown --output argument.md
jq '{bindings: .direct_bindings,
     requirements: .support_elaboration.requirements,
     acquisitions: .support_elaboration.acquisitions,
     design_review: .design_review}' analysis.json
export AUTHORING_REPORT="$PWD/analysis.json"
```

Hybrid authoring normally uses six logical model calls. The command reserves
one additional call for structurally invalid final output. It authorises a
ten-minute deadline per logical call and a forty-minute overall deadline. Those are
maximum waits, not expected durations or a claim that a slow model will finish.
`--mode single --max-calls 1 --max-repairs 0` selects a one-call authoring baseline.

Inspect the argument and support requirements before executing anything. Check
that the source scope, thresholds, units, comparisons, inference method,
applicability assumptions and challenges match the engineering question.
`direct_bindings.unbound_operations` lists acquisitions without implementations.
An empty list does not mean that evidence or required reviews have been supplied.

### If authoring times out

The default per-call deadline is 120 seconds and the overall deadline is
480 seconds. A provider timeout is separate from an EAL host or
collector timeout. Raising `--timeout-secs` alone does not raise the per-call
deadline. Use both explicit deadline flags and `--progress` as above.
Connection establishment also has a separate 15-second limit. Increasing
`--request-timeout-secs` does not raise it. A `connection_timeout` calls for
checking network reachability and proxy configuration, rather than assuming
the model needs more generation time.

A timeout can leave completed exchanges in the audit and checkpoint without a
published `analysis.json`. Inspect the checkpoint before spending on another run:

```sh
engarg authoring inspect --checkpoint analysis.checkpoint.json
```

If inspection reports `resumable: true`, resume final validation using new paths:

```sh
engarg authoring resume --checkpoint analysis.checkpoint.json \
  --tools tools.toml --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --eal-host "$EAL_HOST" --request-timeout-secs 600 --timeout-secs 2400 \
  --max-repairs 1 --progress --audit resumed.audit.jsonl \
  --next-checkpoint resumed.checkpoint.json --output resumed-analysis.json
export AUTHORING_REPORT="$PWD/resumed-analysis.json"
```

The tools configuration must match the original authoring bytes. Recovery can
finish or repair a retained final candidate once the primary authoring stages
are present. An interrupted specialist call leaves that prerequisite unmet.
If `resumable` is false, use a fresh authoring attempt with new output, audit and
checkpoint paths. A timed-out
request is not automatically retried: the provider may have processed or billed
it even when no response arrived.

### If a structured response is rejected

An HTTP 200 response means that the provider returned an envelope. EngArg must
still check that it completed successfully and contains the selected API's
structured output. Refusal, incomplete generation, invalid JSON and malformed
envelope metadata require different actions.

The audit retains a `response_rejected` receipt with the authoring role, HTTP
attempt, status and a bounded `response_failure` record. Inspect it alongside the
terminal failure:

```sh
jq 'select(.event == "failure" or
           (.event == "http_attempt" and
            (.record.kind == "response_rejected" or
             .record.kind == "invalid_response_json")))' \
  analysis.audit.jsonl
```

The record contains safe `kind` and `reason` labels, provider status or Chat
finish reason, reported input/output/reasoning tokens, the configured output-token
cap and a valid bounded response ID when supplied. It excludes generated text,
refusal text and remote error messages. Missing provider usage remains unknown.

| Retained finding | What to investigate next |
|---|---|
| `incomplete` with reason `max_output_tokens`, or Chat `length` | Inspect reported usage, input size and the model's context window. An output or context limit interrupted generation. A larger output budget helps only when the context still has room. |
| `refusal` or `content_filter` | Review the request and the provider's stated policy. A longer deadline does not address this response. |
| `invalid_response_json` or `malformed` | Check that the endpoint supports the selected Responses or Chat protocol and the requested structured-output contract. |
| Another non-completed status or an unknown reason | Retain the safe receipt and investigate the provider result before changing budgets. |

The older message `refused, incomplete or malformed structured response` combined
these cases and did not retain the rejected envelope's diagnostic metadata. An
audit showing three completed specialists followed by a synthesiser HTTP 200 and
that message establishes the failure stage, but cannot identify which case
occurred. A 10,000-token allowance makes exhaustion a plausible explanation, not
a demonstrated cause.

For reasoning models, the provider's output allowance includes reasoning as well
as visible output. See the [OpenAI reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
for that contract. The default 10,000-token cap, 131,072-byte retained-content
limit and time deadlines are separate bounds. Increasing time alone cannot fix
a token limit. A larger response may also need an explicitly larger byte allowance.

If the receipt and available context identify an insufficient output allowance,
the following is a new, larger-budget attempt using the same request and tools.
It authorises up to 32,768 output tokens
per call and a 1 MiB response, so consider the additional generation cost before
running it. Use new paths again for any later attempt:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --eal-host "$EAL_HOST" --format json \
  --max-output-tokens 32768 --max-response-bytes 1048576 \
  --request-timeout-secs 600 --timeout-secs 2400 \
  --max-calls 7 --max-repairs 1 --progress \
  --audit analysis-002.audit.jsonl --checkpoint analysis-002.checkpoint.json \
  --output analysis-002.json
export AUTHORING_REPORT="$PWD/analysis-002.json"
```

Refusal and truncation do not enter the structural repair loop or trigger an
automatic retry. `--max-repairs` applies to invalid final model content after a
completed response. A checkpoint ending before a completed synthesiser response
cannot resume final validation because its primary stages are incomplete.

## 4. Supply or generate the missing tools

Bind an existing collector directly when it supplies the required acquisition.
Use code generation when an operation needs an implementation.
Generation requires a complete authored `operation_spec`, source access and a
testable input/output contract. It cannot create missing production measurements,
credentials, historical observations or independent review judgements.

EngArg freezes verification cases before coding, then builds and tests the
candidate in the operator-selected container. Install Docker, prepare the trusted
images and run the appropriate toolchain check before any coding calls. The
full prerequisites are in [tool implementation](TOOL_IMPLEMENTATION.md).

Build the selected trusted image before checking it:

```sh
docker build --tag engarg-rust-tools:1.99.1 "$ENGARG_REPO/examples/toolchains/rust"
docker build --tag engarg-go-tools:1.25.14 "$ENGARG_REPO/examples/toolchains/go"
docker build --tag engarg-typescript-tools:5.9.3 "$ENGARG_REPO/examples/toolchains/typescript"
```

The samples default to no runtime network or inherited credentials. Copy the
selected configuration into the investigation before customising its authority:

```sh
cp "$ENGARG_REPO/examples/toolchains/rust.json" operator-rust.json
cp "$ENGARG_REPO/examples/toolchains/go.json" operator-go.json
cp "$ENGARG_REPO/examples/toolchains/typescript.json" operator-typescript.json
```

For a tool that must read a permitted remote CI service, the following example
creates new network-enabled configurations and grants the named source token.
Set `CI_READ_TOKEN` through your secret manager before verification and execution.
Use this alternative only when the operation requires that authority:

```sh
jq '.runtime_network = "bridge" | .inherit_env = ["CI_READ_TOKEN"]' \
  operator-rust.json > ci-rust.json
jq '.runtime_network = "bridge" | .inherit_env = ["CI_READ_TOKEN"]' \
  operator-go.json > ci-go.json
jq '.runtime_network = "bridge" | .inherit_env = ["CI_READ_TOKEN"]' \
  operator-typescript.json > ci-typescript.json
```

Docker `bridge` permits network access without restricting it to selected hosts. Keep source
selection and access restrictions in the tool's contract and deployment policy.
For retained-data calculations, leave the network-disabled operator copies in
place. The runner resolves the selected image to its immutable local identity
and requires that image to exist locally before execution.

Choose **one** language for an implementation attempt. These commands generate
the same acquisition contracts in different implementation languages:

```sh
export ENGARG_CODING_MODEL="$ENGARG_MODEL"  # Or another exact supported model.
export RUST_TOOLCHAIN="$PWD/operator-rust.json"  # Use "$PWD/ci-rust.json" for the remote-source alternative.
export GO_TOOLCHAIN="$PWD/operator-go.json"      # Or "$PWD/ci-go.json".
export TYPESCRIPT_TOOLCHAIN="$PWD/operator-typescript.json"  # Or "$PWD/ci-typescript.json".

# Rust: main.rs, using the repository's prepared dependency image.
engarg eal toolchain-check --toolchain "$RUST_TOOLCHAIN" \
  --output rust-toolchain-check.json
engarg eal implement --input "$AUTHORING_REPORT" --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --tool-language rust --toolchain "$RUST_TOOLCHAIN" \
  --tool-store tool-store --tool-output-dir implemented-rust \
  --tool-max-calls 16 --tool-timeout-secs 3600 \
  --request-timeout-secs 600 --progress

# Go: main.go, using the prepared standard library and evidence packages.
engarg eal toolchain-check --toolchain "$GO_TOOLCHAIN" \
  --output go-toolchain-check.json
engarg eal implement --input "$AUTHORING_REPORT" --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --tool-language go --toolchain "$GO_TOOLCHAIN" \
  --tool-store tool-store --tool-output-dir implemented-go \
  --tool-max-calls 16 --tool-timeout-secs 3600 \
  --request-timeout-secs 600 --progress

# TypeScript: collector.ts, compiled with the prepared TypeScript/Node image.
engarg eal toolchain-check --toolchain "$TYPESCRIPT_TOOLCHAIN" \
  --output typescript-toolchain-check.json
engarg eal implement --input "$AUTHORING_REPORT" --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --tool-language typescript --toolchain "$TYPESCRIPT_TOOLCHAIN" \
  --tool-store tool-store --tool-output-dir implemented-typescript \
  --tool-max-calls 16 --tool-timeout-secs 3600 \
  --request-timeout-secs 600 --progress
```

The stage retains source, frozen fixtures, build/test results and passing
verification receipts. Its new `analysis.with-tools.json` is the canonical report
for the next stage. Its `bindings/tools.toml` merges original and generated bindings.
Generated packages run through the paired `eal-tool-run`, which must remain
installed with the host and supervisor. Passing fixtures establishes the recorded cases, not
the empirical truth of a CI performance claim. A failed implementation remains a
failed attempt with diagnostics, not a usable binding.

Set the selected inputs for the remaining commands. This example selects the Rust
implementation:

```sh
export ANALYSIS="$PWD/implemented-rust/analysis.with-tools.json"
export TOOLS="$PWD/implemented-rust/bindings/tools.toml"
```

For existing, manually supplied tools instead, set:

```sh
export ANALYSIS="$AUTHORING_REPORT"
export TOOLS="$PWD/tools.toml"
```

## 5. Build and fulfil scoped reviews

```sh
engarg build --input "$ANALYSIS" --tools "$TOOLS" --output-dir investigation
jq '{claims, pending_reviews, limitations}' investigation/build.json
```

The package contains `case.eal`, `tools.toml` and referenced local resources.
`build.json` is an inspection receipt, not a hidden execution input. The build
fails when an acquisition has no concrete binding. A successful build can still
contain unanswered review obligations.

For any `pending_reviews`, obtain genuine reviews of the exact criterion, target
and scope. The request already includes the export context needed for identity
checks. The following command prepares a document for reviewers. Its null values
remain unusable until the reviewers supply their decisions:

```sh
export TOOL_ROOT="$(dirname "$TOOLS")"
mkdir -p "$TOOL_ROOT/reviews"
jq '{schema: "eal.review-inputs/2", reviews:
     [.pending_reviews[] | {request: {tool, tool_version, input, context}, value: null}]}' \
  investigation/build.json > "$TOOL_ROOT/reviews/review-inputs.json"
```

Each `value` must repeat the request's `criterion`, `target` and `scope` and add
the actual `reviewer`, `verdict`, nonempty `reasons` and original UTC
`reviewed_at`. Verdicts are `supported`, `rejected` or `inconclusive`. Review the
recorded evidence and assumptions before deciding. The verdict must express that
review rather than an attempt to unblock execution. An unavailable measurement cannot be replaced by
an affirmative review.

For example, a completed `value` has this shape. Copy `criterion`, `target` and
the complete JSON `scope` from that request, then replace the illustrative content
with the actual review. `reviewed_at` is when the reviewer made the judgement,
not when this file was imported:

```json
{
  "criterion": "EXACT request.input.criterion",
  "target": "EXACT request.input.target",
  "scope": {"EXACT": "request.input.scope"},
  "reviewer": "Identified reviewer",
  "verdict": "inconclusive",
  "reasons": ["The retained runs do not yet establish fixture isolation."],
  "reviewed_at": "2026-10-27T12:00:00Z"
}
```

A request with missing scope or an unavailable measurement needs its stated
observation or an explicit support-contract revision. An affirmative review alone
cannot resolve that gap.

The [completed Python qualification example](../examples/assess-claim/python-dependencies/qualified/README.md)
shows that handoff: explicit prior-digest revisions, actual native artifact checks,
scoped implementation and challenge assessments, and a bound native deduction.
It records a supported metadata claim and adverse controls without changing its
original empirical predicates. AI review identity and independence limits remain
explicit. A source can still be unsupported after successful tool generation.

When an observation fails, inspect its retained primary execution error before
the secondary digest, status and JSON-value diagnostics. For example, a missing
`engarg_review_contract` binding produces no usable review observation. Adding
that binding alone cannot discharge requirements still authored as `unavailable`
or a warrant with an unresolved reasoning method; revise those contracts
explicitly, then obtain the exact evidence and scoped assessments.

Once the records are complete, add the importer binding to the selected manifest
and build a new package. Copy the manifest beside the original first, preserving
relative paths and the original tool-generation receipt:

```sh
cp "$TOOLS" "$TOOL_ROOT/reviewed-tools.toml"
export TOOLS="$TOOL_ROOT/reviewed-tools.toml"
cat >> "$TOOLS" <<EOF

[tools.engarg_review_contract]
kind = "command"
version = "1"
argv = ["$EAL_BIN/eal-collector", "review-json", "--inputs", "reviews/review-inputs.json"]
inherit_env = []
timeout_seconds = 30
max_output_bytes = 1048576
EOF

engarg build --input "$ANALYSIS" --tools "$TOOLS" --output-dir reviewed-investigation
```

If the manifest already binds `engarg_review_contract`, update that operator
binding in the copied manifest. TOML requires each table to be unique. If there are no
reviews, use the original `investigation` package. Missing, rejected,
inconclusive, stale or mismatched reviews remain visible in the assessment.
The importer checks correspondence and age. Reviewer identity and independence
require their own assurance.

## 6. Execute the EAL, collect and assess

Select the completed package and obtain its emitted name for `current_claim`:

```sh
export PACKAGE="$PWD/reviewed-investigation"  # Or "$PWD/investigation".
export CLAIM="$(jq -er '.claims.current_claim' "$PACKAGE/build.json")"

"$EAL_HOST" run "$PACKAGE/case.eal" --tools "$PACKAGE/tools.toml" \
  --workspace "$PWD/run-001" --claim "$CLAIM" --timeout 3600 \
  > execution.json

jq '{is_error, error, collection_id: .result.collection.collection_id,
     assessment: (.result.assessment | {assessment_id, valid, complete, claims})}' \
  execution.json
```

This is execution through `eal-mcp`: the host initializes MCP, discovers the
operation schema and invokes `execute`. The EAL program selects evidence
dependencies. TOML grants the concrete command/MCP/file/HTTP authority. Collection
retains original observation clocks and request identities in the run store,
and assessment applies the evidence predicates and reasoning methods. Keep the
run directory and `execution.json` together with the source package.

Set source credentials in the environment before running. A binding can access
only the names granted in its `inherit_env`. This direct host command uses the
calling environment. The EngArg runtime wrappers below add a second explicit
forwarding boundary through `--collector-env`.

The host's `--timeout 3600` bounds the whole MCP exchange. It does not raise a
tool's `timeout_seconds` or a generated package's pinned execution limit. Set
those operator limits before tool verification and package building when the
source operation requires more time.

Host exit status zero means the operation returned successfully. Inspect the
assessment too: `valid: true` and `complete: true` do not mean the claim is
supported. Claim `unsupported` is not a finding that the claim is false. Examine
unavailable evidence, failed predicates, rejected premises and objections to
understand a non-supporting result. A measured negative is useful evidence.
Source failure and missing observations leave different questions unresolved.

### Inspect an ASPIC+ snapshot

The generated `case.eal` contains EAL/4. To inspect its finite ASPIC+ translation
in the [companion visualiser](https://github.com/emmett08/aspic_visualisation),
export a separate JSON snapshot from the complete retained execution:

```sh
engarg eal export-aspic --input "$PACKAGE/case.eal" --execution execution.json \
  --eal-core "$EAL_BIN/eal-core" --output aspic-view.json
```

Import `aspic-view.json` in the visualiser. EAL source and ordinary assessment
JSON are not viewer inputs. The wrapper uses the paired native core's
`compile-aspic` and `export-aspic` operations. It reuses the exact collection
records, context, assessment time and executed claim from `execution.json`;
it makes no model calls and collects no evidence. `--claim` can select another
exact EAL claim name and is required when the original run assessed all claims
without selecting a goal. The output is published only after successful checked
export and never replaces an existing file.

For a [grounded argument investigation](argument-investigation.md), add
`--verification candidate/verification.json` to inspect the candidate's
`engarg.assurance-receipt/2` reports beside the graph. The receipt must match the
execution's source, executable, collection and assessment identities and record
completed normalization and native replay. The viewer labels these supplied
reports as EngArg assurance, separately from native method reports. Completed
verification can still contain failed or unknown obligations.

The retained assessment must be valid and complete. A complete unsupported
claim or measured-negative result can still be exported. The wrapper checks
that native reassessment reproduces the retained assessment, including source,
availability, tool-binding digests and method results. The default method
profile is `builtin`, as in the host command above. If the original run selected
`engineering`, `aspic` or `all`, supply the same `--methods` value. Select the
paired core explicitly or set `ENGARG_EAL_CORE`; the installer includes it beside
`eal-host`.

The wrapper supports self-contained packages generated by EngArg. EAL sources
with imports need the native EAL application and their original source
workspace. Snapshot provenance is supplied data: this offline export does not
authenticate observations or recheck a current operator registry. The visualiser
keeps authored claim status separate from formal extension results, and imports
stay in the browser.

Formal resource limits can be smaller than a generated argument. For example,
the recorded Bayesian package has a route with nine antecedents, exceeding the
default of eight. An explicit EAL limits file permits the bounded construction:

```toml
[limits]
formal_antecedents = 16
```

Pass that file as `--limits aspic-limits.toml`. Both native steps use the same
limits and share `--timeout-secs` (30 seconds by default). An exhausted bound
fails without publishing a partial view. Export also enforces the browser's
4 MiB, 4,096-argument and 65,536-defeat import limits. Raising a bound changes
computational capacity; it does not add evidence or justify the claim.

### Collect and assess as separate operations

Use this alternative when you need to inspect the collection before assessment.
Both commands must use the same package, context and run store. Create the
workspace first because EngArg's wrapper requires it to exist.

```sh
mkdir run-staged
engarg eal validate --input "$PACKAGE/case.eal" --input-format eal \
  --eal-host "$EAL_HOST" --workspace "$PWD/run-staged" --output validation.json
engarg eal plan --input "$PACKAGE/case.eal" --input-format eal --claim "$CLAIM" \
  --eal-host "$EAL_HOST" --workspace "$PWD/run-staged" --output plan.json

engarg eal collect --input "$PACKAGE/case.eal" --input-format eal --claim "$CLAIM" \
  --tools "$PACKAGE/tools.toml" --eal-host "$EAL_HOST" \
  --workspace "$PWD/run-staged" --timeout-secs 3600 --output collection.json

export COLLECTION_ID="$(jq -er '.result.collection_id' collection.json)"
export NOW="$(python3 -c 'from datetime import datetime, timezone; print(datetime.now(timezone.utc).isoformat())')"
engarg eal assess --input "$PACKAGE/case.eal" --input-format eal \
  --tools "$PACKAGE/tools.toml" --collection-id "$COLLECTION_ID" --now "$NOW" \
  --eal-host "$EAL_HOST" --workspace "$PWD/run-staged" \
  --timeout-secs 3600 --output assessment.json

export ASSESSMENT_ID="$(jq -er '.result.assessment_id' assessment.json)"
engarg eal explain --assessment-id "$ASSESSMENT_ID" --claim "$CLAIM" \
  --eal-host "$EAL_HOST" --workspace "$PWD/run-staged" --output explanation.json
engarg eal packet --assessment-id "$ASSESSMENT_ID" --claim "$CLAIM" \
  --eal-host "$EAL_HOST" --workspace "$PWD/run-staged" --output packet.json
```

For a credentialed collector, add `--collector-env CI_READ_TOKEN` to `collect`.
The name must also occur in the selected manifest's `inherit_env`. A bare command
such as `python3` may also need `--collector-env PATH`. Only operator-declared
public configuration should additionally use `--public-collector-env NAME`.
No API model key is needed for validation, planning, collection or assessment
unless an explicitly bound evidence tool itself uses that service.

`assess --collection-id` reuses the retained collection without acquiring sources
again. It preserves the original evidence age. To obtain new measurements, run
`collect` again with a new output path, then assess its new collection ID.
Do not change the EAL or context and attach an old collection to it.
The explicit assessment clock above retains fractional seconds. Truncating the
current time can incorrectly place a just-collected observation in the future.

## 7. Resolve gaps and revise explicitly

| Finding | Required next action |
|---|---|
| Missing direct binding | Supply the actual operation or generate a verified implementation, then build a new package. |
| Missing source access or observations | Grant the intended source permissions or perform the defined measurements, then collect again. |
| Required review absent or inconclusive | Obtain the exact scoped review and retain its original time and reasons. |
| Valid negative measurement | Retain the result and reconsider or narrow the claim through an explicit revision. |
| Inapplicable assumption or unresolved challenge | Obtain its specified evidence or explicitly revise the argument. |
| Method/query or acceptance criterion is wrong | Make an explicit argument/support revision, rebuild and collect against the new requests. |
| Tool process deadline | Inspect its receipt and adjust the operator tool timeout and host deadline only when justified. This is separate from model authoring timeouts. |

Do not hand-edit the accepted report and retain its old provenance. Export the
current argument and elaboration as starting points for a reviewed revision:

```sh
jq '.argument' "$ANALYSIS" > revised-argument.json
jq '.support_elaboration' "$ANALYSIS" > revised-support.json
```

Edit those proposals, retaining dissent applications and explicit change reasons.
Changed requirements must carry their previous digest and revision rationale.
Inspect and then accept the revision with new output paths:

```sh
engarg eal revise --input "$ANALYSIS" --argument revised-argument.json \
  --support-elaboration revised-support.json --tools "$TOOLS" \
  --preview --output revision-preview.json
engarg eal revise --input "$ANALYSIS" --argument revised-argument.json \
  --support-elaboration revised-support.json --tools "$TOOLS" \
  --output revised-analysis.json
engarg build --input revised-analysis.json --tools "$TOOLS" --output-dir revised-investigation
```

The revision preserves the parent and dissent and invalidates derived work.
Inspect its new review requests and execute the new package. Completion means
that the declared investigation ran and retained a valid assessment with its
limitations. It may properly conclude that the claim remains unsupported.

## A runnable check without model calls

This finite synthetic example tests the installed path before a paid authoring
run. It performs actual MCP acquisition, a command audit, pure derivations and
deductive conjunction. It uses no catalogue-authoring wrapper:

```sh
"$EAL_HOST" run "$ENGARG_REPO/examples/executable/finite-inventory/case.eal" \
  --tools "$ENGARG_REPO/examples/executable/finite-inventory/tools.toml" \
  --workspace "$PWD/finite-check" --claim inventory_verified --timeout 120 \
  > finite-execution.json
jq '.result.assessment | {valid, complete, claim: .claims.inventory_verified}' \
  finite-execution.json
```

Expect a supported claim about exactly the retrieved three-entry synthetic
snapshot. This verifies installation and execution, not the CI performance
claim or an external catalogue's population. For API and contract details, see
[executable packages](executable-packages.md),
[reasoning methods](REASONING_METHODS.md) and
[EAL direct tool manifests](https://github.com/emmett08/eal-mcp/blob/main/docs/direct-tools.md).
