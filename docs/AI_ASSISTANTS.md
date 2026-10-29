# Author arguments with installed AI assistants

[Documentation](index.md) · [Engineer workflow](USAGE.md)

EngArg can send its authoring calls to Claude Code, Codex, Copilot CLI, Gemini
CLI, Cursor or Auggie. Select a native preset below, or use `--provider command`
for an adapter. The default `--provider http` retains the Responses and Chat
Completions integrations.
Every provider returns the same completion contract. EngArg still checks the
authored JSON, retains the exchanges and preserves missing evidence and dissent.

These options apply to `analyse`, `authoring resume`, `eal repair` and
`eal implement`. Tickets accept them in `authoring_args` and implementation
`provider_args`. The [claim-assessment script](assess-any-claim.md) also forwards
the selected provider to authoring and implementation.

## Select an installed assistant

Install and authenticate the selected CLI using its own instructions. Choose an
exact model identifier available to that assistant. Command providers use the
CLI's authentication; EngArg's HTTP API-key setting does not authenticate them.

| Provider | Default executable | Supported noninteractive profile |
|---|---|---|
| `claude` | `claude` | Claude Code print mode, one JSON success result, safe mode and explicit tool/MCP restrictions. |
| `codex` | `codex` | `exec --json`, one completed JSONL turn, a requested read-only sandbox and ephemeral session. |
| `copilot` | `copilot` | Final text with silent, non-streaming output, no user questions and explicit tool denials. |
| `gemini` | `gemini` | One JSON response, default approval policy and extensions disabled. |
| `cursor` | `agent` | Print/ask mode with a terminal JSON success envelope. |
| `augment` | `auggie` | Stable Auggie 0.x print/quiet mode with final response text. |

The presets target these documented flag and output profiles. Use a CLI version
that supports the selected profile; EngArg does not silently fall back to older
flags or interactive execution. A different profile can use a `command` adapter.

Using the request and tool manifest from [the engineer workflow](USAGE.md):

```sh
engarg analyse --input request.json --tools tools.toml \
  --provider claude --model YOUR_CLAUDE_MODEL_ID \
  --request-timeout-secs 600 --timeout-secs 2400 \
  --max-calls 7 --max-repairs 1 --format json \
  --audit claude.audit.jsonl --checkpoint claude.checkpoint.json \
  --output claude-analysis.json
```

Select `--provider codex`, `--provider copilot`, `--provider gemini`,
`--provider cursor` or `--provider augment` with that CLI's model identifier to
use the same workflow through another preset.

Claude Code uses `--print --input-format text --output-format json --model MODEL`
with `--safe-mode`, empty built-in tools, a denied MCP tool pattern, an empty
strict MCP configuration and no session persistence. Safe mode retains the CLI's
normal authentication; the preset does not select `--bare`, which changes its
authentication requirements. Managed policy hooks can still apply. EngArg admits
only a completed success result and preserves reported usage when its counters
are complete and valid. See the official [headless guide](https://code.claude.com/docs/en/headless)
and [CLI reference](https://code.claude.com/docs/en/cli-reference).

Codex uses `exec --json --sandbox read-only --skip-git-repo-check --ephemeral
--color never --model MODEL -`. The prompt arrives on stdin. EngArg admits the
final agent message only after a successful terminal turn; progress messages,
failed turns and truncated JSONL streams do not become completions. See the
official [noninteractive guide](https://developers.openai.com/codex/noninteractive/).

Copilot CLI uses its documented final-text interface with silent, non-streaming
output rather than assuming that its JSONL events match the SDK protocol. The
preset disables built-in MCP servers, denies its named file, shell, URL and
memory tools, and disables user questions. Custom MCP servers and hooks remain
part of the trusted operator configuration. Model identity and usage are not
inferred from the response text. See the official
[programmatic reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference)
and [command reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference).

Gemini CLI uses `--output-format json --model MODEL --approval-mode default
--extensions none` with stdin input. EngArg extracts the final response and
rejects reported errors. This preset does not claim a read-only sandbox: its
default approval policy and operator MCP configuration still apply. It targets
Gemini CLI, not every Gemini Code Assist IDE interface. Its session statistics
can combine routed models and several token categories; the preset leaves
actual model identity and per-completion usage unknown rather than inventing a mapping.
See the official
[headless guide](https://geminicli.com/docs/cli/headless/) and
[configuration reference](https://geminicli.com/docs/reference/configuration/).

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
The schema is prompt context and EngArg's own validation is authoritative.
The Claude and Codex presets do not use their optional native schema-enforcement
flags; a JSON result envelope alone does not establish schema conformance.
`--role-model sceptic=MODEL` can select another model for that authoring role.

## Configure the executable and environment

Native presets search for their default executable on `PATH` and inherit only
available `PATH` and `HOME` values by default. Supply
`--assistant-config assistant.toml` to replace the executable argument vector and
inherited environment names:

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
instruction, not a provider-enforced token cap. Output byte limits and the
caller's logical deadline remain enforced. Logical-call budgets count CLI invocations; a native assistant
may make internal inference requests that EngArg does not observe.

Cleanup has its own lifetime: a timed-out call can return while its worker still
retains the concurrency slot and directory pending supervisor acknowledgement.
This wait does not block Tokio runtime shutdown. If the whole EngArg process
exits before acknowledgement, detached-thread destructors cannot remove that
directory; the independent supervisor still owns process cancellation.

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
metadata only when known. Native presets label a configured model fallback as
`assistant:PROVIDER:MODEL` when the CLI does not report its actual model. Unknown
token usage remains unset. Session or thread
identifiers are not fabricated HTTP request identifiers.

Reports retain the selected assistant and configuration digest without copying
the executable arguments or environment values into provider metadata. Historical
HTTP reports retain their original shape. Structural checks establish consistency
of the retained authoring records; they do not authenticate an assistant or turn
its claims into observations. Build and execute the resulting investigation
through the normal [EAL workflow](USAGE.md#5-build-and-fulfil-scoped-reviews).

An assistant connecting to EAL's MCP server is a separate direction of integration.
These providers let EngArg ask an assistant to author a completion; they do not
configure that assistant's MCP servers or change the checked `aspic-view/3`
contract consumed by the ASPIC inspection app.
