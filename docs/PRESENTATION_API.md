# Present an argument without losing its conditions

[Documentation](index.md) · [Argument model](DOMAIN_MODEL.md)

A short view of the CI claim still needs to show the conditions on which it
depends. Hiding a deeper derivation should not make a conditional conclusion
appear unconditional. EngArg's renderers therefore share the admitted argument
view and retain consequential selected-route conditions at display boundaries.

Public rendering functions return `Result<String, RenderError>`. Callers handle
the error before writing a result. A successful result contains the complete
requested view within its declared limits. Exhausting a limit produces an error,
so a partial document cannot be mistaken for a complete assessment. The CLI uses
these checked entry points.

## Share meaning and bound the presentation work

`PresentationView` borrows the index from `ValidatedArgument` after admission.
It supplies canonical claim lookup, selected routes, warrant dependencies and
attached conditions to every formatter. Evidence and its uses are indexed once.
Backends own escaping, anchors and layout. They display literal authored
propositions and the recorded assessment, whose truth still depends on evidence.

Every backend accepts the same `RenderLimits`:

| Resource | Default ceiling |
| --- | ---: |
| Records | 100,000 |
| Reference edges | 500,000 |
| Conservative visit units | 8,000,000 |
| Encoded argument input | 16 MiB |
| Output | 16 MiB |
| Conservatively charged working storage | 192 MiB |

These are logical allocation and work bounds. They do not measure operating
system RSS or impose a wall-clock deadline. The `*_with_limits` entry points
allow applications to select lower ceilings. Index queries reserve work for
their traversal and formatting passes before execution.

The input is counted without allocating a second encoded document, and output
storage stops growing at its allowance. Repeated evidence, conditions and
composition rationales stream from borrowed records through the escaping writer.
A failed write stops further expansion. This keeps repeated references from
allocating a complete source copy for every occurrence before the output bound
can be checked.

## Keep inspection distinct from accepted input

Library convenience functions can show arguments with narrative or assessment
policy errors when their identities and graph references remain safe. They build
an inspection index with the same resource ceilings and place a notice and domain
diagnostics beside the report heading. The notice identifies the failed domain
validation and the need to repair the recorded assessments. Conflicting identities
or broken graph references require repair before rendering. Canonical CLI reports
undergo domain admission before presentation.

Attachment traversal uses an explicit stack. It visits assumptions and their
nested attachments before challenges, and follows investigations attached to each
target. Shared targets and cycles expand once per traversal. Bounded indentation
keeps deep arguments readable, with deeper support in shallow detail records or
an explicitly selected display-depth summary.

In HTML, a closed heading retains the qualification and the number of reliance
conditions. Their full statements appear in the disclosure body, outside
`summary`, so the native control can hide and reveal them with the supporting
argument. Those conditions remain available when the display-depth setting omits
deeper derivations.

## Append a retained audit safely

`with_audit` accepts a decoded `serde_json::Value`. It checks JSON depth and work
iteratively before pretty printing, and shares the renderer's output and working
storage ceilings. Its Markdown fence is longer than any embedded backtick run.
Exhausting a JSON or output budget rejects the append. Accepting decoded JSON
gives the function an inspectable traversal boundary.

Use the rendering commands in [USAGE.md](USAGE.md) to inspect a retained report.
The [validation guide](VALIDATION.md) explains the checks that distinguish a
well-formed presentation from an adequately supported claim.
