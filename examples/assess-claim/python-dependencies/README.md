# Assess the actual Python dependency environment

This bounded claim asks whether the installed metadata for DoWhy 0.14 and
causal-learn 0.1.4.8, and their reachable applicable runtime requirements, is
consistent in the selected immutable image. It concerns real installed metadata.
Binary/API correctness and scientific adequacy require their own evidence.

The request specifies a missing acquisition operation; it contains no collector
implementation or accepted observations. The generic runner asks EngArg to author
the argument, generate and verify that operation, export concrete bindings, build
EAL and execute it. The generated collector uses `importlib.metadata` and
`packaging` in the prepared image. No review instrument supplies affirmative
judgements.

From the repository root, build the trusted image as described in
[Library selection](../../../docs/library-selection.md#prepare-the-dependencies).
Then inspect its actual image ID. The checked-in request and toolchain retain the
exact image used for the recorded investigation:

```text
sha256:b6e5b7c0314ca219a8b51dbfd6c676737240443728c5d00390a00d396ec015a2
```

If rebuilding produces another image ID, copy both JSON files into your
investigation directory and explicitly replace that ID throughout them before
starting a new authoring run. This declares the new target; earlier observations
retain their original target and timestamps. The toolchain's `image` controls
execution authority. The operation's `image_id` argument is an operator scope
label, not image attestation by the generated process.

With a compatible EAL host and all its companions on `PATH`:

```sh
scripts/assess-claim.sh \
  --input examples/assess-claim/python-dependencies/request.json \
  --toolchain examples/assess-claim/python-dependencies/toolchain.json \
  --model "$ENGARG_MODEL" --api-key-env OPENAI_API_KEY \
  --eal-host "$EAL_HOST" --output-dir work/python-dependencies-001 \
  --author-mode single --author-max-calls 3 --author-max-repairs 2 \
  --author-max-output-tokens 32768 --tool-max-output-tokens 32768 \
  --request-timeout-secs 300 --tool-timeout-secs 1800
```

Use a new output directory for every attempt. The default tool store is private
to that run; supply `--tool-store` to retain verified packages across runs. Inspect
`summary.json`, stage logs, native generation audit, frozen verification cases,
exported bindings and the actual EAL observations and assessment separately.
A successful collection or completed assessment does not by itself mean that the
claim is supported. Unavailable source, implementation-adequacy or review
obligations remain explicit.

The Packaging version matters. An initial live generation froze an incorrect
expectation that `>=1` rejected installed version `2.0rc1`. The exact Packaging
26.3 API returns true for default single-version containment; that behavior
changed in 26.0. The incorrect attempt was retained, and the specification was
explicitly revised before another native generation. This example includes the
version-specific semantic control so the coding stage cannot legitimately change
library behavior to satisfy that historical expectation.

## Recorded real investigation

The [recorded result](recorded/result.json) retains two actual EAL acquisitions
on 28 October 2026, the executable [package](recorded/package/case.eal), its
concrete [binding](recorded/package/tools.toml), the native generated source,
[frozen contract](recorded/frozen-contract.json) and
[verification receipt](recorded/package/tools/generated/dependency_audit/verification.json).
The native package, canonical argument/support, contract and candidate were
copied without edits. Collection and assessment JSON files are structured
extracts from the actual EAL responses. The frozen cases are verification inputs;
the actual collections read the selected image's filesystem.

| Actual acquisition | Installed distributions | Checked constraints | Violations | Satisfaction |
|---|---:|---:|---:|---|
| `/usr/local/lib/python3.13/site-packages` | 50 | 93 | 0 | true |
| `/tmp` (empty-directory control) | 0 | 2 | 2 | false |

The successful implementation attempt used one coding call and passed all ten
native verification cases. A subsequent run with the current release, no model
calls and provider credentials removed reproduced the positive result using the
exported binding. The negative control used the same source, binding and five
acceptance predicates, changing only the metadata-directory input. It rejected
satisfaction, zero violations and minimum inventory; completeness and counting
still passed. It does not refute the original image claim.

All five empirical predicates passed in the positive run, and EAL supported the
measured ground G1. The overall claim remains **unsupported**: the authored
binding/implementation qualification and challenge obligations have not received
accepted reviews. The retained authored report and limitations describe their
state before collection. Actual values are in `recorded/positive/collection.json`
and `recorded/positive/assessment.json`; this README supplies no missing review.
Finite test cases and dependency metadata also do not establish runtime or
scientific validity.

Inspect the historical records without Docker or provider access. To execute the
archived package again, the exact local image ID above must exist and the EAL
companions must be installed. From the repository root:

```sh
"$EAL_HOST" run \
  examples/assess-claim/python-dependencies/recorded/package/case.eal \
  --tools examples/assess-claim/python-dependencies/recorded/package/tools.toml \
  --workspace work/python-dependencies-replay-001 \
  --claim claim_63757272656e745f636c61696d --timeout 300
```

Use `recorded/negative-control/case.eal` with that same manifest and a new
workspace to rerun the empty-directory control. Archived observations retain
their original times; rerunning produces fresh observations under the authored
3600-second freshness bound. If your rebuilt image has a different ID, follow
the new-investigation procedure above. Keep the archived package, receipts and
pins unchanged.
