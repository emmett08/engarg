# Build the investigation that the argument requires

[Documentation](index.md) · [Engineer workflow](USAGE.md)

Once the CI argument has explicit evidence requirements and usable source tools,
the builder makes their relationship executable. It emits `case.eal`,
`tools.toml` and the local implementation or retained-resource files referenced
by the manifest. EAL can then execute that package independently of EngArg.

The [engineer workflow](USAGE.md) covers authoring and tool construction. This
chapter explains what a successful build promises and what still needs evidence.

## Connect requirements to actual tools

An analysis must contain support requirements, acceptance criteria and acquisition
dependencies. Bind each acquisition identifier in the operator manifest, or use
its compiler-generated EAL operation name. Then build a new package:

```sh
engarg build --input analysis.json --tools tools.toml --output-dir investigation
jq '{claims, pending_reviews, limitations}' investigation/build.json
```

A command binding can take this form when `measurement` is the actual acquisition
and `tools/measure.py` is its implementation:

```toml
[tools.measurement]
kind = "command"
version = "1"
argv = ["python3", "tools/measure.py"]
inherit_env = ["PATH"]
timeout_seconds = 15
max_output_bytes = 1048576
```

The command consumes the bounded EAL request on stdin and returns an observation
envelope with `value`. If it reports an existing measurement, it must preserve the
original `observed_at` time. Direct MCP stdio/HTTP, fixed-authority HTTP GET and
retained-file bindings follow the same acquisition boundary. Their protocol and
response mapping belong to the manifest, as described in
[private collectors](PRIVATE_COLLECTORS.md).

The compiler rejects an absent binding, conflicting compiler context or a formal
query misrepresented as an observation. The query is authored programme data.
Source tools provide observations or scoped review records that the programme
uses to evaluate it.

## Preserve the package's identity when moving it

Relative paths and command working directories are anchored to the manifest's
directory. Commands inherit only selected environment variables. Custom
implementations belong under `tools/`. Explicit schema, retained-file and pinned
dependency resources are copied with them. A data argument outside `tools/` needs
to be an existing literal relative file or an explicitly pinned dependency.
Dynamic filenames and external services remain tool inputs with declared scope.

`build.json` is an inspection receipt containing source/programme digests, file
hashes, emitted claim names and pending reviews. Execution needs `case.eal`,
`tools.toml` and their referenced resources. It does not read the receipt as an
additional semantic plan.

The builder validates before publication and stages files privately. It reads
regular resources within the manifest root without following links, checks
SHA-256 pins and bounds the written package to 256 MiB. It preserves executable
permission bits. Linux and Apple platforms publish the completed directory
atomically, without replacing an existing output. A copy or pin failure removes
the stage and leaves the final path absent.

## Fulfil reviews, then acquire evidence

A successful build may still list pending reviews. Those requests carry the
final merged operator/compiler context, which the importer checks against the
actual acquisition. A review needs the exact criterion, target and scope, an
identified reviewer, verdict, reasons and original review time. Follow the
[review workflow](USAGE.md#5-build-and-fulfil-scoped-reviews) to retain those
judgements and rebuild using the reviewed manifest.

Run the completed package with the paired EAL host:

```sh
export CLAIM="$(jq -er '.claims.current_claim' investigation/build.json)"
"$EAL_HOST" run investigation/case.eal --tools investigation/tools.toml \
  --workspace investigation-run-001 --claim "$CLAIM" > execution.json
jq '.result.assessment | {valid, complete, claims}' execution.json
```

An accepted tool binding permits execution. An observation supplies a value.
Claim support additionally depends on the criterion, applicable assumptions,
reasoning and material challenges. Keep these distinctions when examining an
unsupported result or deciding whether to revise the argument.

To inspect the completed run as a finite ASPIC+ graph, use
`engarg eal export-aspic --input investigation/case.eal --execution execution.json
--eal-core "$EAL_BIN/eal-core" --output aspic-view.json`. This produces a separate
`aspic-view/3` file for the companion visualiser. It preserves the retained
observations and assessment clock without executing collectors. See
[snapshot export](USAGE.md#inspect-an-aspic-snapshot) for method profiles, resource
limits and the distinction between authored status and formal results.
For [grounded argument investigations](argument-investigation.md), add
`--verification candidate/verification.json` to attach the candidate's matching
assurance receipt. The viewer retains its supplied EngArg origin and each
obligation's verdict separately from native computation completion.

## Verify portability and correspondence

The package integration tests move only executable artefacts to a new workspace,
remove the original build inputs and execute through a real EAL MCP host. They
also reject conflicting context, reserved output names, path escape, symbolic
links, excessive resources, changed local pins and invalid executable modes.

The mode suite adds actual native calculations and adverse reviews after the
same relocation. A shared finite batch is acquired once for several requirements,
while omitted, duplicated, substituted, truncated and unavailable results fail
their declared checks. These tests establish coverage of the enumerated request.
An external population's completeness still requires its own evidence.

See [verification](VALIDATION.md) for prerequisites and commands. When the accepted
argument itself needs to change, use [explicit revision](USAGE.md#7-resolve-gaps-and-revise-explicitly)
and build the resulting report into a new package before collection.
