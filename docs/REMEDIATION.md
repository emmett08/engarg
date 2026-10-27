# Executable evidence cutover

The current execution contract is an EAL/3 program with the `execution/1`
profile, direct `tools.toml` bindings and any required implementations under
`tools/`. EngArg compiles these artefacts; EAL collects observations and assesses
the argument. The package does not require a capability catalogue, provider
ranking, generated registration or an executable graph JSON sidecar.

The earlier remediation history and its exact tested revisions remain in Git.
Those historical results do not establish acceptance of the current change.
Current checks and their limits are recorded on the paired pull requests.

## Ownership

| Responsibility | Owner |
| --- | --- |
| Authored argument, retained dissent and explicit revision | EngArg domain and engine |
| One argument-to-EAL translation | EngArg support compiler |
| Grammar, method contracts, exact expressions and assessment | EAL pure core |
| Concrete tool authority and request/envelope contracts | EAL collector contracts |
| Dependency scheduling, immutable observations and reuse | EAL runtime |
| MCP wire client | EAL MCP client |
| Bounded process lifecycle | EAL process supervisor |
| Generated tool isolation and portable package execution | EAL tool runtime |
| CLI and MCP run operation | Thin front ends over EAL execution service |

## Replaced paths

| Previous mechanism | Current mechanism |
| --- | --- |
| Frozen v1/v2 EngArg source reconstruction | Explicit authoring revision and current package build |
| Authored projection followed by appended support gates | One compiler lowers acquisitions, requirements, methods and challenges |
| `engarg_support_graph` and support-graph collector | Direct named acquisitions scheduled from EAL dependencies |
| Semantic execution-plan JSON | EAL `bind` dependencies and bounded pure `derive` expressions |
| Runtime catalogue ranking and provider registration | Exact tool-name lookup in authorised TOML |
| Fixed formal-input response echo | Authored method query with acquired data bindings |
| Legacy command-supervisor alias | `eal-process-supervisor` |
| Ticket registration and bundle artefacts | Ticket/2 source, TOML, build receipt and execution result |
| Catalogue-specific qualification orchestration | General package build and EAL run commands |

Optional discovery remains an authoring concern. It can suggest an operation;
it cannot grant an executable, endpoint or credential authority. Generated tools
retain their explicit verification and isolation requirements. A successful
interface check is not evidence of population completeness or inference validity.

## Preserved boundaries

Observations retain the original measurement time, request identity, concrete
binding identity and scope. Derived observations retain their upstream age.
Measured negatives remain available to counter-evidence calculations. Failed or
missing acquisition remains distinguishable from a false predicate. A completed
assessment can legitimately leave the claim unsupported or unresolved.

Pure computational methods evaluate their declared input domains. They do not
certify sampling design, causal identification, model applicability or a prose
interpretation. Those obligations use independent evidence or a scoped review.
A review must match the exact criterion, target and scope; a bare accepted flag
is insufficient. Challenges to applicability assumptions are translated for
native methods as well as structured reasoning.

Historical observations are immutable. The current compiler does not silently
reconstruct an old executable program, rewrite observations or refresh their age.
An explicit revision creates a new argument identity and invalidates dependent
compilation and assessment artefacts while retaining dissent context.

Process supervision, bounded file admission, exact JSON values, credential
classification, immutable argument views and checked rendering remain shared
boundaries. MCP stdio retains its SDK child-process lifecycle; command execution
uses the stronger process-tree supervisor. Documentation must not equate those
lifetime guarantees.

See [executable architecture](EXECUTABLE_ARCHITECTURE.md),
[package workflow](executable-packages.md) and
[direct authoring](support-planning.md) for the current interfaces.
