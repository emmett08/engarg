# Author arguments with Cursor, Augment or another assistant

[Documentation](index.md) · [Engineer workflow](USAGE.md)

EngArg can send its authoring calls to an installed assistant CLI. Select
`--provider cursor`, `--provider augment` or `--provider command`; the default
`--provider http` retains the Responses and Chat Completions integrations.
Every provider returns the same completion contract. EngArg still checks the
authored JSON, retains the exchanges and preserves missing evidence and dissent.

These options apply to `analyse`, `authoring resume`, `eal repair` and
`eal implement`. Tickets accept them in `authoring_args` and implementation
`provider_args`. The [claim-assessment script](assess-any-claim.md) also forwards
the selected provider to authoring and implementation.

## Select an installed assistant

Install and authenticate the selected CLI using its own instructions. Choose an
exact model identifier available to that assistant. An OpenAI API key is not
required for these command providers.

Using the request and tool manifest from [the engineer workflow](USAGE.md):

```sh
engarg analyse --input request.json --tools tools.toml \
  --provider cursor --model YOUR_CURSOR_MODEL_ID \
  --request-timeout-secs 600 --timeout-secs 2400 \
  --max-calls 7 --max-repairs 1 --format json \
  --audit cursor.audit.jsonl --checkpoint cursor.checkpoint.json \
  --output cursor-analysis.json

engarg analyse --input request.json --tools tools.toml \
  --provider augment --model YOUR_AUGGIE_MODEL_ID \
  --request-timeout-secs 600 --timeout-secs 2400 \
  --max-calls 7 --max-repairs 1 --format json \
  --audit augment.audit.jsonl --checkpoint augment.checkpoint.json \
  --output augment-analysis.json
```

The Cursor preset invokes `agent --print --mode ask --output-format json
--model MODEL`. It admits a successful terminal result envelope and extracts
its response text. See the official [parameters](https://cursor.com/docs/cli/reference/parameters)
and [output format](https://cursor.com/docs/cli/reference/output-format).

The Augment preset targets **Auggie stable 0.x** and invokes
`auggie --print --quiet --model MODEL`. Its stdout must contain only the final
response. Auggie preview 1.x has different flags and event formats; use a
`command` adapter for that version. Check the official
[stable CLI repository](https://github.com/augmentcode/auggie),
[installation channels](https://docs.augmentcode.com/cli/setup-auggie/install-auggie-cli)
and [current reference](https://docs.augmentcode.com/cli/reference) when choosing
the installed version.

Each native invocation receives the role instructions, user context and JSON
Schema over stdin. The assistant must return JSON matching that schema. EngArg
does not convert prose or an incomplete response into an accepted argument.
`--role-model sceptic=MODEL` can select another model for that authoring role.

## Configure the executable and environment

Native presets search for `agent` or `auggie` on `PATH` and inherit only available
`PATH` and `HOME` values by default. Supply `--assistant-config assistant.toml`
to replace the executable argument vector and inherited environment names:

```toml
argv = ["/absolute/path/to/agent"]
inherit_env = ["PATH", "HOME", "CURSOR_API_KEY"]
```

Every explicitly listed variable must exist. Include any proxy, CA, configuration
directory or authentication variables the installed assistant needs. Keep secret
values in those variables, outside the TOML. Native presets append their protocol
and model flags to `argv`; use `command` for a different invocation protocol.
Argument vectors execute directly without shell expansion. Use absolute paths
for wrapper scripts and their resources.

EngArg starts each call in a fresh temporary directory. This avoids loading the
investigation checkout as the assistant's working directory; it is not a sandbox.
The selected executable remains a trusted local programme with its own account
and filesystem permissions. Cursor's preset selects its `ask` mode. An assistant's
own tools do not replace the separately configured EAL evidence acquisitions.

Calls use EngArg's process supervisor, explicit environment selection, independent
stdout/stderr byte bounds and a deadline. Cancellation owns descendant cleanup.
Command calls have no automatic retries. The logical-call and returned-content
budgets remain in force. For native presets, `--max-output-tokens` is a prompt
instruction, not a provider-enforced token cap; process byte and time limits
remain hard bounds. Logical-call budgets count CLI invocations; a native assistant
may make internal inference requests that EngArg does not observe.

## Adapt another assistant

Use `--provider command --assistant-config adapter.toml --model EXACT_MODEL_ID`.
For example:

```toml
argv = ["python3", "/absolute/path/to/my-assistant-adapter.py"]
inherit_env = ["PATH", "HOME", "MY_ASSISTANT_TOKEN"]
```

The adapter reads one UTF-8 JSON object from stdin:

```json
{
  "schema_version": "engarg.completion.v1",
  "model": "EXACT_MODEL_ID",
  "max_output_tokens": 10000,
  "request": {
    "role": "single",
    "system": "Role instructions supplied by EngArg",
    "user": "Question and supplied context",
    "schema": {"type": "object"}
  }
}
```

Invoke the selected assistant without an interactive session, pass through the
instructions and schema, then write one completion object to stdout and exit zero:

```json
{
  "content": "{\"the\":\"authored JSON response\"}",
  "model": "ACTUAL_MODEL_ID",
  "input_tokens": null,
  "output_tokens": null,
  "request_id": null,
  "service_tier": null,
  "reasoning_effort": null
}
```

`content` is a string containing the assistant's JSON response. The schematic
content above illustrates encoding, not a valid EngArg argument. Keep diagnostics
on stderr; use a nonzero exit for assistant failures. Report usage and model
metadata only when known. Native presets leave unknown usage unset and label a
configured model fallback as `assistant:cursor:MODEL` or
`assistant:augment:MODEL` when the CLI does not report its actual model.

Reports retain the selected assistant and configuration digest without copying
the executable arguments or environment values into provider metadata. Historical
HTTP reports retain their original shape. Structural checks establish consistency
of the retained authoring records; they do not authenticate an assistant or turn
its claims into observations. Build and execute the resulting investigation
through the normal [EAL workflow](USAGE.md#5-build-and-fulfil-scoped-reviews).
