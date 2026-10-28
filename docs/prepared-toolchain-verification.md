# Prepared toolchain verification

[Library selection and build instructions](library-selection.md#prepare-the-dependencies)

Recorded on **28 October 2026 (UTC)** for Linux x86_64. All five language
profiles were built and their checked-in API smoke sources executed successfully
in external Docker containers. The JavaScript and TypeScript tags share one
image, so this represents four distinct immutable image identities.

This record covers dependency preparation and local API behaviour. The native
`DockerRunner` integration tests and the fresh generated-tool claim investigation
are separate checks, pending at the time of this record. A local smoke result does
not establish that generation, verification, export, reuse or claim assessment
has completed.

## Images actually tested

Sizes are Docker's uncompressed image sizes, not download sizes or incremental
disk requirements. These are local image IDs, not registry publication digests.
Tags can move; retain the immutable identity when reproducing an investigation.

| Profile | Built tag | ID below | Bytes |
|---|---|---|---:|
| [Python evidence](../examples/toolchains/python-evidence.json) | `engarg-python-evidence:0.14-prepared` | P | 1,318,110,018 |
| [Rust](../examples/toolchains/rust.json) | `engarg-rust-tools:1.99.1` | R | 2,216,594,115 |
| [Go](../examples/toolchains/go.json) | `engarg-go-tools:1.25.14` | G | 391,599,668 |
| [JavaScript](../examples/toolchains/javascript.json) | `engarg-javascript-tools:24` | N | 300,609,106 |
| [TypeScript](../examples/toolchains/typescript.json) | `engarg-typescript-tools:5.9.3` | N | 300,609,106 |

```text
P sha256:5d317440653dd5a188223baff22644397cc2a1bd071183a12e9c4cb2f3688f7f
R sha256:3c55eb0f6e9e4a914621fee04094e8c5b71b43373da135fe1341fc1cedad10f4
G sha256:c677125f2d9b66676c491c0a07576e63e43a535c997c1d1c888c8fac39216b5f
N sha256:936b874e81540c36eb105e962f24542c53a4316e764c0690fd81420c2d753cb9
```

## Prepared versions

- **Python 3.13.18:** DoWhy 0.14, causal-learn 0.1.4.8, pgmpy 1.1.2,
  Z3 Solver 5.3.2.0, SciPy 1.18.1, statsmodels 0.15.0,
  scikit-learn 1.9.1 and SymPy 1.14.0. The
  [wheel lock](../examples/toolchains/python-evidence/requirements.lock)
  pins all 65 distributions and their hashes, preserving the original 49
  scientific pins. pgmpy uses its NumPy backend; Torch, Pyro and CUDA are absent.
- **Rust 1.99.1, edition 2024:** why_rs 0.2.1, Polars 0.52.0,
  Statrs 0.19.1, Nalgebra 0.35.0, Varisat 0.2.2, linfa-nn 0.8.1,
  ndarray 0.16.1 and good_lp 1.14.2, plus serde 1.0.229,
  serde_json 1.0.154 and reqwest 0.13.5. See the
  [manifest](../examples/toolchains/rust/Cargo.toml) and
  [lock](../examples/toolchains/rust/Cargo.lock).
  good_lp selects the pure Rust `microlp` backend, without system CBC.
  Version 1.14.2 preserves compatibility: good_lp 1.15 and later require a
  microlp dependency using `sprs >= 0.11.4`, while linfa 0.8.1 pins `sprs 0.11.2`.
- **Go 1.25.14:** CausalGo 0.6.3, Gophersat 1.4.0 and Gonum 0.17.0,
  with causalgo/lasso 0.2.1 and x/tools 0.30.0 in the
  [module closure](../examples/toolchains/go/go.mod).
  [Checksums](../examples/toolchains/go/go.sum) and prepared import archives
  support offline compile/link execution.
- **Node 24.22.1 configured in the image; TypeScript 5.9.3:** Math.js 15.2.0,
  Simple Statistics 7.12.1, Logic Solver 2.0.1, ml-distance 5.0.0,
  javascript-lp-solver 1.0.3, `@types/node` 24.10.1 and libdagitty source
  revision `1f3d1463285315deda09b5454dc56338bf379cbc`.
  The [shared npm lock](../examples/toolchains/typescript/package-lock.json)
  records integrity checks for 91 package archives, including the source tarball.
  TypeScript compiles with strict checks and checked declarations for untyped
  interfaces. The Node patch version comes from the inspected image configuration
  (`NODE_VERSION`); a separate executed `node --version` result was not retained.

## External smoke results and limits

All external probes exited zero and reached `evidence-packages-ok`. They used
network isolation, UID 65534, a read-only root filesystem and read-only source
mounts. Writable compilation output and temporary files used bounded scratch
mounts. Memory limits were Python 1 GiB, Rust 1 GiB, Go 512 MiB,
JavaScript 128 MiB and TypeScript 512 MiB. The Go, Rust and Node probes also
explicitly dropped capabilities and enabled `no-new-privileges`.

| Source | Calculations exercised |
|---|---|
| [Python](../examples/toolchains/python-evidence/smoke.py) | DoWhy graph identification, linear effect estimation and refutation; causal-learn PC; pgmpy posterior `9/13`, MAP and rejection of zero evidence mass; fixed-equation counterfactual `Y: 7 -> 13` preserving exogenous noise; Wilson interval, binomial test, held-out prediction, nearest neighbour, linear-program optimum, Z3 SAT/UNSAT and symbolic roots. |
| [Rust](../examples/toolchains/rust/smoke.rs) | why_rs DAG and PC construction with a real Polars table; binomial probability mass; affine solve; SAT/UNSAT; nearest neighbour and microlp optimum; serde JSON preservation and offline HTTP-client construction. PC construction does not test a full discovery run. |
| [Go](../examples/toolchains/go/smoke.go) | Statistical mean and probability functions, matrix solve, nearest neighbour, optimisation, SAT/UNSAT, CausalGo SURD/SCIC calculations and VarSelect ordering. |
| [JavaScript](../examples/toolchains/typescript/evidence-smoke.mjs) / [TypeScript](../examples/toolchains/typescript/evidence-smoke.ts) | Arithmetic, linear solve, sample mean/variance, SAT and contradiction, distance and tree APIs, LP optimum and infeasibility, and Dagitty adjustment with/without a confounder. Strict TypeScript compilation and JavaScript ESM resolution also passed; source/dependency write attempts failed. |

These are synthetic software controls. They do not qualify an external causal
model, sampling design, candidate-hypothesis set or engineering claim. Package
recommendations are a curated dependency closure rather than a universal ranking.
An operator override, optional backend or package outside this closure requires
an explicit trusted rebuild and fresh checks. Generation and runtime execution
perform no dependency installation; preparing a package grants no new network,
filesystem or credential authority.

The checked-in [native integration tests](../tests/prepared_toolchains.rs) run
these sources through EngArg's actual isolated runner after the images exist:

```sh
cargo test --locked --test prepared_toolchains -- --ignored
```

That result must be reported separately from these external Docker probes and
from any subsequent generated-tool claim investigation.
