# Select libraries for generated evidence tools

[Documentation](index.md) · [Tool implementation](TOOL_IMPLEMENTATION.md)

An evidence operation may need a statistical estimator, a graph algorithm or a
solver. Configure its libraries in the language's operator-owned toolchain JSON.
EngArg supplies task-specific recommendations, lets an operator replace them,
and lets the implementation model choose when no matching recommendation remains.
These instructions guide generated source. They do not install dependencies or
change the evidence that the operation must obtain.

## Configure selection

The authoring profile uses `engarg.toolchain/2`. Add this member to a complete
profile, such as [the Python example](../examples/toolchains/python.json):

```json
"generation": {
  "libraries": {
    "use_defaults": true,
    "fallback": "model",
    "tasks": {
      "causal_testing": [
        {
          "package": "dowhy",
          "version": "0.14",
          "reference": "https://github.com/py-why/dowhy",
          "guidance": "Use refutation or graph falsification for the explicit assumptions; retain unresolved assumptions."
        }
      ],
      "similarity": []
    }
  }
}
```

The precedence is explicit:

1. An operator `tasks` entry replaces the corresponding language default. Its
   package names, optional versions and guidance enter the generation request.
   An empty array disables that task's recommendation.
2. With `use_defaults: true`, other tasks retain the bundled recommendations.
3. For an empty or unmatched task, `fallback: "model"` permits the model to
   choose a suitable available library or the standard library. Set
   `fallback: "standard_library"` to permit only the standard library for those
   tasks. Fallback does not override an explicit task rule.

Task labels describe operations, rather than asserting that a library implements
an entire reasoning mode. Custom labels may use letters, digits, `_`, `-` and
`.`; keep their meaning explicit in the operation specification. A rule can list
complementary packages with different roles. Python causal discovery, for
example, pairs a discovery algorithm with DoWhy modelling and assumption checks.
It does not treat those packages as interchangeable.

[The prepared Python evidence profile](../examples/toolchains/python-evidence.json)
demonstrates explicit package/version overrides. Its defaults are disabled so
the supplied rules describe its prepared scientific environment.
[The model-selected Python profile](../examples/toolchains/python-model-selected.json)
has no defaults or task overrides; it demonstrates model selection within a
standard-library image.

Legacy `engarg.toolchain/1` profiles without `generation` remain accepted. To add
generation settings, change the authoring schema to `/2`. EngArg validates that
profile and projects its unchanged execution fields to the shared `/1` runtime
profile. Exported tools and EAL do not consume the authoring-only settings as
runtime execution authority.

## Defaults by evidence operation

The bundled catalogue is
[library_defaults.json](../src/tooling/library_defaults.json). A dash means no
verified default is supplied and the configured fallback applies. The JavaScript
and TypeScript recommendations share runtime packages; TypeScript still needs
compatible declarations and module settings.

| Task | Python | Rust | Go | JavaScript / TypeScript |
|---|---|---|---|---|
| `causal_discovery` | `causal-learn` plus `dowhy` | `why_rs` | `github.com/causalgo/causalgo` | — |
| `causal_modelling` | `dowhy` | `why_rs` | — | — |
| `causal_identification` | `dowhy` | — | — | `libdagitty` source |
| `causal_estimation` | `dowhy` | — | — | — |
| `causal_testing` | `dowhy` | — | — | — |
| `counterfactual` | `dowhy` GCM | `deep_causality` | — | — |
| `statistical_inference` | `scipy`, `statsmodels` | `statrs` | Gonum `stat` | `simple-statistics` |
| `formal_verification` | `z3-solver`, `sympy` | `varisat` | `github.com/crillab/gophersat` | `logic-solver` |
| `probabilistic_inference` | `pgmpy` | `statrs` primitives | Gonum `stat/distuv` primitives | `mathjs` primitives |
| `similarity` | `scikit-learn` | `linfa-nn` | Gonum `spatial/kdtree` | `ml-distance` |
| `optimisation` | `scipy.optimize` | `good_lp` | Gonum `optimize` | `javascript-lp-solver` |

Each catalogue entry links to its source and states its limited role. For
example, SAT libraries check authored propositional encodings, not arbitrary
SMT or mathematical proofs. Distribution functions supply numerical primitives,
not a general abduction engine. Similarity calculations leave transfer and
analogy applicability as separate obligations. Optimisation needs an authored
objective, constraints and policy; infeasibility, numerical limits and exhausted
computation remain distinct results.

For causal work, preserve more specific distinctions:

- [DoWhy](https://github.com/py-why/dowhy) models, identifies, estimates and refutes
  causal claims. Its current `CausalModel` documentation deprecates `learn_graph`
  and directs discovery through a separate package such as
  [causal-learn](https://github.com/py-why/causal-learn). Preserve discovered
  equivalence classes, unresolved orientations and the assumptions behind them.
  A passing refutation or falsification check does not prove all causal assumptions.
- [why_rs](https://github.com/jwhogg/why-rs) supplies DAGs, functional causal
  models, PC discovery and interventions. Its Cargo name is `why_rs`.
  [deep_causality](https://github.com/deepcausality-rs/deep_causality) supports
  dynamic causal models and what-if execution. Neither is a complete DoWhy API
  replacement, and model interventions do not identify an external causal effect.
- [CausalGo](https://github.com/causalgo/causalgo) supplies time-series
  decomposition and discovery operations with their own assumptions. It does
  not supply a general graph-identification/refutation workflow.
- [libdagitty](https://github.com/jtextor/libdagitty) supplies JavaScript graph
  adjustment, separation and identification criteria. It does not estimate
  numerical effects. The npm registry had no `libdagitty` package when checked;
  prepare a pinned source build and TypeScript adapter rather than assuming
  `npm install libdagitty` works. Source entry point: `src/index.js`.
- [causalkit](https://github.com/rakutentech/causalkit) is a Rust implementation
  exposed through a PyO3 Python extension. Its documented installation is through
  pip, and its `cdylib` is not a verified ordinary Rust dependency. The published
  Rust crate named `smelt` is a metabuild tool, not a causal-forest implementation.
  Neither name is supplied as a Rust default.

These package identities and interfaces were checked against their upstream
sources and registries on 28 October 2026. Default recommendations omit version
pins because the operator must choose versions compatible with the actual
prepared environment. The optional scientific image below pins its own complete
dependency set.

## Prepare the dependencies

The base language profiles remain small: Python and JavaScript have their
standard libraries; Rust adds its declared transport dependencies; Go exposes a
precompiled standard library; TypeScript adds the compiler and Node declarations.
Enabling recommendations does not assert that third-party packages are installed.

An operator prepares another trusted image when an operation needs those
packages. Pin dependency versions and lockfiles during that build. Rust needs the
prepared archives and explicit `--extern` arguments used by its fixed wrapper;
Go's wrapper needs precompiled modules and its import configuration. TypeScript
needs both declarations and working runtime module resolution. A package name in
the policy does not extend any of those interfaces. For example, `good_lp` needs
an explicitly selected prepared solver backend; its default backend requires CBC.

The optional [Python evidence image](../examples/toolchains/python-evidence/Dockerfile)
uses a digest-pinned Python base and hash-pinned direct and transitive wheels for
CPython 3.13 on Linux x86_64. It includes DoWhy 0.14, causal-learn 0.1.4.8, SciPy,
statsmodels, scikit-learn and SymPy. Build it before generation:

```sh
docker build --platform linux/amd64 --tag engarg-python-evidence:0.14 \
  examples/toolchains/python-evidence

engarg eal toolchain-check --toolchain examples/toolchains/python-evidence.json \
  --output python-evidence-check.json

docker run --rm --network none --memory 1g \
  --mount type=bind,src="$PWD/examples/toolchains/python-evidence/smoke.py",dst=/smoke.py,readonly \
  engarg-python-evidence:0.14 python3 -I -B /smoke.py
```

A managed proxy environment can pass its public CA with
`--secret id=proxy_ca,src="$CODEX_PROXY_CERT"` during the trusted build. The CA
mount is temporary and TLS verification stays enabled. Generated tools install
nothing, have no fixture network access, and retain the selected immutable image
identity. Inspect the local image ID and use it in the investigation's operator
profile when reproducing that exact environment.

The smoke check fits a supplied synthetic causal model, identifies and estimates
its effect, runs a refutation, and invokes causal-learn's PC operation. It checks
software availability and API execution only. Its synthetic data and known
construction do not qualify a method for an external population.

Use the prepared profile with the ordinary implementation CLI:

```sh
engarg eal implement --input analysis.json --tools tools.toml \
  --model "$ENGARG_CODING_MODEL" --api-key-env OPENAI_API_KEY \
  --eal-host "$EAL_HOST" --tool-language python \
  --toolchain examples/toolchains/python-evidence.json \
  --tool-store .engarg-tools --tool-output-dir implemented
```

The resolved policy enters the retained frozen generation context and the
generated package's cache identity. It does not change the authored logical
operation's identity or acceptance criteria. Review the generated source and
frozen-case receipt to check that it used the
requested library appropriately. A library policy is a generation instruction,
not an import allowlist or scientific qualification. It supplies no network,
filesystem, credential or installation authority and does not change native EAL
reasoning methods, evidence scope, independent reviews or claim assessment.
