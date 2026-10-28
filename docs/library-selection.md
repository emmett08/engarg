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
| `causal_identification` | `dowhy` | — | — | `jtextor/libdagitty` pinned source |
| `causal_estimation` | `dowhy` | — | — | — |
| `causal_testing` | `dowhy` | — | — | — |
| `abductive` | `pgmpy` | `statrs` likelihood primitives | Gonum `stat/distuv` primitives | `mathjs` probability arithmetic |
| `inductive` | `scipy`, `scikit-learn` | `statrs` | Gonum `stat` | `simple-statistics` |
| `counterfactual` | `dowhy` GCM | `nalgebra` affine-model primitives | Gonum `mat` affine-model primitives | `mathjs` affine-model primitives |
| `statistical_inference` | `scipy`, `statsmodels` | `statrs` | Gonum `stat` | `simple-statistics` |
| `formal_verification` | `z3-solver`, `sympy` | `varisat` | `github.com/crillab/gophersat/solver` | `logic-solver` |
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

### Abductive, inductive and counterfactual requirements

These three task labels provide defaults for every supported language. They
describe families of evidence operations; selecting a label does not select or
replace EAL's native reasoning method. Native Bayesian conditioning, Wilson
intervals and fixed-exogenous affine reasoning retain their existing contracts.
A generated tool may supply their observations or model inputs, or perform a
separately specified calculation. It must not duplicate or change the authored
native method merely because a library offers another computation.

`abductive` covers evidence for comparing explicitly supplied explanations.
Python's pgmpy supplies conditional posterior and MAP queries over a declared
Bayesian network. Rust's Statrs and Go's Gonum distributions supply likelihood
primitives; Math.js supplies arithmetic over declared prior/likelihood vectors.
Those primitives do not discover hypotheses or validate their completeness.
Require a nonzero finite evidence normaliser, retain alternative hypotheses and
likelihood uncertainty, and distinguish probability mass from a continuous
density. The highest posterior is conditional on the candidate set and model.

`inductive` covers sampled observations, statistical estimates and, when
requested, predictive fitting or validation. SciPy supplies statistical routines;
scikit-learn supplies learning and held-out validation. Statrs, Gonum and Simple
Statistics supply numerical statistical operations. Preserve the population,
trial identities, exclusions, sampling design and uncertainty. Fitting training
data does not establish generalisation; predictive operations need explicit
split identities, leakage controls and actual held-out observations. Do not
replace an authored Wilson interval with another interval.

`counterfactual` requires a supplied structural model and the same factual
exogenous context after the intervention. DoWhy's
[`gcm.counterfactual_samples`](https://www.pywhy.org/dowhy/main/dowhy.gcm.html#dowhy.gcm.whatif.counterfactual_samples)
implements abduction, intervention and prediction. Reconstructing noise from
observed data requires invertible mechanisms; justified noise data can instead
be supplied. Nalgebra, Gonum `mat` and Math.js are numerical primitives for
explicit bounded affine models, not general causal engines. Preserve the model,
replace the intervened equation, and check dimensions, admissibility, unique
solvability, conditioning and residuals under the authored numerical limits.
Math.js `lusolve` can return one solution to a consistent singular system; a
returned vector alone does not establish uniqueness. Reject unsupported or
ambiguous models. Observational regression, interventional population samples
and feature changes that alter a predictor's output do not by themselves supply
an individual structural counterfactual.

Task overrides remain exact: overriding `abductive` does not rewrite
`probabilistic_inference`, and overriding `inductive` does not rewrite
`statistical_inference`. Configure each applicable task explicitly when an
operator wants the same restriction across families; there is no hidden alias
or precedence router. `use_defaults: false` disables all bundled families, and
an empty task rule still selects the configured fallback.

The combined resolved policy retains its limits of 32 task rules, 64 package
choices and 16 KiB. New bundled tasks count toward those limits. A large operator
policy can disable defaults and supply only its required rules. Catalogue changes
also change the frozen generation context and cache identity; earlier archived
packages and their recorded policies are not rewritten.

The [selection review](library-selection-review.md) records current versions,
public uptake indicators, API sources and alternatives checked on 28 October
2026. Popularity informs the recommendation; operation fit and evidential limits
determine which API can be used. Versions in that review are a dated snapshot,
not permission to install the latest release during generation.

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
  The bounded affine counterfactual default now uses the more widely used
  Nalgebra numerical primitives. An operator can still select DeepCausality when
  the operation explicitly requires its causal-flow framework.
- [CausalGo](https://github.com/causalgo/causalgo) supplies time-series
  decomposition and discovery operations with their own assumptions. It does
  not supply a general graph-identification/refutation workflow.
- [libdagitty](https://github.com/jtextor/libdagitty) supplies JavaScript graph
  adjustment, separation and identification criteria. It does not estimate
  numerical effects. The npm registry has neither `libdagitty` nor `dagitty`.
  The default is the source package `jtextor/libdagitty`, pinned to revision
  `1f3d1463285315deda09b5454dc56338bf379cbc`. Prepare that source and its lockfile
  in the trusted image, then expose `libdagitty/src/index.js` with a checked
  TypeScript adapter where needed. The upstream package's root `main` points to
  an absent `index.js`, so a bare `require("libdagitty")` is not a valid entry.
  The pinned `src/index.js` API was executed against a confounded DAG: adjusting
  for the confounder succeeds and omitting it fails.
- [causalkit](https://github.com/rakutentech/causalkit) is a Rust implementation
  exposed through a PyO3 Python extension. Its documented installation is through
  pip, and its `cdylib` is not a verified ordinary Rust dependency. The published
  Rust crate named `smelt` is a metabuild tool, not a causal-forest implementation.
  Neither name is supplied as a Rust default.

The Go formal-check default uses `github.com/crillab/gophersat/solver`.
The module root is `package main`, so `github.com/crillab/gophersat` itself is
not an importable solver library. The published solver subpackage and the Gonum
subpackages in the matrix were checked directly in their module archives.

These package identities and interfaces were checked against their upstream
sources and registries on 28 October 2026. Registry recommendations omit version
pins because the operator must choose versions compatible with the actual
prepared environment; source-only defaults carry an immutable revision. The
optional scientific image below pins its own complete dependency set.

## Prepare the dependencies

The prepared images supply the package families in the bundled catalogue before
generation or execution. This is a bounded, pinned set: adding an arbitrary
operator override still requires preparing its dependencies. The plain Python
and model-selected Python profiles remain available for standard-library work.
Library preferences alone never install a package.

The [prepared-toolchain verification record](prepared-toolchain-verification.md)
lists the built image identities, tested APIs and limits of those checks.

| Prepared profile | Included evidence packages |
|---|---|
| `python-evidence.json` | DoWhy, causal-learn, pgmpy (NumPy backend), SciPy, statsmodels, scikit-learn, SymPy and Z3 |
| `rust.json` | why_rs, Polars, Statrs, Nalgebra, Varisat, linfa-nn, ndarray and good_lp with the pure Rust microlp backend; serde/serde_json/reqwest remain available |
| `go.json` | Gonum statistics, distributions, matrices, nearest neighbours and optimisation; Gophersat and CausalGo's public inference packages |
| `javascript.json`, `typescript.json` | Math.js, Simple Statistics, Logic Solver, ml-distance, javascript-lp-solver and the pinned libdagitty source |

Build the image for the language you need from the repository root:

```sh
docker build --platform linux/amd64 --tag engarg-python-evidence:0.14-prepared \
  examples/toolchains/python-evidence
docker build --tag engarg-rust-tools:1.99.1 examples/toolchains/rust
docker build --tag engarg-go-tools:1.25.14 examples/toolchains/go
docker build --target javascript --tag engarg-javascript-tools:24 \
  examples/toolchains/typescript
docker build --tag engarg-typescript-tools:5.9.3 examples/toolchains/typescript

engarg eal toolchain-check --toolchain examples/toolchains/python-evidence.json \
  --output python-evidence-check.json
```

A managed HTTPS proxy can pass its public CA with
`--secret id=proxy_ca,src="$CODEX_PROXY_CERT"` during the trusted image build.
The temporary secret does not enter image layers and TLS verification stays
enabled. Runtime profiles use no network and inherit no credentials. The runner
requires a local image, resolves its immutable identity and uses `--pull never`.
It executes the configured build check before the generated tool; it performs no
automatic dependency installation. A new image identity is a new execution
environment, so archived investigations retain their original image pins.

Rust's trusted build freezes exact direct-dependency archives for the fixed
`rustc` wrapper; this also disambiguates different Nalgebra versions in the
transitive closure. Go prepares module archives and an immutable import map.
JavaScript and TypeScript share a locked package closure and immutable module
resolution from the read-only source mount. TypeScript compiles strictly with
checked declarations for the untyped solver/graph interfaces. Preparation covers
usable imports and linkage, not merely downloaded files.

The Rust profile explicitly selects good_lp 1.14.2 with microlp: the newer
microlp dependency conflicts with the sparse-matrix version required by the
prepared causal stack. No system CBC backend is required. Python preserves the
existing scientific pins and adds pgmpy 1.1.2 and Z3 5.3.2.0; pgmpy's optional
Torch/Pyro/GPU stack is unnecessary for the supplied finite Bayesian queries.
The Python wheel lock targets CPython 3.13 on Linux x86_64. Node packages and the
libdagitty source archive have lockfile integrity checks; each language recipe
retains its full dependency lock.

Each image has API smoke sources with positive and adverse mathematical controls.
`tests/prepared_toolchains.rs` runs those sources through the actual isolated
runner with the canonical profile, including compilation, read-only source
mounts, runtime resource limits and network isolation. CI builds each language
family separately and also runs the existing generated-tool protocol,
verification, export, execution and reuse tests. These checks establish software
availability and tested API behaviour; they do not establish external model
adequacy, causal assumptions or independent judgement.

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
