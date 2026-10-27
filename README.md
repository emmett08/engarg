# EngArg

EngArg develops engineering arguments from a claim, optional context and optional
source material. It constructs an extended Toulmin argument, examines material
objections and specifies the evidence needed for its premises and warrants.
A coherent candidate remains distinct from an established conclusion.

EngArg compiles the authored investigation into `case.eal`, `tools.toml` and any
local tool implementations. EAL executes that package independently, gathers
observations and assesses the claim:

```sh
engarg build --input analysis.json --tools operator-tools.toml --output-dir investigation
eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

The TOML binds named operations directly to commands or MCP servers. No capability
catalogue, registration directory or execution-plan JSON is required.
`build.json` is a diagnostic receipt; EAL and the tool bindings define execution.
Missing evidence and scoped reviews remain visible in the assessment. See
[executable packages](docs/executable-packages.md) and
[architecture decisions](docs/EXECUTABLE_ARCHITECTURE.md).

## Install and try it

Install the pinned Rust toolchain with [rustup](https://rustup.rs/). A C compiler is
required by the TLS dependencies. From this repository:

```sh
cargo install --path . --locked
bash scripts/install-eal-runtime.sh
engarg demo --claim-only --format markdown
engarg demo --format outline
engarg demo --format prose
engarg schema request
engarg validate --input examples/request.json --case examples/case.json
```

The demo is synthetic and performs no model calls. `demo --claim-only` starts
from the minimal request in `examples/claim-only-request.json`. The installed
`engarg-process-supervisor` companion provides bounded subprocess execution.

To execute a complete synthetic investigation with an actual local MCP source,
command audit and deductive conjunction:

```sh
bash scripts/run-catalog-sync-authoring.sh --simple-example \
  --output-dir engarg-runs/finite-inventory
```

This example performs no model calls and concerns only its declared finite
snapshot. It does not establish a production catalogue or infrastructure claim.
See [the example guide](docs/CATALOG_SYNC_AUTHORING.md).

## Author an investigation

Set `ENGARG_API_KEY` using your secret manager or environment, and select an exact
model identifier supported by your provider. Direct operation planning is the
default:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model "$ENGARG_MODEL" --format json \
  --audit analysis.audit.jsonl --output analysis.json
```

A claim alone is also sufficient:

```sh
engarg analyse \
  --claim 'Parallelising independent CI test groups reduces pipeline duration without changing test outcomes.' \
  --model "$ENGARG_MODEL" --format json \
  --audit analysis.audit.jsonl --output analysis.json
```

Without supplied tools, operations remain explicitly unbound. The provider
receives operation names, versions and pinned interface schemas from TOML; command arguments, environment
values and credentials stay out of the authoring view. The saved report contains
the canonical argument, `support_elaboration`, `direct_bindings`, design review
and every completed model exchange. Building requires concrete bindings for the
resulting acquisitions.

The default endpoint uses OpenAI's Responses API. To use `OPENAI_API_KEY`, select
`--api-key-env OPENAI_API_KEY`. Keys do not belong in command arguments or request
JSON. Claim text, evidence and prior assessments are sent to the configured
provider. The adapter sends `store: false`; provider retention policies remain
outside EngArg.

Compatible Chat Completions endpoints can be selected explicitly:

```sh
engarg analyse --input request.json --tools tools.toml \
  --model YOUR_MODEL --api chat \
  --endpoint http://localhost:8000/v1/chat/completions \
  --output analysis.json
```

HTTP is permitted on loopback only; other destinations require HTTPS. Endpoints
must support the selected structured-output contract. Rejection, refusal and
truncation fail explicitly. Optional `--reasoning-effort` and `--service-tier`
values are passed unchanged and retained with provider-reported metadata. Select
values supported by every model in the run.

Hybrid mode normally uses six calls: Constructor, Sceptic, EvidenceAuditor,
Synthesiser, FinalCritic and Adjudicator. `--mode single --max-calls 1` provides a
one-call baseline. `--role-model sceptic=MODEL_ID` and equivalent role overrides
select different models through the same endpoint. Separate calls do not imply
independent errors.

`--no-tool-planning` requests an argument-only report. Otherwise analysis always
extracts executable support requirements; there is no flat planning mode or
analysis catalogue switch. Optional
`--evidence-planning graph --evidence-config CONFIG` enables capability
discovery as authoring assistance. The executable package still uses concrete
TOML bindings. See [support planning](docs/support-planning.md).

## Reasoning and evidence

Computational warrants use EAL-owned descriptors for deductive, inductive,
abductive, causal, counterfactual and analogical methods. EngArg retains a method
identifier and exact authored JSON query, plus explicit empirical input bindings.
It does not duplicate EAL's formula or statistical schemas. Pure formal queries
are executable program data and need no artificial source that echoes them.
Practical reasoning uses explicit policy and scoped review obligations.

A successful acquisition records a measurement. Claim support also requires the
declared criteria, accepted premises, applicable assumptions and required reviews.
Reviews retain exact criterion and scope, reviewer identity, verdict, reasons and
original review time. Structural validation does not establish evidence truth or
prose sufficiency. See [reasoning methods](docs/REASONING_METHODS.md).

TOML operations resolve to commands, MCP stdio or MCP HTTP. Commands consume the
bounded EAL request envelope on standard input. MCP bindings specify the tool and
response projection. Relative files resolve from the manifest directory;
inherited environment names are explicit. Collection retains original source
clocks, identity digests and dependency lineage. Derived calculations cannot
refresh stale evidence. Moving a complete package does not require a planning
sidecar or registration directory.

## Revisions and recovery

`--argument-depth N` bounds the authored warrant layers on every support path
(1–16). The boundary must retain concrete evidence obligations for unresolved
premises. It does not bound truth or waive challenges.

`--max-repairs N` permits 0–3 extra calls for invalid final model content, within
the reserved call and time budgets. The default is zero. Hybrid mode with one
repair therefore needs `--max-calls 7`. Every completed correction is retained;
provider errors and truncation do not trigger repairs. These calls repair output
structure, not evidential adequacy.

Retained objections cannot disappear or silently change target. A revised target
requires an explicit dissent application and rationale; the host records its
original and current identities. `DISSENT_TARGET_CHANGED` identifies a missing
application. Use `engarg eal revise` for an explicit accepted-argument revision.
The previous case remains retained and its derived execution artifacts are
invalidated. Replanning does not replace a warrant or acceptance criterion.

Add `--checkpoint authoring.checkpoint.json` to retain each completed authoring
exchange before final validation. Inspect or resume it without repeating prior
specialist calls:

```sh
engarg authoring inspect --checkpoint authoring.checkpoint.json
engarg authoring resume --checkpoint authoring.checkpoint.json \
  --tools tools.toml --max-repairs 1 --model YOUR_MODEL \
  --audit resumed.audit.jsonl --next-checkpoint resumed.checkpoint.json \
  --output resumed-analysis.json
```

Resume checks the original configuration identity and requires new output paths.
A valid saved candidate can finish without another model call. Incomplete or
omitted responses remain blockers. A changed protocol needs explicit
`--accept-protocol-migration` and revalidation. Checkpoints retain at most 32 calls
and 128 MiB; published reports retain the 16 MiB admission bound. Historical
observations are not rewritten or relabelled during migration.

## Coordinated workflows and presentation

`--topology` selects a specialist communication graph such as a ring, tree or
bipartite graph. `eal-coordinate` owns routing and round barriers; EngArg supplies
prompts and retains every objection. See [debate topologies](docs/DEBATE_TOPOLOGIES.md)
for call budgets and configuration.

`engarg ticket --spec ticket.json --run investigation-001` coordinates authoring,
explicit tool implementations, package compilation and EAL execution. Its durable
controller preserves stage identities and incomplete results. See
[ticket execution](docs/TICKETS.md).

The canonical argument supports Markdown, outline and prose presentations without
changing its meaning. The glossary identifies the claim, grounds, warrants,
backing, qualifier, assumptions and defeaters. Rendering APIs return `Result` and
enforce resource budgets; see [presentation API](docs/PRESENTATION_API.md).

## Build and verify

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

`Cargo.lock` fixes the dependency graph. Native integration tests require the
matching EAL companions; compatibility CI builds the exact pinned producer.
Tests exercise real local MCP and command processes, direct package movement,
method inputs, adverse observations, scoped reviews and retained provenance.
Fixture-provider checks establish orchestration, not live model quality.
See [VALIDATION.md](VALIDATION.md) for recorded verification and
[EVALUATION.md](EVALUATION.md) for evaluation requirements.

## Design sources

- [Toulmin, *The Uses of Argument*](https://doi.org/10.1017/CBO9780511840005): claim,
  grounds, warrant, backing, qualifier and rebuttal.
- [Bloomfield, Netkachova and Rushby](https://arxiv.org/html/2405.15800v1): explicit
  defeaters and unsupported outcomes in assurance arguments.
- [Du et al.](https://composable-models.github.io/llm_debate/): debate experiments.
- [Choi, Zhu and Li](https://arxiv.org/html/2508.17536v1): comparison with independent
  initial responses.
- [Wynn et al.](https://arxiv.org/html/2509.05396v2): risks of peer-induced errors.

GPL-3.0-only licensed. Model service, API access and paid inference are separate.
