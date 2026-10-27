# Executable evidence architecture

EngArg owns argument construction, explicit revision, tool construction and
compilation. EAL owns the language, method contracts, acquisition, observation
storage and assessment. Operator TOML supplies concrete execution authority.

## One semantic program

The compiler emits EAL/3 with the additive `execution/1` profile. Evidence `bind`
fields express input dependencies; `derive` expresses bounded pure calculations
and separate acceptance checks over shared observations. Claims, methods,
assumptions, backing and objections are in the same source. A derived result is
recomputed against its retained parents during assessment and keeps their original
age. Reusing one observation does not create independent evidence.

The build receipt records source and executable-program digests separately.
Removing it does not remove executable semantics. Static formal queries are
authored program data. Tool implementations supply empirical inputs, not echoed
copies of the query. An unsupported conclusion remains distinct from failed tool
execution and from a completed measurement that contradicts a predicate.

## SOLID in the Rust boundaries

| Principle | Concrete responsibility | Verification |
|---|---|---|
| Single responsibility | Grammar/lowering, pure evaluation, tool transport, observation persistence and argument compilation are separate modules | Parser, method, adapter and package tests exercise each boundary |
| Open–closed | Concrete tool transports share the acquisition contract; method queries use EAL-owned descriptors | Command/MCP/HTTP/file substitution cases and descriptor admission |
| Liskov substitution | Every adapter preserves request identity, observation age, failures and bounded output | Real subprocess and loopback HTTP tests, stale and malformed observations |
| Interface segregation | Authoring sees operation/schema metadata; runtime receives resolved transport authority | Prompt tests exclude argv, endpoints and credential values |
| Dependency inversion | Pure core has no IO; reusable MCP client has no dependency on the EAL server; EngArg consumes EAL contracts | Cargo dependency graph and independent EAL package execution |

The compiler pipeline separates acquisition lowering from obligation and argument
emission. Transport adapters own their external protocols. Reasoning methods are
strategies behind the same typed input/result contract. Acquisition requests are
bounded command data, and RAII brackets process cancellation, store leases and
package publication. These patterns have concrete variation or lifetime duties;
there is no global service locator or abstract factory for a single implementation.

## Maintenance and migration

The appended support-gate projection, graph-JSON executor and `engarg_support_graph`
tool have been removed. Frozen legacy source-reconstruction modules and the old
supervisor executable alias have been removed. The canonical companion is
`eal-process-supervisor`. Historical observations remain immutable data; a new
argument revision invalidates derived plans and assessments rather than rewriting
their identities or timestamps.

Optional discovery and qualification tools may help author an investigation.
They do not grant authority to execute a newly discovered operation, and their
catalogues are not required by a resolved package. Missing reviewer judgement is
represented by a scoped review request, not by an automatically accepted Boolean.

## Measured scope

The acceptance suite uses real MCP stdio and HTTP, supervised commands, retained
files, copied packages and independently stated numeric/logical outcomes. It
covers native deductive, inductive, abductive, causal, counterfactual and bounded
analogical methods, plus practical reasoning through an explicit policy review.
The synthetic finite-inventory example checks count, distinctness and explicit
population scope. These tests establish the implemented boundaries, not the
truth of arbitrary engineering premises, completeness of an external catalogue
or universal adequacy of analogy and decision policies.

Rust edition 2024 and each repository's pinned minimum toolchain remain in force.
Formatting, Clippy, workspace tests, parser regeneration and installed CLI/MCP
smoke checks are required. Model authoring quality and cost require separately
frozen trials with actual selected providers; deterministic protocol tests do
not claim those results.
