# Turn a claim into obtainable support

[Documentation](index.md) · [Argument model](DOMAIN_MODEL.md)

The CI duration claim needs more than a proposed speed-up. It needs a definition
of the compared workloads, measurements under the chosen scheduling policy and
a way to detect changed test outcomes. Support planning turns those dependencies
into explicit obligations before choosing an implementation.

Direct planning is the default authoring mode. It turns a claim into an argument
and explicit support requirements, with operation names bound by the operator's
TOML. The resulting executable package is an EAL file, a TOML manifest and any
local tool implementations.

```bash
engarg analyse --input request.json --tools tools.toml \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY --eal-host "$EAL_HOST" \
  --format json --output analysis.json
engarg build --input analysis.json --tools tools.toml --output-dir case-package
eal-host run case-package/case.eal --tools case-package/tools.toml \
  --workspace case-package/run
```

Authoring and execution are separate. The first command calls the configured
language model and retains its full exchanges. The build command validates and
compiles the authored case without model calls. The host executes the EAL source,
collects observations and assesses the selected claim.

## Authoring contract

The author describes each material target's support requirement independently of
whether an implementation is available. Requirements retain their target,
original acceptance clauses, measurement units, subject scope, coverage, time
bounds, provenance and challenges. Acquisitions specify inputs and exact
source-to-input bindings. Derivations are deterministic expressions over those
values. Evaluations declare the criteria that must hold.

The provider receives a bounded operation and pinned interface view from `--tools`, never
the manifest's command arguments, credential values or connection configuration.
A missing direct binding is recorded under `direct_bindings.unbound_operations`.
It does not become a successful observation. A package build requires concrete
bindings for its acquisitions.

Hybrid mode normally uses six logical calls: Constructor, Sceptic,
EvidenceAuditor, Synthesiser, FinalCritic and Adjudicator. The critic receives the
candidate binding plan with its limitations. Deterministic matching after
synthesis and adjudication makes no source calls. Single mode remains the
one-call authoring baseline. Model, repair, time and byte budgets remain explicit.

The saved report has `argument`, `support_elaboration`, `direct_bindings`,
`design_review` and the retained transcript. Support elaborations use
`engarg.support-elaboration/2`. Each case-level alternative has its exact
distinguishing test and explicit support requirements. Uncovered alternatives
are validation errors.

## Runtime contract

The compiler writes executable EAL acquisitions, exact input bindings,
derivations, requirements and native method applications. The runtime schedules acquisitions from
the dependencies declared in that source and resolves tools through TOML. The
build receipt records diagnostics and package identity for inspection.

TOML binds an operation to a command, an MCP stdio server or an MCP HTTP endpoint.
Relative files resolve from the manifest directory. Commands receive bounded
requests. MCP calls use the selected tool and explicit response projection. Both
transports retain source identity, original observation time, data digests and
dependency lineage. Manifest-declared environment inheritance is explicit.

Derived computations do not refresh their oldest source observation. Missing,
failed, stale or altered inputs remain unavailable. Collection completion and
structural validity do not by themselves establish a claim. Retained reviews
must match the exact criterion and scope and include reviewer identity, verdict,
reasons and the original review time.

## Reasoning and revision

Computational warrants select an EAL-owned method identifier and exact authored
query. Evidence binds only the empirical fields that the method needs. A pure
formal query is authored programme data. Only its empirical inputs require
acquisition. Method outputs and permitted result paths come from the
same EAL descriptor used by the runtime. See [reasoning methods](REASONING_METHODS.md).

Replanning preserves an argument's obligations. An argument change is a separate
explicit revision through `engarg eal revise`: the previous argument, dissent
history and parent identity remain retained, and derived executable artefacts
are invalidated. Rebuild and collect against the revised exact requests.
Historical evidence is never relabelled as a newly accepted review.

A checkpoint records completed authoring calls. Inspect it with
`engarg authoring inspect --checkpoint PATH`. A checkpoint whose primary authoring
stages are complete can resume final validation and explicitly bounded repair
through `engarg authoring resume`. The command checks the original manifest
identity and requires new output, audit and checkpoint paths. An interrupted
specialist or synthesiser request requires a new authoring attempt. The
[recovery instructions](USAGE.md) explain how to inspect `resumable` before
choosing either path.

Optional capability discovery can help an operator find tools. Explicit
`--evidence-planning graph --evidence-config CONFIG` retains that discovery
workflow as an authoring input. Its output still needs concrete direct bindings
before package execution. Discovery, qualification and implementation generation
are not runtime prerequisites.

Once the obligations and bindings are ready, [build an executable package](executable-packages.md).
The [finite-inventory example](finite-inventory.md) provides a complete synthetic
run, while [USAGE.md](USAGE.md) follows a new engineering claim through the
individual public commands.
