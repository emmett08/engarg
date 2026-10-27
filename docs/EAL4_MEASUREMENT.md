# Reproducing the EAL/4 completion measurement

The harness compares the same finite question before and after the completion
changes: whether the retrieved `fixture-v1` snapshot contains exactly three
pairwise distinct entries under case-sensitive string equality. It does not claim
that this snapshot exhausts an external catalogue. The snapshot and audit are
real local command acquisitions; they have identical code and fixed observation
times in both variants. No model or network source participates.

Build the baseline EAL host and companion supervisor at
`cf9d43d42e6983c69f7c09fc72cf2881fe7deef9`, and build the current host and supervisor
in a separate target directory. The harness reads the historical source from
EngArg revision `d181276f39f2189cbadf2654c1d0af2233e5e71e`. It requires the current
finite-inventory example to have identical text except for its language header;
a changed question requires a separately reviewed comparison.

From the EngArg repository, run:

```sh
python3 scripts/measure-eal4-completion.py \
  --baseline-host /path/to/baseline/eal-host \
  --current-host /path/to/current/eal-host \
  --eal-repo ../eal-mcp \
  --output benchmarks/eal4-completion \
  --repetitions 5
```

The output directory must be new. Each repetition has a fresh package and store.
The order alternates to reduce a consistent startup-order bias. The retained
`measurement.json` records executable hashes, source identities, complete command
lines, request counts, package files, logical byte counts, store sizes, latency
samples and complete assessment status. The companion Markdown file summarises
those observations. Every invocation retains its stdout, stderr, source, tools
and request log, so a summary can be checked against the acquired records.
The complete observation and assessment payloads are retained in `stdout.json`.
Generated SQLite stores and their local integrity keys are excluded from Git;
the harness records their measured file sizes before they are discarded.

The testable acceptance bounds are two source requests per fresh run (one shared
snapshot and one audit), four retained observation records (two acquired and two
purely derived), complete support of the bounded claim, and identical normalized
observation values across variants. Execution has zero model calls because its
only executable bindings are the two retained command tools; this says nothing
about the cost of authoring a real claim.

The static obligation inventory records each complete evidence declaration's
input, dependency bindings, derivation or tool identity, acceptance predicates,
environment and age bound. Identical normalized declarations under different
names count as duplicate obligation/evaluator mappings. The finite fixture has
four such obligations and nine predicates. The independent audit and the pure
check of its reported value have different roles and bindings. Assessment's
integrity recomputation of retained derivations is a separate lifecycle step,
not a second authored semantic model. This inventory does not measure internal
function invocation counts.

A separate source inventory compares production Rust and grammar files against
the original plan bases (`79e2903` in EngArg and `520abf0` in EAL), the frozen
completion baselines, and the current working tree. It excludes generated
parsers, vendored code and test directories. Added, modified and removed file
counts describe source-change fan-out; they do not measure runtime work or
maintainability. No old workflow timing or historical authoring-call count is
inferred from source. Latency samples from this small scenario establish neither
performance superiority nor general performance equivalence.

The retained 27 October run is in [measurement.md](../benchmarks/eal4-20261027-final/measurement.md), with complete per-invocation JSON and request logs beside it.
