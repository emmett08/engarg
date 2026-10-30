# Preserve the boundary between an argument and its evidence

[Documentation](index.md) · [Engineer workflow](USAGE.md)

A CI timing argument can remain valid as algebra while its production premises
remain untested. The implementation must preserve both facts. EngArg owns the
construction and explicit revision of the argument, and compiles the required
investigation. EAL owns acquisition, retained observations and assessment.
Operator TOML supplies authority to contact sources or execute code. Each
boundary answers a different question before the next stage can proceed.

## Compile one executable meaning

`engarg build` and `engarg eal export` use the same package builder and
`support_export::compile_direct`. Its passes lower acquisition dependencies,
exact requirement checks, native method applications and the argument into one
EAL/4 programme. The package contains that source, `tools.toml` and referenced
local resources. Its `build.json` receipt records hashes, claim names and pending
reviews for inspection. Removing the receipt leaves the executable meaning intact.

The source carries the reasoning. Evidence `bind` fields express dependencies,
while `derive` expresses bounded pure calculations over observations. Claims,
methods, assumptions and objections belong to the same programme. EAL executes
the selected claim's dependency closure. One acquired CI run may supply several
checks, but every use retains the same source identity and original age.

A method query is authored programme data. Acquired values supply the empirical
inputs to that query. EAL-owned descriptors define each method's input schema,
result paths and computational limits. EngArg consumes those descriptors rather
than maintaining another method schema registry. During assessment, EAL
recomputes retained derivations against their parents to check integrity.
This does not create another observation or independent confirmation.

The [compiler mapping](EAL_MAPPING.md) explains how argument elements become
source declarations. The [package contract](executable-packages.md) explains
which files move with the executable investigation.

## Admit a report before changing or executing it

The current report contract is `engarg.run.v3`. Authoring, tool implementation,
revision, ticket and build paths read the same typed report. It retains the
argument, support elaboration, concrete binding state, acquisition design review
and target-runtime receipt. Its declared origin is either provider authoring or
an explicit synthetic fixture.

Retained completions constrain what the original author actually supplied.
Direct, optional-discovery and argument-only reports all check that relationship
at their original authoring state. Recovery and replanning first reconstruct
and validate their retained parent. These checks establish consistency of
unsigned records. They do not authenticate a provider or an external source.

Before model calls, EngArg checks a selected `--eal-host` or `EAL_HOST` for EAL/4
features and method contracts. The receipt identifies the selected executable.
Portable authoring without a selected host instead checks the exact linked EAL
contracts and labels the result accordingly. Ticket execution always checks its
selected host. Later tool implementation checks its current host while preserving
the report's original authoring receipt.

Authoring budgets constrain both live execution and later report admission.
Original calls retain their configured count and byte allowances. Each explicit
repair records its position in the transcript and its own allowance, including a
repair stage that makes no call. Later repairs cannot enlarge earlier budgets.
Checkpoint and report admission share this policy through small allowance records,
without copying the transcript for each repair.

A checkpoint can complete or repair final validation once the primary authoring
stages are present. It cannot continue a missing specialist response. See the
[provider failure and recovery instructions](USAGE.md#3-author-and-inspect-the-argument)
for the distinction between a retained candidate and a failed provider exchange.

## Substitute the authoring provider at one contract

HTTP models and supervised assistant commands implement `LanguageModel` with the
same typed request and completion. Cursor, Auggie stable, Claude Code, Codex,
Copilot CLI, Gemini CLI and generic JSON adapters own their wire encodings;
the shared command transport owns environment selection,
temporary working directories, byte limits, deadlines and cancellation. Selection
does not move argument validation or evidence acquisition into an adapter.

Common model selection and completion limits are separate from HTTP endpoint,
credential and retry settings. The command builder consumes only the common
settings. The enum selects a concrete implementation of the narrow trait; it
does not require a hierarchy of provider objects. A new native protocol requires
explicit selection changes, while another generic command adapter leaves the
argument engine unchanged.

An owned concurrency permit brackets each assistant worker, its supervisor
acknowledgement and its temporary directory. A dedicated blocking worker reports
through a oneshot channel, so a logical timeout does not make Tokio shutdown
wait for cleanup. The worker retains resources until the supervisor closes its
private retention endpoint, including protocol-error paths. Terminated processes
awaiting OS reaping are distinguished from executing descendants in the tests.

Assistant configuration remains operator authority. Retained provider metadata
identifies the selected assistant and configuration digest, while old HTTP reports
round-trip without added assistant fields. Unknown native model usage remains
unknown. [Assistant providers](AI_ASSISTANTS.md) describes the protocol and the
limits of native token-budget enforcement.

Historical JSON remains compatible. Rust callers constructing
`ProviderRunConfig` with a struct literal must add `assistant: None` for an HTTP
configuration.

## Make revisions explicit

An accepted argument may need a narrower population, a different inference or a
corrected criterion. `engarg eal revise` makes that change visible. Its bounded
history retains the parent identity and source snapshot, original request,
authoring exchanges and dissent. Changed executable arguments require complete
support elaboration. Changed requirements identify their previous digest and
explain the revision.

The host revalidates support and recomputes acquisition design review. It resolves
bindings from the supplied TOML or marks them unbound. Previous tooling,
collection and assessment claims cannot silently attach to the new argument.
The revised report remains a normal report that can be revised again or built.

Route-only replanning preserves the argument's meaning. Sparse encoding metadata
can restore omitted defaults or null encodings for an exact parent digest check,
but the restored report must preserve the complete typed content. It cannot hide
a changed warrant, support requirement or provenance record.

Coordinated authoring also retains participant and round identities. Replay
uses the admitted debate configuration and preserves parallel arrival accounting.
Two participants sharing a role still have distinct objections and histories.

## Search alternative routes without changing their obligations

`engarg optimise` searches alternative-route selections through a separate pure
beam-search module. Its executor adopts changed selections only through the
existing explicit revision API, builds each candidate with the same support
criteria and collects fresh source-bound observations. It recomputes the native
assessment before admitting a supported candidate. A separate optimisation
receipt records search completeness and selected paths without extending the
authored-report schema. See [argument optimisation](optimal-argument.md).

## Construct tools without granting them evidence status

A missing acquisition with an authored parameterised specification can enter the
isolated tool builder. Verification cases are frozen before coding. A passing
candidate retains its implementation, verification receipts and generation audit,
then contributes concrete bindings to a new operator manifest. Main authoring,
`eal implement` and explicitly enabled ticket implementation use this same stage.

A generated CI reader can be tested against its protocol and fixtures. That
result leaves production access, measurement design and reviewer judgement as
separate obligations. The source argument and original manifest remain retained.
[Tool implementation](TOOL_IMPLEMENTATION.md) gives the code-generation contracts
and Rust, Go and TypeScript toolchain boundaries.

## Keep transport authority in the manifest

`tools.toml` selects command, MCP, HTTP and file access. Prompts receive operation
contracts, while execution receives resolved transport settings and explicitly
selected environment variables. The EAL source continues to own dependencies,
predicates, method queries and argument relationships.

Package publication pins admitted local implementation files referenced by
command arguments with SHA-256, preserving operator-declared pins. Rebuilding or
executing a modified pinned script fails admission. Acquired source data has its
own observation identity. A data file becomes a fixed implementation dependency
only when the operator explicitly pins it as such.

EAL's optional interface preparation initialises selected MCP servers and lists
their tools. It creates no observation. Its receipt and new private manifest
retain server/protocol identities and canonical schema hashes. Runtime admission
checks supplied pins before calling a tool. Both preparation and collection admit
the entire selected closure first, so an invalid configured version, credential
policy, local pin or complete literal input fails before an earlier source can run.
An absent binding instead remains a durable unavailable collection outcome.

An unpinned call retains the observed interface, including completed MCP error
responses. Advertised server identity identifies the protocol peer. Binary
attestation requires separate evidence.

## Bound process ownership and cancellation

MCP stdio uses the official SDK over a bounded duplex stream owned by the
process supervisor. Command execution and MCP stdio share process-group cleanup
and Linux subreaping. Completion, timeout, cancellation and host death close the
owner channel and trigger descendant cleanup before releasing the acquisition
lease. Input, output, stderr and time bounds remain explicit.

HTTP MCP sessions use the SDK without local child ownership. An interrupted
acquisition may have produced an external effect, so it does not imply an
automatic retry. Retained failure distinguishes that uncertainty from an
observed negative result.

## Apply design principles at the changing boundaries

| Principle | Responsibility boundary | Verification |
|---|---|---|
| Single responsibility | Recognition, pure evaluation, transport, persistence and argument compilation have separate owners. | Parser, method, adapter and package suites. |
| Open–closed | Transport variants share the acquisition contract. Method queries consume EAL-owned descriptors. | Command/MCP/HTTP/file substitution and descriptor admission. |
| Liskov substitution | Every adapter preserves identity, observation age, failures and output bounds. | Real subprocess/HTTP, stale and malformed-observation cases. |
| Interface segregation | Authoring receives contracts. Runtime receives execution authority. | Prompts exclude command arguments, endpoints and secret values. |
| Dependency inversion | Pure evaluation has no IO. The reusable MCP client does not depend on the server. | Cargo dependencies and independently executed packages. |

Transport adapters handle external protocols. Method strategies share typed
input/result contracts. RAII brackets process cancellation, store leases and
package publication. These choices isolate actual variation and lifetime
responsibilities. A second registry or global service locator would add another
source of authority for the engineer to reconcile.

## Verify the boundary that changed

`optimal_argument::development` orchestrates bounded proposal and sceptic calls
through a `CandidateInvestigator` interface. It does not own evidence authority.
`optimal_argument::assurance` evaluates the operator's frozen correspondence and
discharge contract; `investigation::VerifiedInvestigator` admits explicit
revisions and invokes native execution. The CLI owns private artifacts, input
pins and provider configuration. These boundaries keep model criticism separate
from acquired observations and mechanically checked results. See
[argument investigation](argument-investigation.md) for the policy and artifacts.

EAL/4 is the supported language and `engarg.run.v3` is the current report
contract. Older source and report shapes require explicit reconstruction of their
current contracts. Historical observations retain their original content and
measurement times.

[Verification](VALIDATION.md) gives the current repository and installed-runtime
checks, including moved packages, actual MCP acquisition, method limits and
adverse reviews. [Evaluation](EVALUATION.md) addresses a different question:
whether this architecture and authoring workflow improve decisions at an
acceptable total cost. Synthetic acceptance cases establish implementation
behaviour within their declared scope. They leave that empirical question open.
