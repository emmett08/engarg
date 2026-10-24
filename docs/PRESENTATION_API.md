# Bounded argument presentations

EngArg 0.10 replaces the public `String` rendering results with
`Result<String, RenderError>`.
Callers must handle an explicit resource error before writing a result.
The CLI uses these checked entry points.
A successful result contains the complete requested view.
An error never returns a truncated document as a successful assessment.

Every backend shares `RenderLimits`.
The default ceilings permit 100,000 records and 500,000 reference edges.
They permit 16 MiB of encoded argument input and 16 MiB of output.
Working storage is charged conservatively against 192 MiB.
The work allowance is eight million conservative visit units.
An index query reserves enough units for its traversal and formatting passes.
These are logical allocation and work bounds rather than operating system RSS
or wall-clock deadlines.
The `*_with_limits` entry points let applications select lower ceilings.
The encoded input is counted without allocating a second encoded document.
Output storage stops growing when its byte allowance is exhausted.
The extended template streams repeated evidence and condition descriptions from
borrowed records through its escaping writer.
It does not retain one expanded source copy per reference or build all fourteen
argument entries before checking the output allowance.
A failed output write stops the remaining cell and record expansion.
Composition summaries use the same streaming path, including inspection views
with duplicate composition diagnostics.
Repeated lookups of one retained rationale therefore share the output allowance
without first allocating one rationale copy per record.

The shared borrowed `PresentationView` consumes the index from
`ValidatedArgument` when domain admission succeeds.
It owns canonical claim lookup and route selection for every formatter.
It also owns consequential conditions, warrant-specific dependencies and
attachment traversal.
It indexes supplied evidence and its uses once.
The formatting backends own escaping, anchors and layout.
They use literal authored propositions and retain the recorded assessment.
Rendering does not establish that the propositions are true.

The convenience functions also retain the existing inspection behaviour for
reports that have narrative or assessment-policy validation errors.
Conflicting identities and broken graph references require repair before rendering.
This compatibility path builds an inspection index after failed admission.
It has the same resource ceilings and never treats its index as an admission
certificate.
Every backend places an inspection notice and its domain diagnostics beside
the report heading.
The notice states that domain validation failed and that the recorded assessments
require repair.
The extended template also retains the diagnostics in its composition checks.
Canonical CLI reports undergo domain admission before rendering.

Attachment traversal uses an explicit stack.
It visits assumptions before their nested attachments and then challenges.
Investigations follow the attachments of their target.
Shared targets and attachment cycles expand once per traversal.
Claim indentation is bounded.
Deeper support appears in shallow detail records or in the explicitly selected
display-depth summary.
Consequential selected-route conditions remain visible at a collapsed boundary.

`with_audit` now accepts decoded `serde_json::Value`.
It checks JSON depth and work iteratively before pretty printing.
It shares the output and working-storage ceilings of the other renderers.
Its fence is longer than any embedded run of backticks.
A failed JSON or output budget rejects the audit append.
The generic `Serialize` entry point was removed because arbitrary user-supplied
serialisers provide no inspectable traversal boundary.
