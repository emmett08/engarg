# Find a smaller supported argument

[Documentation](index.md) · [Argument model](DOMAIN_MODEL.md)

For model-generated alternatives, premise investigation and formalisation
repair, use [`engarg investigate`](argument-investigation.md). This chapter
describes the deterministic search over routes already present in a report.

After authoring an argument, `engarg optimise` searches its declared alternative
routes for the unchanged `current_claim`. It checks support before minimising
size. An unsupported short route cannot displace a supported longer route.

```sh
engarg optimise --input analysis.json --tools operator-tools.toml \
  --eal-host "$ENGARG_EAL_HOST" --beam-width 8 --max-candidates 32 \
  --candidate-timeout-secs 120 --output-dir optimisation
```

The input is a normal `engarg.run.v3` report with complete support elaboration.
The output directory must be new. No model calls are made: the LLM has already
proposed the candidate warrants during authoring. This command **collects new
evidence through the supplied tools for each assessed candidate**. Collection
can include external requests and scoped reviews. The candidate budget includes
the original selection. Each host invocation has its own deadline and store;
failed calls are retained without automatic retry. Forward collector variable
names explicitly with `--collector-env`, as for other EAL operations.

## What is optimised

The search changes only `selected_warrant_ids` in `alternative_routes`
compositions reachable from the current claim. Jointly required and cumulative
support remain intact. Every claim, qualifier, scope, premise, warrant,
assumption, defeater and support criterion remains in the canonical argument.
Alternative routes must already have explicit support requirements; the
optimiser cannot invent a missing observation, review or inference.

Support is a hard gate: the exact candidate must receive a valid, complete EAL
assessment with its current claim `supported`. EngArg independently recomputes
that assessment from its retained source, observations, context and assessment
time. Authored status labels and agreement between LLMs supply no substitute.
The checked EAL methods and separate applicability obligations retain their
[existing limits](REASONING_METHODS.md): structured reasoning is not a formal
proof, and a formal result does not establish the truth of empirical premises.

Among candidates passing this gate, length is the number of unique nodes in
`ArgumentIndex::selected_dependency("current_claim")`. Shared subarguments
count once, and attached assumptions and challenges remain visible. This
measures structural economy, not word count, source independence or statistical
strength. All supported candidates belong to one acceptance class; the command
does not invent a scalar measure of evidential force.

## Read the result and use the selected package

`optimisation/optimisation.json` records the objective, frozen target, candidate
selections, dependency IDs, assessments, search limits and selected paths.
Every attempt has a directory under `candidates/`, including failed revisions,
builds and source calls. Successful builds retain their executable package and
admitted report; executions retain their original observation records.

- `minimum_within_declared_routes` means the finite search completed and every
  candidate assessment completed. It is the smallest supported selection among
  those declared routes under their recorded assessments.
- `best_found` means a supported candidate was found, but beam pruning, the
  candidate budget or an unassessed candidate prevents a minimum claim.
- `no_supported_candidate` leaves both selected paths null. Inspect individual
  outcomes for adverse, missing, stale, inapplicable or failed evidence.

Changing a selection is an explicit argument revision. Existing dissent rules
still apply: a candidate requiring an authored dissent disposition is retained
as `invalid_revision`, rather than silently accepting that change. The revised
report remains a normal buildable and revisable report. Its original authoring
history and parent are preserved; old assessments are not attached to the new
source. The selected package is already built, and its execution is beside it.

The beam expands choice points and warrants in lexical order, retaining
supported states first, then shorter states, with lexical ties. It assesses the
original selection first and reuses results only for identical complete
selections within this run. Defaults retain eight states and assess at most 32
candidates; hard limits are 64 retained states and 256 candidates.
Input admission also bounds the graph to 4,096 argument/evidence nodes, 131,072
reference occurrences and 64 alternative choice points. Oversized inputs fail
explicitly instead of silently narrowing the search family.

The operator manifest and local implementation inputs are checked for change
throughout the run. External source data can still change between candidate
executions. Their observations and assessment times remain distinct; this is
not a comparison under a common frozen evidence snapshot. Use identified,
repeatable source fixtures when that comparison is required. Later reliance
must still satisfy current scope, freshness and applicability conditions.

No finite search establishes the strongest possible argument over all natural
language arguments, undiscovered evidence or unrepresented objections.
