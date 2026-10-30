# Generate, investigate and shorten an argument

[Documentation](index.md) · [Route optimisation](optimal-argument.md)

`engarg investigate` proposes revised arguments, checks their premises and
formalisation against an operator-owned verification plan, executes their EAL
packages, and sends the results back for repair. A separate sceptic call can
reject a candidate; agreement between models cannot make a failed check pass.

```sh
engarg investigate --input analysis.json --tools operator-tools.toml \
  --verification verification.json --eal-host "$ENGARG_EAL_HOST" \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --rounds 3 --max-calls 8 --beam-width 2 --output-dir investigation
```

The input is an admitted `engarg.run.v3` report with complete support
elaboration. The tools manifest supplies existing source and review operations;
the model receives their contracts, while the operator retains execution
authority. Install the EAL revision pinned by this checkout. Assistant providers
and HTTP settings use the same options as [tool implementation](TOOL_IMPLEMENTATION.md).
Collection can contact external sources. Forward necessary variable names with
`--collector-env`; their values do not become model context.

## Define what a successful revision must preserve

The verification plan uses schema `engarg.assurance-plan/1`. Its checks name an
original argument target, a role (`premise`, `formalisation`, `inference` or
`applicability`), and one of these procedures:

| Procedure | What passes the check |
|---|---|
| `requirement` | The named original requirement has an actual successful native observation and satisfies its original predicate, scope and freshness conditions. |
| `value` | A JSON pointer into the proposed `SupportedArgument` equals the independently supplied `expected` value. This checks correspondence to that reference. |
| `method` | The selected conclusion uses the specified method and exact `reference_query`, and native execution satisfies the specified result predicate. |
| `logical_equivalence` | Native deductive checks establish equivalence of the premise conditions and, separately, the conclusions of the intended and proposed queries. Countermodels are retained. |

For example, this is one check within a complete plan:

```json
{
  "id": "intended_inference",
  "role": "formalisation",
  "target_id": "current_claim",
  "procedure": {
    "kind": "method",
    "conclusion_id": "current_claim",
    "identifier": "deductive/1",
    "reference_query": {"premises": ["p", {"implies": ["p", "q"]}], "conclusion": "q"},
    "result": {"path": "entailed", "operator": "==", "expected": true}
  }
}
```

Every originally selected support requirement must have a `requirement` check
or an explicitly permitted formal discharge. At least one formalisation check
is required. Use source-backed requirements for external claims and separate
applicability requirements for the conditions under which the inference can be
used. The command rejects an incomplete plan before model calls or collection.
The tool does not infer a trustworthy reference merely from a model's assertion.

Plan checks refer to targets declared in the starting case. The loop can propose
new warrants for those conclusions and revise their selected dependencies.
Introducing a new empirical target requires extending the starting case and its
operator-owned plan; a model cannot enlarge its own verification authority.

The original current claim, qualifier, scope, stronger target, acceptance
criteria and source contracts remain fixed. Critical assumptions and material
defeaters remain attached. Changed ground meanings, formal-premise mappings and
typed conclusion quantities require explicit complete reference values.
Removing a selected dependency requires successful checks discharging every
original obligation attached to it.

A formal discharge names both an original requirement ID and its canonical
SHA-256. Only a successful native formalisation or inference check may discharge
a mathematical calculation, proof check or judgement obligation. Observation,
source retrieval, experiment, assumption and challenge obligations cannot be
removed by this mechanism. Original acquisitions and derivations remain in the
revision even when a formal obligation has been discharged.

## Inspect the retained result

The new private output directory contains:

- `input-report.json`, `verification-plan.json` and `run.json`: the admitted
  starting case, frozen policy, runtime identity, input hashes and budgets.
- `audit.jsonl`: model requests, completions, usage and failures, retained before
  interpretation. Treat this file as confidential case material.
- `candidates/NNNN/`: proposals, verification receipts, admitted revisions,
  packages and native executions, including rejected attempts.
- `investigation.json`: candidate assessments, criticism, selected paths and
  the bounded search result. `failure.json` records an interrupted or failed run.

Each verification receipt binds the original report, plan and exact proposal by
digest. `pass`, `fail`, `unknown` and `error` remain distinct. Missing, stale or
negative evidence prevents selection. A successful method execution must also
satisfy its result predicate; completion alone supplies no support.

Requirement receipts expose the input actually evaluated, its result, the
original acceptance criterion and dependency clocks and digests. They omit raw
acquisition requests, transport settings and unrelated source fields.

Repair prompts include recent rejected proposals alongside their diagnostics,
labelled as unadmitted data. Rejection never makes a proposal an executable
parent. Verification receipt IDs identify checks; they are not supplied evidence
IDs and cannot be inserted into the argument as observations.

The baseline is checked first. Generated candidates must pass all mandatory
checks, a fresh native EAL assessment, independent replay, and the sceptic gate.
The smallest eligible selected dependency closure wins; shared nodes count
once. This is a structural size measure. It does not rank all supported
arguments by evidential strength.

Nodes used only by other branches stay in the canonical argument and add
nothing to this objective.

The round, call, beam, byte and timeout limits bound the work. Failed model
calls consume the call budget. Unreviewed proposals remain ineligible when the
critic budget runs out. Critical findings against an identical argument and
support elaboration remain binding when receipt timestamps change; a later
reviewer's omission cannot clear them. Tool implementations and the host are
checked for changes between attempts; source data can change and observations
retain their own times.

## What the checks establish

Native methods check declared mathematical relationships. `finite_probability/1`
can enumerate a bounded Bayesian network with exact rational inputs and return
an exact posterior; its node definitions are authored model data. Such a result
does not establish that those probabilities describe a physical power supply.
Empirical model adequacy still needs its source, calibration and applicability
requirements. Structured support remains an authored argument, without a formal
proof of its prose.

The result is always `best_verified_candidate_found` or
`no_verified_supported_candidate`; `search_complete` remains false. Generated
natural-language alternatives are not exhaustively enumerable. The plan's
references and scope determine what has been checked, so inspect them alongside
the selected argument before relying on it.

## Run the synthetic acceptance example

The independently authored test fixture contains a redundant proof condition,
an incorrect premise mapping, and actual local source operations. It concerns a
test protocol; it supplies no evidence about physical power supplies. Export a
fresh copy from the repository:

```sh
mkdir -p work
ENGARG_INVESTIGATION_FIXTURE_DIR="$PWD/work/investigation-fixture" \
  cargo test --locked --test argument_investigation export_synthetic -- --ignored
engarg investigate --input work/investigation-fixture/input.json \
  --tools work/investigation-fixture/tools.toml \
  --verification work/investigation-fixture/verification.json \
  --eal-host "$ENGARG_EAL_HOST" --model "$ENGARG_MODEL" \
  --api-key-env OPENAI_API_KEY --rounds 3 --max-calls 6 --beam-width 1 \
  --output-dir work/investigation-result
```

The export destination must be empty. `proposal.json` is an independently
specified accepted repair for inspecting the fixture and testing the verifier;
the model-driven command receives only the input report, tools and verification
plan. A generated run may fail to find that repair. The paired native test feeds
wrong-query and weakened-criterion proposals before the valid repair and checks
that only the repaired four-node current argument is selected, compared with
five selected dependency nodes at the start.

A live smoke test on 30 October 2026 used `gpt-6.1-sol` for one proposal and one
sceptic call. It generated a four-node candidate that passed native execution
and the fixed checks. The sceptic questioned how the fixture's count/distinct
measurements established the propositions `p` and `p implies q`, among other
critical findings. The controller retained that veto and returned
`no_verified_supported_candidate`, with null selected paths. The candidate,
native result and criticism remained available for inspection. This records
the gates' behaviour on one synthetic case; it leaves broader argument quality
for a separate evaluation.
