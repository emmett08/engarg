# EngArg and EAL remediation

This ledger tracks the 21 findings in the 24 October 2026 remediation plan.
It records implementation locations and the checks needed to close each finding.
The original programme supplied implementation and regression evidence for all 21 findings.
Subsequent adversarial checks exposed boundary gaps that the original checks missed.
The follow-up below records their corrections and the additional evidence.
Exact tested revisions and hosted platform results are recorded on the paired pull requests.

The reviewed bases are EngArg `c4606e52d9dd07cf0e4e364d0dab30687118682c` and EAL `7bd9948ed242b1173b8331a5c4cee4a158a42722`.
These base identities describe the starting point and provide no acceptance evidence for the changes.
`G` below means this repository.
`E` means [emmett08/eal-mcp](https://github.com/emmett08/eal-mcp).
Test paths identify regression sources rather than a claim that every release gate has passed.

| Finding | Owner and implementation | Regression evidence | Removed ownership or deliberate compatibility |
| --- | --- | --- | --- |
| D1 Objection target escape | G `src/output_validation.rs` retains target, attack kind, basis and semantic target context with the admitted challenge and materiality | G internal objection-identity tests and `tests/dissent_retention.rs` cover identity mutation, target revision and legitimate disposition | Replaced retention based only on ID, text and materiality. Adjudication and repair use the same retained identity |
| D2 Registry credential disclosure | E `crates/eal-collector-contracts/src/binding.rs` owns parser-safe registry diagnostics and direct admission | E runtime `tests/registry_admission.rs` and CLI/MCP `tests/registry_diagnostics.rs` | Removed parser Display text and source excerpts from registry failures |
| D3 Semantic response mutation | G `src/eal_host.rs` and E MCP `redaction.rs` reject known sensitive values in semantic responses. Operator classification lives in E contracts | G `tests/eal_host.rs`, E `tests/host.rs` and installed bearer-leak checks | Removed successful responses whose semantic values were changed by redaction. Public configuration preserves its exact value |
| D4 Unbounded file admission | G `src/bounded_file.rs` and public CLI readers apply format limits and path policy. E adapters own reusable bounded artifact admission | G `tests/file_admission.rs`, E adapter admission tests and native FIFO tests | Replaced unbounded reads at these boundaries. Unsupported special files fail before reading. Registry aliases select a canonical target |
| D5 Renderer stack overflow | G `src/presentations/{plain,view,budget}.rs` use iterative attachment traversal and checked rendering | G `tests/presentation_budgets.rs` exercises 12,000 attachments through API and CLI, cycles, shared nodes and exhausted budgets | Removed recursive attachment expansion and infallible public rendering results in EngArg 0.10 |
| D6 Collector contract drift | E contracts own identifiers, profiles, templates, bindings, requests and envelopes. G catalogue and registry code consume them | E `registry-corpus.json`, contract admission tests, runtime registry tests and G catalogue/live tests | Removed G's independent wire models and identifier/environment admission rules. Runtime-exportable restrictions have a named profile |
| D7 Kubernetes capability mismatch | E collectors own immutable typed configuration and version 2 namespaced/cluster resource contracts | E configuration and collector tests cover absent generation, explicit scope, wrong identity, malformed metadata and measured absence | Version 2 preserves optional generation. Explicitly selected version 1 keeps its published namespaced contract during its compatibility window |
| S1 Mixed domain responsibilities | G `src/domain/{records,identifiers,diagnostics,index}.rs` and separate structural/narrative/evidence/status validators | G policy diagnostic tests and existing domain acceptance cases | `src/domain.rs` is a public facade rather than a combined record, graph and policy implementation |
| S2 Repeated graph reconstruction | G borrowed `ArgumentIndex` and `ValidatedArgument` serve validation, depth, export and presentation consumers | G index tests cover candidate/selected routes, warrant scope, cycles, diamonds, authored order and traversal counts | Replaced independent graph builders and repeated closure scans. Validated consumers reuse the borrowed index |
| S3 Repeated presentation semantics | G `PresentationView` owns route choice, consequential conditions, attachments and evidence uses | G presentation, plain-argument and budget suites check the supported formats and nested conditions | Removed backend-specific semantic expansion. Backends retain escaping and layout |
| S4 Repeated acquisition models | E `AcquisitionRequest` and `ObservationEnvelope` serve the runtime and optional adapters. G adds case/need correspondence | E contract and adapter admission tests, G retained/scoped/native collector tests | Removed repeated six-field wire request records and independent envelope definitions |
| S5 Repeated process policy | E `eal-process` owns protocol, client, cancellation and the process-tree supervisor. Runtime leases supply only a retention capability | E `eal-process/tests/process.rs`, runtime lease tests and G host deadline/output tests | Removed runtime implementation copies and G acquisition subprocess loops. Runtime process paths are delegating exports |
| S6 String-map provider configuration | E `crates/eal-collectors/src/config/` owns provider-specific records and `CollectorLimits`. CLI maps exist only during conversion | E `tests/configuration.rs` checks direct construction, provider selection, scope and rejected settings | Removed mutable raw option maps stored beside parsed configuration |
| S7 Validation by serialisation | E contracts `BindingSpec::validate` and registry admission own direct invariants used by parsing and construction | Shared registry corpus runs through contract and actual runtime loaders | Removed typed-binding to TOML to parser validation. Serialisation produces output |
| S8 Multiple operation-contract owners | E `crates/eal-mcp/src/operations/requests.rs` generates typed requests, operation identities, fields, schemas, exposure and acquisition classification | E operation corpus, SDK transport tests and installed checks for all 15 operations through text and MCP | Deleted `tools.json`, host-specific field rules and the separate integer dispatcher. Typed handlers match the operation enum exhaustively |
| A1 String identifiers and diagnostics | G checked argument/evidence/need references and diagnostic codes. E checked collector/tool/environment names and typed operations | G reference and diagnostic tests, E contract corpus and operation rejection tests | Public string encodings remain compatible. Prose and extensible evidence vocabulary remain text rather than invented enums |
| A2 Hidden 24-hour policy | G `src/export_policy.rs` resolves review and acquisition ages, including per-need values, into the v3 identity | G `tests/export_v3.rs` checks explicit defaults, invalid ages and identity-sensitive policy changes | The initial 86,400-second default is documented and persisted. Frozen readers alone retain historical defaults |
| A3 Comment-based acquisition plans | G v3 `ExportPlan` is structural authority for acquisition. Plans, policy and selected profile participate in revision identity | G v3 deterministic reconstruction and tampering tests, plus real EAL export tests | New exports use no comment protocol. Frozen v1/v2 readers retain their historical marker interpretation |
| A4 Split adapter/trace ownership | E `eal-adapters` owns reusable artifact, HTTP, command, scoped and retained mechanics and trace inspection. G retains case-specific correspondence | E adapter admission tests and G trace, retained, scoped, laboratory and live catalogue tests | G's old entry points delegate during migration. Attribute keys are configurable and trace inspection remains candidate identification |
| Q1 Fleet observation age | E collector timing records aggregate and per-item acquisition intervals with a conservative oldest bound | E controllable-clock fleet tests cover staggered reads, backwards time, future windows and partial failures. Runtime reuse tests preserve age | Removed aggregate timestamping based only on final completion. Historical observations retain their original time and unknown interval detail |
| Q2 Registry-alias verification claim | G `src/bounded_file.rs`, host path admission and `VALIDATION.md` describe the implemented alias policy | G file-admission and host tests cover permitted registry aliases, forbidden leaf aliases and parent resolution | Removed the claim that all registry leaf symlinks are rejected. Path pinning does not assert immutable contents under an in-place writer |

## Ownership and compatibility

EngArg owns argument construction, retained dissent, evidence-need mapping and presentation.
The EAL contract package owns acquisition records and binding invariants.
The process package owns bounded execution.
Optional adapters own source integrations.
The runtime owns correspondence, persistence, leases and reuse.
The pure language core remains independent of collectors, process execution and external services.

| Release boundary | Compatibility rule |
| --- | --- |
| EngArg 0.10 | Checked rendering returns `Result` and callers handle budget failures. Audit append accepts decoded JSON. New exports use v3 |
| EngArg 0.11 | Remove the old known-tool, scoped and retained command facades and trace alias after callers and catalogues migrate to EAL-owned adapters |
| EAL 0.2 | Remove the legacy `eal-command-supervisor` executable alias after deployments install `eal-process-supervisor`. Runtime process paths delegate to the sole process implementation during migration |
| Native collectors 1.0 | End the version 1 descriptor/command compatibility window and the legacy Tempo `--inherit-env` facade. Version 2 is explicitly selected before this boundary |
| Historical export readers | Keep v1 and v2 source and identity reconstruction frozen. These readers have a persistence compatibility policy separate from command-facade removal |

Historical reconstruction is read-only compatibility.
It preserves recorded source and identity rather than granting authority to execute.
Current file admission, binding validation, credential handling and process supervision apply before execution.
An unsafe historical binding is refused.
Explicit re-export creates a new revision and requires evidence bound to that revision.
Neither re-export nor reuse changes the original observation age.

## Boundary follow-up

The follow-up starts at consumer `74af378` and producer `b48a48d`.
Those identities identify the inspected sources rather than acceptance of the corrections.

| Gap | Implemented correction | Additional evidence |
| --- | --- | --- |
| Encoded known credentials | EAL contracts own the bounded sensitive-JSON inspector used by EngArg, EAL host and native Tempo. Ambiguous encoded JSON and exhausted inspection budgets fail safely. Successful semantic responses retain their exact values | Contracts inspector tests, both host suites and native CLI credential tests |
| Repeated evidence expansion before output admission | Template rows borrow records and stream through bounded escaping. Output failure stops further expansion | `tests/template_expansion.rs` uses a fresh process with 1,500 references to one 64-KiB source. Streaming unit tests and existing presentation suites preserve escaping and complete output |
| Repeated composition prose in failed-admission inspection | Alternative-route and support-condition lists stream through the same checked sink. Reference-safe inspection retains its admission notice | Fresh-process duplicate-composition regressions preserve inspection and stay within the working allowance |
| Diagnostic labels expanded before narrative admission | Invalid requests and canonical IDs stop dependent checks. Reading claim and paragraph references admit before narrative policy can reuse their labels | `tests/reference_admission.rs` covers API/CLI refusal, bounded diagnostic volume, all renderers and fresh-process memory probes |
| Same-ID replacement of an inference endpoint | Retained target context includes direct endpoint propositions for warrants, assumptions and defeaters. Capture follows one hop without traversing dependency closures | Identity tests cover premise/conclusion replacement, support-only updates, missing endpoint admission, changed-relation poisoning and cyclic attachments |
| Repeated authoritative prompt context | Generated prompts may intern identical complete target contexts into a request-local table when the encoded request shrinks after the added instruction. Every original context remains recoverable in the same request | Independent round-trip tests and the fixed-output token replay in `EVALUATION.md` |
| Evidence qualification lost by packet projection | EAL preserves unresolved availability when issues were omitted and rejects unknown issue values | Producer packet and model-context suites, including incomplete packet/3 and fallback cases |
| Reusable path and native admission gaps | EAL canonicalises relative-artifact bases and admits native attribute and UTF-8 path configuration before effects | Producer adapter, configuration and installed collector regressions |

Direct endpoint context preserves represented proposition fields, rather than proving equivalence between rewritten sentences.
Support status, evidence links, backing and investigation plans can change without replacing those propositions.
Unknown context from an invalid candidate is filled only when the original target relation is unchanged.
The known scope remains fixed throughout repair.

The template escaping responsibility is extracted into `render/text.rs`.
The existing argument-record formatter retains its fourteen-component semantic mapping and uses borrowed display fragments.
Prompt context interning is a separate private policy with an independent reconstruction check.
The six-call hybrid topology, provider settings, output schema and repair allowance remain unchanged.
Replay measures input reference tokens and preserved fixed completions, not live reasoning quality or billed cost.
Reading-reference defects now use the existing `ReferenceIntegrity` category rather than `NarrativePolicy`.
Known-reference cases with narrative or composition policy defects remain available for labelled inspection.

## Original programme acceptance record

The immutable EAL producer is `b081ef126bf231bf66be3523870efa39b362229c`.
Cargo and the compatibility job pin that same revision.
The GitHub producer tree was checked against the locally tested tree `105c290809475984fcb5fd79e5d9b5205b77c8fb` before publication.
The consumer pull request records its exact tested commit and hosted Linux/macOS results.

| Gate | Result |
| --- | --- |
| EngArg standard suite | 293 passed with 9 installed-boundary cases reserved for the mandatory compatibility job |
| Immutable validated-view doctest | Passed |
| Paired real-binary compatibility suites | 30 passed, including every reserved installed-boundary case |
| EAL workspace | 457 passed |
| Separate context experiment checks | Formatting and warnings-denied Clippy passed, 24 tests passed |
| Parser regeneration | Exact generated recogniser agreement, grammar unchanged |
| Formatting and linting | Both repositories passed with warnings denied |
| Release builds | Both repositories passed |
| Installed EAL applications | All 10 groups passed, including all 15 operations through actual text and MCP entry points |
| Installed coordination | Four completed JSON/laboratory runs and unknown-effect recovery passed |
| Historical compatibility | Separately compiled v1/v2 producers supplied exact reconstruction fixtures, including arbitrary-precision decimal contracts |
| Older consumer with new producer | Reviewed EngArg 0.9 successfully validated its original v2 bundle through the new installed EAL host |

The local Rust 1.90 toolchain intermittently emitted zero-length archive objects in unchanged code.
EAL verification used a code-generation-unit override after that host failure.
Repository build profiles remain unchanged.

The final review removed unused historical writer/registry helpers.
Only the source, identity and validation rules required to reconstruct historical bundles remain in the private legacy modules.
New exports obtain plans from their structural record and policy from their resolved policy object.
An unsafe historical binding cannot bypass current execution admission.

Growing private responsibilities were extracted into contract admission, process supervision, adapter profiles, argument indexing, presentation budgets and typed operation definitions.
The current exporter retains declaration order and case-to-EAL mapping as one cohesive projection responsibility.
Retained-objection comparison remains beside its workflow transition because adjudication and repair share that same invariant. Its target-context policy is extracted into `output_validation/target_context.rs`, separating semantic fields from mutable support and investigation records. All case-producing roles receive the same identity and restore instructions. Field-level diagnostics and deterministic one-repair `critic-3` regressions cover preserved open dissent, justified revision, silent target mutation and investigation refinement. A missing target in an invalid candidate does not freeze a fictitious null meaning; its known scope and attack identity remain protected until the target is supplied.
Compatibility command facades contain delegation and case binding rather than independent acquisition algorithms.

This evidence closes the listed findings at the paired tested revisions.
It does not establish permanent freedom from defects or code smells.
