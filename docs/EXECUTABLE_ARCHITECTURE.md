# Executable evidence architecture

EngArg owns argument construction, explicit revision, tool construction and
compilation. EAL owns the language, method contracts, acquisition, observation
storage and assessment. Operator TOML supplies concrete execution authority.

## One semantic program and compiler

The compiler emits `language "EAL/4"`. This version defines executable semantics
directly; it has no execution selector. Evidence `bind`
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

`engarg build` and `engarg eal export` call the same package builder and
`support_export::compile_direct`. Acquisition lowering, exact requirement checks,
native method applications and argument construction are passes within this
compiler. There is no public authored-only exporter or second bundle-obligation
model. EAL's method descriptors define query shapes and result paths; EngArg does
not maintain an independent method schema registry.

The runtime executes the selected claim's dependency closure. Each source
operation has a stable identity and each canonical requirement has one evaluator.
Shared source observations remain shared, including their age and provenance.
Assessment checks retained derivations against their parents; this integrity
check does not introduce another authored obligation.

## Authoring, revision and generated tools

The current report is the strict `engarg.run.v3` contract. The public authoring,
tool implementation, revision, ticket and build paths consume this contract.
Direct support, bindings, acquisition design review, target-runtime receipt and
revision history are typed fields. Optional graph discovery can supply authored
support contracts; operator TOML determines the concrete execution bindings.

An explicit revision preserves the original request, authoring exchanges and
dissent. Each bounded revision record retains its exact parent identity and
source snapshot. Changed executable arguments require a complete support
elaboration; changed requirements identify their previous digest and revision
reason. The host revalidates support and recomputes acquisition design review.
Bindings are resolved from the supplied TOML or marked unbound, and previous
tooling, collection and assessment claims cannot silently attach to the revision.
The resulting report can be revised again and passed directly to the same builder.
For coordinated authoring, replay uses the configured participants and rounds,
preserves parallel arrival accounting and retains each participant/round's
objection identities even when several participants share the same role.

Missing operations with an authored parameterised specification use the existing
isolated tool builder and frozen verification fixtures. The direct generation
stage exports verified implementations, merges their bindings with the operator's
manifest and preserves the source report and generation audit. Both
`analyse --implement-tools` and `eal implement` use this stage; ticket execution
uses it when the ticket explicitly enables implementation. Missing source access,
credentials, operation semantics or reviewer judgement remain explicit gaps.
Neither generated code nor interface qualification establishes those facts.

Before provider calls, EngArg checks the selected `--eal-host`/`EAL_HOST` language,
execution features and method contracts. Its receipt records the selected
executable identity. With no selected host, portable authoring checks the exact
embedded EAL/4 contract and labels that receipt as an embedded contract rather
than an executed-host probe. Ticket execution always checks its selected host.

## Transport authority, pins and lifetime

`tools.toml` supplies command, MCP, HTTP and file authority. Prompts receive
operation contracts rather than command lines, endpoints or credential values.
The executable EAL file contains dependencies, predicates, method queries and
argument relationships; TOML does not become a second reasoning language.
Package publication freezes admitted local implementation files referenced by
command arguments with SHA-256 pins, preserving any operator-declared pins.
Rebuilding or executing a modified pinned script is rejected. Source data outside
those explicit implementation pins remains acquired data with observation
identity; it is not automatically asserted to be a frozen implementation.

Optional EAL preparation initialises selected MCP interfaces and lists tools. It
never calls an acquisition tool or creates evidence. A preparation receipt and a
new private TOML manifest retain server/protocol identities and canonical input
and output schema hashes. Runtime checks supplied pins before `tools/call`.
Preparation and collection share whole-closure admission: an invalid configured
version, credential policy, local implementation pin or complete literal input
fails before an earlier source can execute. A missing tool binding remains a
durable unavailable outcome during collection.
Unpinned execution retains the actual observed interface, and completed MCP error
responses retain it too. Advertised server identity is not binary attestation.

MCP stdio uses the official SDK over a bounded duplex stream owned by the existing
process supervisor. Command execution and MCP stdio share process-group cleanup
and Linux subreaping. Session completion, timeout, cancellation and host death
close the owner channel and trigger descendant cleanup before the acquisition
lease is released. Request, input, output, stderr and time bounds remain explicit.
HTTP MCP sessions use the SDK without local child ownership. No automatic retry
is implied for an interrupted acquisition whose external effect is unknown.

## SOLID in the Rust boundaries

| Principle | Concrete responsibility | Verification |
|---|---|---|
| Single responsibility | Grammar/lowering, pure evaluation, tool transport, observation persistence and argument compilation are separate modules | Parser, method, adapter and package tests exercise each boundary |
| Open–closed | Concrete tool transports share the acquisition contract; method queries use EAL-owned descriptors | Command/MCP/HTTP/file substitution cases and descriptor admission |
| Liskov substitution | Every adapter preserves request identity, observation age, failures and bounded output | Real subprocess and loopback HTTP tests, stale and malformed observations |
| Interface segregation | Authoring sees operation/schema metadata; runtime receives resolved transport authority | Prompt tests exclude argv, endpoints and credential values |
| Dependency inversion | Pure core has no IO; reusable MCP client has no dependency on the EAL server; EngArg consumes EAL contracts | Cargo dependency graph and independent EAL package execution |

Transport adapters own their external protocols. Reasoning methods are
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

This is a breaking language and report cutover. EAL/3 source, selector-bearing
programs and the former execution-field IR are rejected. Roots are version-checked
before import resolution; imported modules must use EAL/4 too. Current authoring
reports use `engarg.run.v3`; old reports require explicit reconstruction of their
current source contracts. Historical files remain available as immutable records.
The removed `eal prepare`, `integrate` and `register-tools` EngArg commands are
replaced by direct implementation and package building. EAL's optional interface
preparation is a distinct, current operation.

Optional discovery and qualification tools may help author an investigation.
They do not grant authority to execute a newly discovered operation, and their
catalogues are not required by a resolved package. Missing reviewer judgement is
represented by a scoped review request, not by an automatically accepted Boolean.

## Acceptance and measured scope

The acceptance suites exercise real MCP stdio and HTTP, supervised commands, retained
files, copied packages and independently stated numeric/logical outcomes. It
covers native deductive, inductive, abductive, causal, counterfactual and bounded
analogical methods, plus practical reasoning through an explicit policy review.
The synthetic finite-inventory example checks count, distinctness and explicit
population scope. These tests establish the implemented boundaries, not the
truth of arbitrary engineering premises, completeness of an external catalogue
or universal adequacy of analogy and decision policies. The composed matrix
defines seven EngArg modes across nine execution worlds, with further scoped
review adversities. An installed-host run is required: merely validating its
fixtures does not establish executable acceptance. See
[EXECUTABLE_ACCEPTANCE.md](EXECUTABLE_ACCEPTANCE.md).

Rust edition 2024 and each repository's pinned minimum toolchain remain in force.
Formatting, Clippy, workspace tests, parser regeneration and installed CLI/MCP
smoke checks are required. Model authoring quality and cost require separately
frozen trials with actual selected providers; deterministic protocol tests do
not claim those results. The finite before/after harness records actual request,
observation, file, byte and timing measurements for one fixed question; it does
not establish broad performance improvements. Its protocol is in
[EAL4_MEASUREMENT.md](EAL4_MEASUREMENT.md). The completion mapping and reproducible
verification commands are in [EAL4_COMPLETION.md](EAL4_COMPLETION.md).
