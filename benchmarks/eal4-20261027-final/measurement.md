# EAL/4 completion measurement

Does the retrieved fixture-v1 snapshot contain exactly three pairwise distinct case-sensitive string entries?

One synthetic finite claim, two local command transports, fixed observed and assessment times, fresh stores. No external catalogue/population or model authoring measurement.

| Observed metric | Frozen completion baseline | Current EAL/4 |
|---|---:|---:|
| Requests per fresh run | 2 | 2 |
| Duplicate acquisition requests | 0 | 0 |
| Execution model calls | 0 | 0 |
| Package files | 3 | 3 |
| EAL source bytes | 2742 | 2720 |
| Logical acquired snapshot bytes | 93 | 93 |
| Retained observation records | 4 | 4 |
| Retained pure derivation records | 2 | 2 |
| Store bytes | 73760 | 73760 |
| Median end-to-end milliseconds | 120.015 | 119.965 |

Both runs use 4 evidence obligations and 9 acceptance predicates. The independent static inventory found 0 duplicate complete obligation/evaluator mappings. All retained normalized observation values match.

Source inventory and added/modified/removed production-file counts for both historical bases are in `measurement.json`. They are separate from the execution measurements.

- Latency includes startup, collection, persistence and assessment; repetitions on one host do not establish performance superiority.
- Source inventory and source-change fan-out are static counts, not measured operational complexity or authoring cost.
- Duplicate evaluator count concerns identical complete authored obligation-to-evaluator mappings. Assessment integrity recomputation is a distinct lifecycle step, not a second semantic model.
- Original-plan revisions have source inventory only; no execution timing or historical authoring-call count is inferred.
- Store bytes sum logical file lengths after process exit, including SQLite metadata and lock/key files, not allocated disk blocks.
