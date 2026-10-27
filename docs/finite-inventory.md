# Check the installed path with a finite inventory

[Documentation](index.md) · [Engineer workflow](USAGE.md)

Before authoring a CI performance claim, establish that the installed applications
can acquire an observation, retain it and use it in a calculation. This example
answers a deliberately small question: does the retrieved synthetic snapshot
contain exactly three distinct case-sensitive entries?

A local MCP server supplies the snapshot. A separate command audits its entries.
EAL derives count and distinct count, checks their acceptance predicates and
executes a deductive conjunction. The claim concerns that retrieved snapshot,
so the example tests installation and execution without claiming completeness
of an external catalogue or evidence about a production pipeline.

## Run the EAL source directly

From an EngArg checkout with Git, Python 3, jq and the repository's Rust toolchain
available, install the paired runtime and run the example:

```sh
export ENGARG_REPO="$(pwd -P)"
export EAL_HOST="$(bash scripts/install-eal-runtime.sh)"

"$EAL_HOST" run "$ENGARG_REPO/examples/executable/finite-inventory/case.eal" \
  --tools "$ENGARG_REPO/examples/executable/finite-inventory/tools.toml" \
  --workspace "$ENGARG_REPO/../finite-inventory-run-001" \
  --claim inventory_verified --timeout 120 \
  > finite-inventory-execution.json

jq '.result.assessment | {valid, complete, claim: .claims.inventory_verified}' \
  finite-inventory-execution.json
```

Use a new workspace and output path for another attempt. Expect a valid,
complete assessment with `inventory_verified` supported. The command performs
actual source acquisition through `eal-mcp`, with no model calls. Keep the run
store and output together when inspecting a failure.

## Follow the evidence through the package

Read [case.eal](../examples/executable/finite-inventory/case.eal) alongside
[tools.toml](../examples/executable/finite-inventory/tools.toml). The source
contains the dependencies and criteria. The manifest grants the concrete
transport authority. Its `mcp_stdio` binding selects the snapshot server and
maps its structured response to an observation value and original timestamp.

Relative implementation paths resolve from the manifest's directory. The
selected environment names are explicit, which lets the same package execute
from a different directory while retaining its authority boundary. Source
request identity and observation age remain available to the assessment.

The independent audit and derived count have different evidential roles.
Obtaining two predicates from one snapshot does not create two independent
samples. The final conjunction also requires its premises to be accepted under
the declared scope. A successful calculation alone cannot establish an absent
empirical premise.

## Interpret a different result

A source process failure leaves acquisition unavailable. A measured count that
fails the criterion is an acquired negative. A stale observation fails its age
condition. Those outcomes can all prevent claim support, but they call for
different responses from the engineer.

The host's successful exit means that the requested operation returned. Inspect
`valid`, `complete` and the selected claim status to determine what the run
established. The [engineer workflow](USAGE.md#6-execute-the-eal-collect-and-assess)
shows collection, assessment, explanation and packet operations separately when
you need to inspect the retained evidence before making a decision.

Once this example works, return to [authoring the CI investigation](USAGE.md#2-state-the-question-and-available-sources).
Replace its synthetic source with the actual bounded acquisition contracts your
claim requires. The example's supported result cannot supply those observations.
