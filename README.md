# EngArg

EngArg turns an engineering claim into an argument with explicit evidence
obligations. You supply the source tools and execution authority. EngArg builds
an EAL/4 package, and the paired EAL runtime gathers observations and assesses
what they support.

The [documentation](docs/index.md) follows a CI parallelisation claim from its
initial argument to evidence collection, assessment and revision. Start with
[the engineer's guide](docs/USAGE.md) for installation and the complete command
sequence, including Rust, Go and TypeScript tool generation.

An investigation executes from its EAL source, TOML tool bindings and referenced
implementations:

```sh
engarg build --input analysis.json --tools operator-tools.toml --output-dir investigation
eal-host run investigation/case.eal --tools investigation/tools.toml --workspace run
```

The assessment preserves missing evidence, measured negatives and unresolved
reviews. A completed execution may conclude that the claim remains unsupported.

The [recorded Bayesian diagnosis example](examples/assess-claim/bayesian-diagnosis/README.md)
includes real model-generated pgmpy tooling, native verification, scoped reviews
and a completed EAL-host assessment, with measured-negative and missing-binding
controls. [Prepared language images](docs/prepared-toolchain-verification.md)
supply the pinned evidence packages before execution.

Browse the [HTML argument examples](examples/assess-claim/index.html) to inspect
the qualified investigations and the historical unsupported argument.

For contributors, [architecture](docs/EXECUTABLE_ARCHITECTURE.md) explains the
responsibility boundaries and [verification](docs/VALIDATION.md) gives the
repository and installed-runtime checks. [Evaluation](docs/EVALUATION.md)
separates those software checks from evidence about argument quality or cost.

GPL-3.0-only licensed. Model service, API access and paid inference are separate.
