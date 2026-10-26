# Specialist debate topologies

EngArg supports the nine communication structures described in [Motger et al., *Multi-Agent Debate Strategies: Survey, Taxonomy, and Challenges*, section 5.1](https://arxiv.org/html/2607.26212v1#S5.SS1). A topology determines which specialist assessments another specialist receives. The execution protocol uses simultaneous logical rounds. Graph selection and scheduling have separate configuration.

The default `--mode hybrid` workflow retains its six calls. Selecting `--topology` or `--topology-config` adds peer exchange between the independent specialist assessments and the existing synthesis, final critique and adjudication. These options require hybrid mode. Single mode remains a one-call baseline.

## Run a topology preset

```sh
engarg analyse \
  --claim 'Parallelising independent CI test groups reduces pipeline duration under workload W.' \
  --model YOUR_MODEL_ID \
  --topology ring --debate-participants 6 --debate-rounds 2 \
  --max-calls 21 \
  --format json --audit ring.audit.jsonl --output ring.json
```

A preset defaults to six participants and one peer round. Participant counts range from 3 to 16. Peer rounds range from 1 to 8. Participants use constructor, sceptic and evidence-auditor roles cyclically in that order. Participant indices start at zero and identify separate calls even when their roles coincide. Per-role model overrides apply to every participant assigned that role.

Every participant first assesses the original request independently. In each subsequent round, it receives its own previous assessment and the previous-round assessments sent along its incoming edges. All prompts for a round are checked before any call in that round starts. The round completes before the next begins. The provider's HTTP concurrency limit can pace these logically simultaneous calls without changing their input context.

The required logical-call allowance is:

```text
participants × (peer rounds + 1) + 3 + max_repairs
```

The three final stages are synthesis, final critique and adjudication. Six participants and two peer rounds require 21 calls. Reserving one repair requires 22. An insufficient allowance fails before any model call. Token limits, byte limits, timeouts and HTTP retry limits continue to apply separately. More edges can increase prompt size without increasing the number of calls.

## Structures and directions

An edge `{ "from": u, "to": v }` permits participant `v` to receive participant `u`'s previous assessment. Self edges are omitted because every participant already receives its own previous assessment.

| Preset | Communication | Default arrangement |
| --- | --- | --- |
| `fully-connected` | Every distinct pair exchanges assessments in both directions | All participants |
| `grouped` | Complete exchange within each disjoint group | Two contiguous groups |
| `chain` | Each participant sends to its successor | `0 → 1 → … → n−1` |
| `ring` | The chain closes back to its first participant | `0 → 1 → … → n−1 → 0` |
| `star` | Every other participant sends to one aggregation hub | Hub 0 |
| `tree` | Each child sends to its parent | Binary tree rooted at 0 |
| `layered` | Every participant sends to every participant in the next layer | Three contiguous layers |
| `sparse` | Only declared directed edges carry assessments | Bidirectional ring |
| `bipartite` | Complete exchange across two partitions in both directions | Two contiguous partitions |

`fully_connected` is accepted as an alias for `fully-connected`. JSON uses `fully_connected`. The grouped structure implements within-group exchange. Cross-group exchange can be introduced through a scheduled graph or occurs at the final global synthesis. Tree direction towards the root and star direction towards the hub are EngArg's aggregation choices. The survey does not prescribe a unique direction or parameterisation for every structure.

Information travels one edge per peer round. A chain needs enough rounds for an initial assessment to traverse multiple participants. The tree similarly needs sufficient rounds for a leaf assessment to reach its root. A participant's generated assessment can carry information received in earlier rounds. These rules constrain immediate prompt inputs rather than guaranteeing secrecy between participants.

## Explicit graph configuration

Use a complete JSON configuration when a preset does not express the intended arrangement:

```sh
engarg analyse --input examples/request.json --model YOUR_MODEL_ID \
  --topology-config examples/debate/sparse.json --max-calls 15 \
  --format json --output sparse.json
```

`--topology-config` is mutually exclusive with `--topology`, `--debate-participants` and `--debate-rounds`. The file is bounded to 65,536 bytes. Unknown JSON fields, invalid endpoints and duplicate edges fail admission. Groups, layers and partitions must cover every participant exactly once. Each group or layer must contain a participant.

```json
{
  "participants": 6,
  "rounds": 1,
  "topology": {
    "kind": "sparse",
    "edges": [
      { "from": 0, "to": 1 },
      { "from": 1, "to": 2 },
      { "from": 2, "to": 0 },
      { "from": 2, "to": 3 },
      { "from": 3, "to": 4 },
      { "from": 4, "to": 5 },
      { "from": 5, "to": 3 }
    ]
  },
  "adaptation": { "kind": "static" }
}
```

The topology object accepts these variants:

| `kind` | Additional fields | Constraint |
| --- | --- | --- |
| `fully_connected` | None | All distinct ordered pairs |
| `grouped` | `groups` | At least two nonempty disjoint groups covering all participants |
| `chain` | None | Forward successor edges |
| `ring` | None | Forward successor edges and closing edge |
| `star` | `hub` | A valid participant index |
| `tree` | `parents` | Exactly `n−1` parents for children 1 through `n−1`, forming a tree rooted at 0 |
| `layered` | `layers` | At least two nonempty ordered layers covering all participants |
| `sparse` | `edges` | Unique directed edges with valid distinct endpoints |
| `bipartite` | `left`, `right` | Two nonempty disjoint partitions covering all participants |

For example, `{ "kind": "tree", "parents": [0, 0, 1, 1, 2] }` defines six participants. Children 1 and 2 send to root 0. Children 3 and 4 send to 1, while child 5 sends to 2. Empty sparse edge lists are admitted to study isolated self-revision under the same final global review.

## Scheduled connectivity

Static connectivity reuses the same graph for every peer round. A scheduled configuration changes the graph at round boundaries using explicit edge sets. The initial peer round uses the chosen topology. `round_edges` then supplies exactly one replacement graph for every later peer round. Each replacement is the complete edge set for that round. An empty replacement isolates participants for that round while preserving their own previous assessments.

The [scheduled example](../examples/debate/scheduled.json) uses two groups in its first peer round. Its second peer round connects participants 2 and 3 in both directions:

```json
{
  "participants": 6,
  "rounds": 2,
  "topology": {
    "kind": "grouped",
    "groups": [[0, 1, 2], [3, 4, 5]]
  },
  "adaptation": {
    "kind": "scheduled",
    "round_edges": [
      [
        { "from": 2, "to": 3 },
        { "from": 3, "to": 2 }
      ]
    ]
  }
}
```

```sh
engarg analyse --input examples/request.json --model YOUR_MODEL_ID \
  --topology-config examples/debate/scheduled.json --max-calls 21 \
  --format json --output scheduled.json
```

This is reproducible edge adaptation. Every participant still executes in each round. Learned pruning, quality-based routing and changing participant populations require their own policies and validation.

## Dissent and audit interpretation

The peer graph applies to the specialist debate. Synthesis, final critique and adjudication receive the complete independent assessments and discussion, with a global register of all objections. Each peer prompt receives only the objections needed to resolve its own and permitted peers' assessment references. Restricted connectivity therefore preserves final dissent handling without broadcasting the global register during peer exchange.

Saved run configuration records the topology and schedule. Peer prompt metadata records participant identity, round and graph context. Transcript order follows round and participant order, while completion observers retain actual completion receipts as calls finish. Comparing both records distinguishes logical ordering from provider completion timing.

The graph contract constrains context visibility and resource use. It supplies no evidence that one topology produces stronger arguments. The implementation instantiates the survey's structural categories under EngArg's bounded protocol. Empirical comparisons need controlled claims, model assignments, budgets and independent quality measurements.

## Architecture

`engine/topology/` contains the directed graph contract and a cohesive builder module for each structure. A tagged Rust enum dispatches to these stateless builders. The common graph representation lets a shared specialist executor handle prompt routing, round barriers, audit ordering and objection retention. Normal construction, flat tool planning and support-graph planning use the same specialist execution path.

This uses static strategy selection through ordinary functions. A trait hierarchy would add indirection to these closed, data-selected variants without changing the executor's graph contract. A new structure requires a builder, its enum variant and graph tests. Execution and final validation remain independent of the graph shape.

Saved reports keep the existing `engarg.run.v2` schema. Earlier configurations without `debate` still load and use the legacy workflow. Rust callers constructing `RunConfig` with an exhaustive struct literal must add `debate: None` or use `..RunConfig::default()`. Public engine type paths remain unchanged after configuration extraction.
