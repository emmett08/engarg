# From an engineering claim to a justified decision

A team proposes running independent CI test groups in parallel. The expected
benefit is easy to state: a shorter pipeline with the same test outcomes. The
engineering work lies in establishing what “independent”, “shorter” and “same”
mean for the selected workload, then obtaining evidence that meets those terms.

EngArg develops that argument and makes its missing support explicit. EAL/4
executes the investigation the argument requires. The engineer supplies source
access, bounds the tools' authority and judges whether the assumptions and
criteria address the intended decision. These responsibilities remain visible
throughout the workflow, so a successful tool call cannot silently become proof
of a broader claim.

## Start with the decision, then follow its evidence

Read [the argument model](DOMAIN_MODEL.md) to see how a claim depends on grounds,
warrants and assumptions, and how a challenge can defeat a particular inference.
The distinction between a current qualified conclusion and a stronger target
lets the investigation make progress while retaining the remaining work.

Then follow [the engineer's guide](USAGE.md). It installs the paired applications,
authors the CI claim, configures command or MCP sources, generates missing tools
in Rust, Go or TypeScript, and builds a package. The same guide obtains scoped
reviews, executes evidence collection through `eal-mcp`, assesses the retained
observations and revises the argument when its conditions fail. Its commands use
the public applications directly.

[Assistant providers](AI_ASSISTANTS.md) use Cursor, Augment or another installed
assistant for authoring and tool generation. The default HTTP integration remains
available for Responses and Chat Completions services.

Before a paid authoring run, [the finite inventory example](finite-inventory.md)
checks the installed path with an actual local MCP source and command audit.
Its supported conclusion concerns a synthetic three-entry snapshot. That narrow
scope makes it useful for diagnosing installation and transport problems without
mistaking it for evidence about your CI system.

## Inspect the contracts at each transition

The guide gives the sequence. The following chapters explain the decisions
within it and the contracts to inspect when a stage remains unresolved.

| Engineering question | Chapter |
|---|---|
| What observations would resolve this argument's gaps? | [Plan support and preserve dissent](support-planning.md) |
| Can the proposed inference be executed faithfully? | [Reasoning methods and their limits](REASONING_METHODS.md) |
| How do I supply an existing source and control its authority? | [Private collectors and direct bindings](PRIVATE_COLLECTORS.md) |
| How do I construct and verify a missing acquisition? | [Tool implementation](TOOL_IMPLEMENTATION.md) |
| What must the portable investigation contain? | [Executable packages](executable-packages.md) |
| How does the authored argument become EAL? | [Compiler mapping](EAL_MAPPING.md) |

An unsupported assessment directs the next investigation. It can identify a
missing source, an adverse observation, an inapplicable model or an unresolved
review. Keep that distinction when deciding whether to collect again, repair a
tool or explicitly revise the claim. The [revision commands](USAGE.md#7-resolve-gaps-and-revise-explicitly)
preserve the previous case and invalidate work that no longer corresponds to it.

## Coordinate larger investigations and communicate the result

Once the direct workflow is understood, [tickets](TICKETS.md) coordinate its
bounded stages and retain their identities. [Debate topologies](DEBATE_TOPOLOGIES.md)
change how specialist authoring contributions are exchanged. Their extra calls
need an evaluation against a stated failure mode and budget.

[Presentation](PRESENTATION_API.md) renders the same canonical argument for
different readers. A concise view still needs to expose the qualified conclusion
and the conditions that determine whether it can be used.

## Maintain the implementation and assess its value

[Architecture](EXECUTABLE_ARCHITECTURE.md) explains why authoring, compilation,
execution and evidence storage have separate owners. [Verification](VALIDATION.md)
maps those boundaries to executable checks and gives the commands to run them.
Use those checks for a code change, including the installed runtime when the
change affects acquisition or assessment.

Whether the system helps engineers make better decisions is a separate empirical
question. [Evaluation](EVALUATION.md) defines a comparison with expert-reviewed
cases and distinguishes historical measurements from current software coverage.
[The complete-workflow study](support-study.md) freezes the inputs, executes each
phase and retains failures, resource use and independent judgements.

For EAL's language and runtime contracts, continue with the
[EAL documentation](https://github.com/emmett08/eal-mcp/blob/main/docs/index.md).
EngArg's dependency and installation helper select an exact producer revision,
which may precede the current EAL default branch.
