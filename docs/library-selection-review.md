# Evidence-library selection review

[Library configuration and matrix](library-selection.md)

Reviewed on **28 October 2026** against primary registries, upstream repository
metadata and API documentation. The recommendations prefer established packages
that fit the requested operation and are available in the language. They are
not a claim that one package is the most popular under every possible measure.
GitHub stars indicate attention; registry downloads include automated builds.
Neither establishes algorithmic correctness, model adequacy or independent use.

| Default package | Current stable observed | Public uptake indicator | API role |
|---|---|---|---|
| [pgmpy](https://pypi.org/project/pgmpy/1.1.2/) | 1.1.2 | 3,363 GitHub stars | Conditional posterior/MAP in an authored discrete Bayesian model |
| [SciPy](https://pypi.org/project/scipy/1.18.1/) | 1.18.1 | 15,161 GitHub stars | Statistical inference and explicitly specified likelihood calculations |
| [scikit-learn](https://pypi.org/project/scikit-learn/1.9.1/) | 1.9.1 | 67,969 GitHub stars | Predictive learning, splitting and validation |
| [DoWhy](https://pypi.org/project/dowhy/0.14/) | 0.14 | 8,377 GitHub stars | Structural counterfactual abduction, intervention and prediction |
| [Statrs](https://crates.io/crates/statrs/0.19.1) | 0.19.1 | 44.47 million cumulative crate downloads; 835 GitHub stars | Distribution masses, statistical summaries and uncertainty primitives |
| [Nalgebra](https://crates.io/crates/nalgebra/0.35.0) | 0.35.0 | 98.18 million cumulative crate downloads; 4,807 GitHub stars | Bounded supplied affine-model calculations |
| [Gonum](https://pkg.go.dev/gonum.org/v1/gonum@v0.17.0) | v0.17.0 | 8,452 GitHub stars | `stat/distuv`, `stat` and `mat`, with separate likelihood/statistical/matrix roles |
| [Math.js](https://www.npmjs.com/package/mathjs/v/15.2.0) | 15.2.0 | 14.91 million npm downloads in the observed month; 15,098 GitHub stars | Explicit probability-vector and bounded affine-model arithmetic |
| [Simple Statistics](https://www.npmjs.com/package/simple-statistics/v/7.12.1) | 7.12.1 | 4.86 million npm downloads in the observed month; 3,527 GitHub stars | Statistical summaries, regression and declared tests |

The npm window was 27 September–26 October 2026. Sources are the official
[Math.js](https://api.npmjs.org/downloads/point/2026-09-27:2026-10-26/mathjs)
and [Simple Statistics](https://api.npmjs.org/downloads/point/2026-09-27:2026-10-26/simple-statistics)
download APIs. Crate totals come from the official
[Statrs](https://crates.io/api/v1/crates/statrs) and
[Nalgebra](https://crates.io/api/v1/crates/nalgebra) registry APIs. Go's
[module proxy](https://proxy.golang.org/gonum.org/v1/gonum/@latest) supplied the
tagged release identity. Each catalogue entry links its upstream; GitHub's
repository metadata supplied star/activity counts. All selected repositories
were unarchived and had observed pushes during August–October 2026. These live
sources may change; the figures above retain the review date.

## Fit before a popularity ranking

For Python abduction, [PyMC](https://github.com/pymc-devs/pymc) and
[Pyro](https://github.com/pyro-ppl/pyro) are established alternatives for broader
probabilistic programming. pgmpy's
[`VariableElimination.query` and `map_query`](https://pgmpy.org/api/generated/inference/pgmpy.inference.VariableElimination.html)
fit the supplied finite-hypothesis/discrete-model task directly. A larger
framework or more stars does not remove the prior, likelihood or candidate-set
obligations. The prepared-image SciPy override has a narrower contract and does
not pretend that pgmpy is installed.

For induction, SciPy's
[`binomtest` confidence intervals](https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats._result_classes.BinomTestResult.proportion_ci.html)
and scikit-learn's
[cross-validation guidance](https://scikit-learn.org/stable/modules/cross_validation.html)
serve different operations. Rust [Linfa](https://github.com/rust-ml/linfa) and
[SmartCore](https://github.com/smartcorelib/smartcore) remain useful operator
choices for actual predictive learning. Statrs is the smaller statistical
default for acquired counts and explicitly specified distributions. Merely
counting trials may need only the standard library.

For Rust counterfactual inputs, [DeepCausality](https://github.com/deepcausality-rs/deep_causality)
is active, but its causal-flow framework is not an interchangeable implementation
of a supplied affine SCM. Nalgebra has substantially broader recorded use and
a [checked linear-solve API](https://docs.rs/nalgebra/0.35.0/nalgebra/linalg/struct.LU.html#method.solve).
Go's [matrix API](https://pkg.go.dev/gonum.org/v1/gonum/mat) similarly exposes
conditioning failures. These are numerical defaults for the bounded authored
model, not replacements for DoWhy's general GCM workflow or evidence of an
external causal relation.

For JavaScript/TypeScript, the inspected `bayesjs` release was from 2021 and
had approximately 712 downloads in the observed month; `@bayesjs/core` and
`bayesnet` were not published under those exact names. Math.js is the established
arithmetic choice for a supplied finite model. It and Simple Statistics include
TypeScript declarations. TensorFlow.js can fit an explicitly requested predictive
task but is unnecessary for simple statistical inputs. A constraint solver such
as Z3 requires a separately justified formal encoding and prepared runtime.

DoWhy's
[`counterfactual_samples`](https://www.pywhy.org/dowhy/main/dowhy.gcm.html#dowhy.gcm.whatif.counterfactual_samples)
retains factual conditioning. [DiCE](https://github.com/interpretml/DiCE)
generates changes to predictive features and is not selected as a substitute for
structural counterfactual evidence. Likewise graph adjustment criteria or an
interventional sample are not an individual counterfactual.

## Verification boundary

Package identities, published versions, imports/exports and documented numerical
failure modes were checked. Pinned Math.js and Simple Statistics APIs executed
synthetic finite-Bayes, zero-mass, affine intervention/no-op, singular-system and
statistical/regression controls. Math.js can return a solution for a consistent
singular system, so the authored model must establish unique admissible
solvability separately from solver success and residual checks. A determinant
check for a small integer example is not a general numerical conditioning test.

Registry/source inspection and these finite API controls do not qualify every
language toolchain or external model. Operators must prepare exact compatible
dependencies and run native frozen verification for each generated tool. Model
choices, uncertainty, numerical tolerances, original observations and independent
reviews retain their own obligations. No recommendation grants installation,
network, shell or credential authority.
