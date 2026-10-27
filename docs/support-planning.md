# Author executable support requirements

Direct planning is the default authoring mode. It turns a claim into an argument
and explicit support requirements, with operation names bound by the operator's
TOML. The resulting executable package is an EAL file, a TOML manifest and any
local tool implementations.

```bash
engarg analyse --input request.json --tools tools.toml --output analysis.json
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
A missing direct binding is recorded under `direct_bindings.unbound_operations`;
it does not become a successful observation. A package build requires concrete
bindings for its acquisitions.

Hybrid mode normally uses six logical calls: Constructor, Sceptic,
EvidenceAuditor, Synthesiser, FinalCritic and Adjudicator. The critic receives the
candidate binding plan with its limitations. Deterministic matching after
synthesis and adjudication makes no source calls. Single mode remains the
one-call authoring baseline. Model, repair, time and byte budgets remain explicit.

The saved report has `argument`, `support_elaboration`, `direct_bindings`,
`design_review` and the retained transcript. Support elaborations use
`engarg.support-elaboration/2`. Each case-level alternative has its exact
distinguishing test and explicit support requirements; uncovered alternatives
are validation errors.

## Runtime contract

The compiler writes executable EAL acquisitions, exact input bindings,
derivations, requirements and native method applications. The generated source
owns execution meaning. Runtime collection does not read a hidden support graph,
registration directory or planner sidecar. A build receipt records diagnostics
and package identity; it is not an execution dependency.

TOML binds an operation to a command, an MCP stdio server or an MCP HTTP endpoint.
Relative files resolve from the manifest directory. Commands receive bounded
requests; MCP calls use the selected tool and explicit response projection. Both
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
formal query is authored program data; it does not need an artificial observation
that echoes the query. Method outputs and permitted result paths come from the
same EAL descriptor used by the runtime. See [reasoning methods](REASONING_METHODS.md).

Replanning preserves an argument's obligations. An argument change is a separate
explicit revision through `engarg eal revise`: the previous argument, dissent
history and parent identity remain retained, and derived executable artifacts
are invalidated. Rebuild and collect against the revised exact requests.
Historical evidence is never relabelled as a newly accepted review.

A checkpoint records completed authoring calls. Inspect it with
`engarg authoring inspect --checkpoint PATH`; resume with
`engarg authoring resume --checkpoint PATH --tools tools.toml` and the required new
output, audit and checkpoint paths. Resume checks the original manifest identity
before making any separately authorised repair calls.

Optional capability discovery can help an operator find tools. Explicit
`--evidence-planning graph --evidence-config CONFIG` retains that discovery
workflow as an authoring input. Its output still needs concrete direct bindings
before package execution. Discovery, qualification and implementation generation
are not runtime prerequisites.

See [executable packages](executable-packages.md) for the build interface and
[the catalogue example](CATALOG_SYNC_AUTHORING.md) for a complete synthetic run.
