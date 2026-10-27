# Support-domain workflow study

`check-support-domains.py --workflow` runs a frozen sequence through final assessment. Its phase recipes select the real EngArg commands and operator-owned measurement configuration. The existing invocation without `--workflow` still measures authoring alone.

The complete study has two distinct acceptance questions. Command assertions establish that the configured workflow produced its specified result. Independent reviewers establish whether its requirements and criteria support the stated claim. Passing a protocol fixture cannot supply measurement qualification.

## Freeze the investigation

Copy `workflow.example.json` and replace its operator paths, producer revision, configuration and qualification procedure. Add each required executable, profile, catalogue and other immutable input to `assets`. Assets are identified by their bytes. The freezer also includes the selected EngArg binary, EAL host, tools manifest and the planning configuration’s collector and connection files. Add every referenced implementation, schema and immutable resource to `assets`. Secret values belong in environment variables. Retained commands contain variable names rather than credential values.

The example selects one algorithm case to probe the generated-tool lifecycle. It is an exploratory failure-mode probe, with one repetition and a maximum of twenty-two EngArg authoring and implementation model calls. External qualification calls are outside that bound and require separate accounting. This sample cannot estimate a population success rate or a tail latency. The six other domain cases retain distinct reference obligations for identity, performance, statistical reliability, API conformance, exact arithmetic and independent design judgement. Select more cases and repetitions only after specifying the precision and resource question they answer.

Each selected case has a `development` or `holdout` partition fixed before inference. Cases are the independent units. Repeated runs remain nested within cases. A confirmatory configuration must supply a decision rule and its justification. The harness retains that rule but does not automatically establish its statistical assumptions or declare a confirmatory decision.

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

The second command rechecks frozen input and harness hashes before every phase. Both output directories must be new. The protocol contains the cases, references, arm order, repetition count, command recipes, call bound, binary/configuration hashes, source revision and local source-diff hash. The first arm alternates across repetitions. Provider model identifiers and prompt hashes are retained where available.

## Configure the complete workflow

A generated-tool recipe contains `implement`, `qualify`, `build`, `collect` and `assess`, in that order. The driver prepends `analyse`. An existing-tool recipe starts with `build`. Both direct and optional discovery authoring feed the same canonical report and package compiler. The qualification phase invokes an executable selected by its hashed asset name, with a hashed measurement reference. That procedure must review the generated package and bind its actual identity to the retained semantics. The harness supplies no automatic truth judgement.

The collection and assessment recipes explicitly select `PATH` and `HOME` with `--collector-env` and classify those nonsecret settings with `--public-collector-env`. The tools registry must admit each selected name in `inherit_env`. Forwarding a name treats its value as secret unless the operator explicitly classifies it as public; credential names cannot be downgraded. Add every further name required by the qualified runtime, including variables outside the host's standard process environment such as `DOCKER_HOST`, `DOCKER_CONFIG` or `XDG_RUNTIME_DIR`. Classify only reviewed nonsecret settings as public and keep source credentials sensitive. Use the same environment selections for collection and assessment. Retain variable names in the frozen recipe and supply their values through the operator environment.

Each phase supplies an argv array, a finite timeout and outcome assertions. `build` invokes `engarg build` with the implemented report and merged `tools.toml`. Collection and assessment consume the package’s `case.eal` and `tools.toml`. There is no registration or executable graph sidecar. The study requires the selected EAL host and explicit tool bindings before authoring. Implementation has a separate call bound and zero automatic HTTP retries. A phase stops its arm when the process fails, times out, exceeds the 16 MiB log bound or fails an assertion. The other arm and its consumed resources remain recorded.

Recipe paths may use `{binary}`, `{config}`, `{model}`, `{api_key_env}`, `{analysis}`, `{work}`, `{now}` and named assets. `{work}` is a new private directory per case, repetition and arm. `{now}` is the actual UTC time immediately before a phase. A capture extracts one scalar from an earlier result and makes it available to later argv entries. Capture names must be new identifiers. They cannot replace reserved settings, pinned asset bindings or earlier captures. The example passes the actual collection identifier into assessment and asserts that it was retained. Substitution is literal and commands run without a shell.

Assertions select an exact JSON pointer. `equals` requires every selected value to match, `any_equals` requires at least one match, `nonempty` requires nonempty values and `exists` requires a selection. An explicit `*` pointer component selects every child of an object or array. The example's final claim identifier must be captured from the exported bundle so assessment checks the intended claim.

A topology that cannot represent a required operation uses `unrepresentable: true` with a frozen reason. This records a capability difference and contributes no successful-pair ratio. It must not be converted to a low-cost successful run. The example supplies both direct and discovery authoring arms. They retain the same obligations and use the same direct package execution workflow.

## Account for every attempted phase

The results retain phase elapsed time, return status, assertions, command, log hash and declared output hashes after every attempted phase. A changed frozen input stops the study and preserves the completed phases and their consumed resources. Model usage comes from phase-local audits. Implementation does not recount the authoring transcript retained in its input report. Failed-call usage stays unknown, alongside the sum of available token counts. A verified package reuse with a reported zero-call implementation stage contributes zero model tokens.

Discovery and acquisition counters are read from explicitly named result pointers. Missing counters remain unknown. An emitted zero must have a measured or command-contract basis. Request and response bytes describe the provider's retained measurement boundary, which may exclude HTTP/TLS framing. Qualification helper I/O remains unknown unless its receipt supplies counters. Its model calls and tokens also remain unknown by default. A `model_usage_receipt` selects a retained JSON object with `calls`, `input_tokens` and `output_tokens` through `file` and `pointer`. Alternatively, an explicit `deterministic_attestation` identifies a `reviewer`, `reason` and hashed-asset `reference` for a zero-model-call assertion. Results distinguish this operator assertion from reported usage. The harness also retains generated-package counts and source-reuse counts when supplied. Include all expected outputs under `artefacts` to retain their hashes and sizes.

Whole-workflow totals include failed arms and deterministic phases. Successful-pair ratios are separately labelled as conditional on workflow acceptance. Repetitions are averaged within each case before reporting cross-case descriptive ratios. The case range describes observed variation. The harness supplies no invented confidence interval for purposively selected cases. Semantic adequacy must be reviewed before a topology recommendation.

The default run directories isolate local outputs. A configured source store, package store, compiler cache or provider cache can still be shared. Declare each cache in `cache_treatment`. Controlled cold/warm studies require separate recipes/configurations that explicitly select or initialise those stores and retain the initial state. Provider caching remains uncontrolled unless the provider exposes a usable control. A finite call bound is not a monetary budget. Monetary estimates require dated rates and known billing classes, neither of which the harness invents.

## Review the retained outputs

`reference.json` contains obligation-level reference criteria authored from the claims before model outputs were collected. These are candidate expert references. Independently review them before using them as confirmatory labels. The error rubric defines critical omissions, false readiness, units, methods, scope, time, lost challenges and unjustified code generation.

`review-template.json` identifies each original analysis by SHA-256. Two reviewers independent of the authoring run label every frozen obligation and identify errors with reasons. Coverage is the count labelled `covered` divided by all frozen obligations for that case. Partial, missing and unrepresentable obligations remain in the denominator. A run is semantically adequate only when both reviewers agree on complete coverage and zero errors. Disagreements remain `disputed`; the supplied reviews and reasons remain in the output. An independent adjudicator can add an `adjudications` entry containing the same run/output identities and labels, their reviewer identity, the original `reviewers` identities and reasons. Scoring retains both original reviews and their disagreement alongside the adjudicated outcome.

```sh
python3 scripts/check-support-domains.py \
  --score-results measured-study/results.json \
  --reviews independent-reviews.json --output-dir reviewed-study
```

Scoring recomputes the retained protocol digest and rejects changed reference obligations or changed analysis, workflow artefact or log bytes. It verifies the retained protocol content without requiring the original runtime executables to remain installed. An unreviewed run remains unresolved even when every command succeeds. Error prevalence uses reviewed runs with one or more errors divided by all reviewed runs. Report this denominator separately from all attempted runs and report unreviewed/failing arms alongside it.

## Verification and evidence status

Run `python3 -m unittest discover -s tests -p test_support_domains.py -v`. The focused regressions cover the entire phase chain, collection binding, assertion failure, call accounting, frozen-input rejection, capture isolation, qualifier identity, process limits and independent-review disagreement. Fixture outputs are labelled as fixture evidence and establish harness behaviour only.

No live domain study has been retained for this implementation. A live API credential, configured source instruments, independent qualification procedure and reviewed study design are required to produce measured semantic-quality and complete-workflow cost evidence. The existence of this harness leaves those empirical acceptance conditions outstanding.
