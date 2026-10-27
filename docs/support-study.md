# Evaluate the complete workflow

[Documentation](index.md) · [Evaluation design](EVALUATION.md)

An authoring run can produce plausible requirements while its evidence workflow
still fails to measure the intended property. This study harness follows the
investigation through final assessment so that those outcomes remain visible.
`check-support-domains.py --workflow` executes frozen recipes using real EngArg
commands and operator-owned measurement configuration. Without `--workflow`, it
measures authoring alone.

The study asks two separate questions. Command assertions check whether the
configured workflow produced its specified result. Independent reviewers assess
whether the requirements and criteria support the stated claim. A passing
protocol fixture establishes the configured execution behaviour. Measurement
qualification requires the second judgement and its supporting records.

## Choose the question before the cases

The supplied [workflow example](../experiments/support-domains/workflow.example.json)
selects one algorithm case to probe the generated-tool lifecycle. It has one
repetition and permits at most twenty-two EngArg authoring and implementation
model calls. External qualification calls sit outside that bound and need
separate accounting. This exploratory sample can expose a failure mode. It cannot
estimate a population success rate or tail latency.

Six further domain cases provide distinct reference obligations for identity,
performance, statistical reliability, API conformance, exact arithmetic and
independent design judgement. Select cases and repetitions according to the
precision and resource question the study is intended to answer. Cases are the
independent units, with repeated runs nested within each case.

Fix each selected case's `development` or `holdout` partition before inference.
A confirmatory configuration also requires a decision rule and justification.
The harness retains that rule. Establishing its statistical assumptions and
applying a defensible confirmatory decision remain responsibilities of the study.

## Freeze the inputs that determine the investigation

Copy the workflow example and replace its operator paths, producer revision,
configuration and qualification procedure. The `assets` map identifies immutable
inputs by their bytes. Include every required executable, toolchain configuration,
catalogue, referenced implementation, schema and immutable resource.

The freezer also includes the selected EngArg binary, EAL host, tools manifest,
and the planning configuration's collector and connection files. Keep secret
values in environment variables. Retained commands identify the variable names
rather than storing their values.

```sh
python3 scripts/check-support-domains.py \
  --binary target/release/engarg --evidence-config /operator/planning.json \
  --tools /operator/tools.toml --eal-host /operator/bin/eal-host \
  --model YOUR_MODEL --case specified_algorithm --repetitions 1 \
  --workflow /operator/study.json --freeze-only --output-dir frozen-study

# Execute exactly the frozen inputs after configuring OPENAI_API_KEY.
python3 scripts/check-support-domains.py \
  --frozen-protocol frozen-study/protocol.json --output-dir measured-study
```

Both output directories must be new. The second command rechecks frozen inputs
and harness hashes before every phase. The protocol retains the cases and
reference obligations alongside the arm order, repetitions, recipes and call
bound. Binary and configuration hashes, the source revision and the local source
diff hash identify the implementation used. Provider model identifiers and prompt
hashes are retained where available. The first arm alternates across repetitions.

## Make the phase chain executable

For a generated tool, the recipe orders `implement`, `qualify`, `build`, `collect`
and `assess`. The driver prepends `analyse`. An existing-tool recipe begins at
`build` after authoring. Both direct and optional discovery authoring produce the
same canonical report and use the same package compiler.

Qualification invokes an executable selected by its hashed asset name, using a
hashed measurement reference. That procedure must review the generated package
and bind its actual identity to the retained measurement semantics. The harness
records the result without supplying its own truth judgement.

Every phase has an argv array, a finite timeout and outcome assertions. `build`
consumes the implemented report and merged `tools.toml`. Collection and assessment
consume the resulting `case.eal` and `tools.toml`. The study requires the selected
EAL host and explicit tool bindings before authoring. Implementation has its own
call allowance and zero automatic HTTP retries.

A process failure, timeout, assertion failure or log exceeding 16 MiB stops that
arm. The results preserve completed phases and consumed resources, including the
other arm's work. A capability that the selected approach cannot express uses
`unrepresentable: true` with a frozen reason. It contributes no successful-pair
ratio and cannot be counted as an inexpensive success. The example's direct and
discovery arms retain the same obligations and direct execution path.

### Grant the environment needed by the measured runtime

The collection and assessment recipes forward `PATH` and `HOME` through
`--collector-env` and classify them with `--public-collector-env`. The tools
manifest must also admit each name through `inherit_env`. A forwarded value is
classified as secret unless the operator explicitly marks it public. Credential
names retain their sensitive classification.

Add every further name needed by the qualified runtime. Docker may require
settings such as `DOCKER_HOST`, `DOCKER_CONFIG` or `XDG_RUNTIME_DIR`, which are
outside the host's standard process environment. Classify only reviewed nonsecret
settings as public. Use identical environment selections for collection and
assessment, retaining names in the frozen recipe and supplying values from the
operator environment.

### Bind later phases to actual earlier results

Recipe paths can use `{binary}`, `{config}`, `{model}`, `{api_key_env}`,
`{analysis}`, `{tools}`, `{eal_host}`, `{work}`, `{now}` and named assets.
`{work}` is a new private directory for each case, repetition and arm. `{now}` is
the actual UTC time immediately before the phase. Substitution is literal and
commands run without a shell.

A capture extracts one scalar from an earlier result for use in later argv
entries. Its name must be a new identifier, leaving reserved settings, pinned
asset bindings and earlier captures unchanged. The example captures the intended
claim identifier from the package build receipt, then passes the actual
collection identifier to assessment and asserts that it was retained.

Assertions address an exact JSON pointer. A `*` component explicitly selects all
children of an object or array. The available predicates have different scopes:

| Assertion | Acceptance condition |
| --- | --- |
| `equals` | Every selected value matches |
| `any_equals` | At least one selected value matches |
| `nonempty` | Selected values are nonempty |
| `exists` | The pointer selects a value |

## Account for failed work as well as successful work

After each attempted phase, results retain elapsed time, process status,
assertions, command, log hash and declared output hashes. A changed frozen input
stops the study while preserving earlier work. Include all expected output files
under `artefacts` so that their hashes and sizes form part of the record.

Model usage comes from each phase's own audit. Implementation therefore avoids
recounting the authoring transcript embedded in its input report. A verified
package reused by an explicitly reported zero-call implementation contributes
zero model tokens. Failed-call usage remains unknown alongside the sum of token
counts that were reported.

Discovery and acquisition counters come from explicitly named result pointers.
Missing counters remain unknown, and an emitted zero needs a measurement or
command-contract basis. Request and response byte counters describe the
provider's declared measurement boundary, which may exclude HTTP/TLS framing.
Generated-package and source-reuse counts are retained when supplied.

Qualification helpers require separate accounting. Their I/O, model calls and
tokens remain unknown unless the receipt supplies them. A `model_usage_receipt`
selects a retained JSON object through `file` and `pointer`, with `calls`,
`input_tokens` and `output_tokens`. Alternatively, `deterministic_attestation`
records a `reviewer`, `reason` and hashed-asset `reference` asserting zero model
calls. Results distinguish that operator assertion from reported usage.

Whole-workflow totals include failed arms and deterministic phases.
Successful-pair ratios are reported separately, conditional on workflow
acceptance. Repetitions are averaged within each case before cross-case
descriptive ratios are calculated. The case range describes observed variation.
Purposively selected cases do not acquire a confidence interval merely because
the harness can calculate ratios. Review semantic adequacy before recommending
an authoring approach.

Separate output directories isolate local results, while source stores, package
stores, compiler caches or provider caches may still be shared. Declare every
cache in `cache_treatment`. A controlled cold/warm comparison needs separate
recipes or configurations that select or initialise those stores and retain
their initial state. Provider caching remains uncontrolled unless the provider
exposes a usable control. The finite call allowance also remains distinct from
a monetary budget, which requires dated prices and known billing classes.

## Review the retained meaning, not just the exit codes

The [reference obligations](../experiments/support-domains/reference.json) were
authored from the claims before model outputs were collected. They are candidate
expert references and require independent review before use as confirmatory
labels. Their error rubric covers critical omissions and false readiness, as
well as defects in units, methods, scope, time, retained challenges and the
justification for generated code.

`review-template.json` identifies each original analysis by SHA-256. Two reviewers
independent of the authoring run label every frozen obligation and record errors
with reasons. Coverage is the number labelled `covered` divided by all frozen
obligations for that case. Partial, missing and unrepresentable obligations stay
in the denominator.

A run is semantically adequate only when both reviewers agree on complete
coverage and zero errors. Disagreement remains `disputed`, with both reviews and
reasons retained. An independent adjudicator may add an `adjudications` entry
using the same run/output identities and labels, their own reviewer identity,
the original `reviewers` identities and reasons. Scoring preserves the initial
disagreement alongside that adjudicated outcome.

```sh
python3 scripts/check-support-domains.py \
  --score-results measured-study/results.json \
  --reviews independent-reviews.json --output-dir reviewed-study
```

Scoring recomputes the retained protocol digest and rejects changed reference
obligations or changed analysis, workflow artefact or log bytes. It verifies
retained protocol content without requiring the original runtime executables to
remain installed. A run without review remains unresolved even when every
command succeeds.

Error prevalence is the number of reviewed runs with at least one error divided
by all reviewed runs. Report this denominator separately from all attempted runs,
and show unreviewed and failing arms alongside it. Otherwise the apparent result
can improve merely by excluding difficult investigations.

## Establish the harness before interpreting a live study

Run the focused checks with:

```sh
python3 -m unittest discover -s tests -p test_support_domains.py -v
```

These regressions cover the phase chain, collection binding, assertion failure,
call accounting, frozen-input rejection, capture isolation, qualifier identity,
process limits and independent-review disagreement. Their outputs are labelled
fixture evidence and establish harness behaviour.

No live domain study has been retained for this implementation. Measuring semantic
quality and complete-workflow cost still requires a live API credential,
configured source instruments, an independent qualification procedure and a
reviewed study design. Use the [evaluation criteria](EVALUATION.md) to state which
claim those observations would justify before collecting them.
