# Execute an engineering investigation ticket

`engarg ticket` connects topology-driven argument authoring to executable evidence acquisition and assessment. The native `eal-coordinate` process owns the durable stage order and retains its reservations and exact worker receipts. EngArg supplies the domain worker. EAL-MCP performs the final collection and registered assessment.

| Stage | Work | Required retained outputs |
| --- | --- | --- |
| `author` | Run the selected topology and produce a typed argument and evidence design | Analysis report and debate journal |
| `integrate` | Activate approved MCP connections, replan, implement missing operations, qualify verified packages and register them, then replan | Ready analysis, planning configuration, connection configuration and registration receipt |
| `export` | Publish the executable argument and its qualified evidence routes | EAL source, tools TOML and JSON bundle |
| `execute` | Register the source, plan the selected claim, collect evidence and invoke registered assessment through EAL-MCP | Source registration, collection, registered assessment and full assessment |

Every completed stage binds its output files by SHA-256 and byte count. The controller checks prior artifacts before starting the next stage. Terminal execution checks connect the exported source, registered entry, initial collection, final assessment collection and selected claim. Compatible evidence reuse can give the registered assessment a different collection identity, and both identities remain explicit.

## Submit a ticket

Install the revision-pinned native companions with `scripts/install-eal-runtime.sh`. Its `bin` directory includes `eal`, `eal-host`, `eal-coordinate` and their supervised runtime companions. Build or install EngArg together with `engarg-process-supervisor`.

Create an operator-owned JSON specification:

```json
{
  "schema_version": "engarg.ticket/1",
  "ticket_id": "catalogue-audit",
  "request": {
    "claim": "The current catalogue snapshot contains three distinct items.",
    "context": "Use the approved catalogue source and the declared snapshot scope.",
    "evidence": []
  },
  "debate": {
    "participants": 6,
    "rounds": 1,
    "topology": { "kind": "ring" },
    "adaptation": { "kind": "static" }
  },
  "evidence_config": "/operator/planning.json",
  "mcp_registrations": "/operator/approved-mcp-connections.json",
  "qualification_argv": ["/operator/qualify-generated-operation"],
  "review_binding": "/operator/independent-review.binding.json",
  "authoring_args": [
    "--model", "YOUR_MODEL",
    "--api-key-env", "OPENAI_API_KEY",
    "--request-timeout-secs", "180"
  ],
  "implementation_args": [
    "--model", "YOUR_MODEL",
    "--api-key-env", "OPENAI_API_KEY",
    "--tool-language", "rust",
    "--toolchain", "/operator/rust-toolchain.json",
    "--tool-store", "/operator/verified-tools",
    "--tool-max-calls", "4"
  ],
  "eal_host": "/runtime/bin/eal-host",
  "eal_application": "/runtime/bin/eal",
  "environment": ["PATH", "HOME", "OPENAI_API_KEY"],
  "collector_environment": [],
  "timeout_seconds": 7200,
  "stage_timeout_seconds": 3600,
  "input_files": []
}
```

The `/operator` paths name your existing approved configurations, policies and toolchain. Relative paths are resolved against the ticket specification. The language toolchain profile identifies an installed immutable Docker image. The Rust image can be built with `docker build -t engarg-rust-tools:1.99.1 examples/toolchains/rust`.

```sh
engarg ticket --spec ticket.json --run investigation-001 \
  --eal-coordinate /runtime/bin/eal-coordinate

/runtime/bin/eal-coordinate ticket-status --run investigation-001/journal
/runtime/bin/eal-coordinate ticket-export --run investigation-001/journal \
  --output investigation-001/ticket-export.json
```

The workspace must be new. The ticket identifier is bounded to 80 ASCII letters, digits, underscores or hyphens. Total deadlines range from 1 through 86,400 seconds. Each stage has a deadline of at most 3,600 seconds within that total. An omitted authoring call allowance is calculated from the topology and repair allowance. Implementation has its own configured allowance.

Only environment names declared in `environment` reach the ticket workers. Every declared name must exist. Names in `collector_environment` must also be declared in `environment` and in the resulting executable registry. Values remain outside the ticket specification and journal. The default worker environment contains `PATH` and `HOME`. Provider credential names must be granted explicitly.

## Configure the evidence authorities

`mcp_registrations` is an `eal.evidence-config/1` document containing approved connection and catalogue additions. Integration validates identity conflicts, activates the connections and performs actual discovery before deciding whether code is required. Transport endpoints, executable locations, credential references and measurement qualifications come from this operator configuration.

The optional qualification command receives one JSON request on stdin with schema `engarg.ticket-qualification/1`, the retained analysis path, generated provider templates and operation contracts. It returns a qualified `eal.evidence-config/1` native-provider document. Existing package, schema, transport and source identities are checked before registration. A generated implementation's synthetic fixture results establish its tested function behaviour. An operator policy supplies its measurement qualification.

`review_binding` selects an independent reviewer through a version-1 `CommandBinding`. It is validated by the same executable-registry contract used by EAL. The exported `review-requests.json` supplies the exact request envelope. Required review obligations remain pending until a selected reviewer returns their observations. Reviewer credential names require explicit collector forwarding. Include policy scripts and other dependencies in `input_files` so their identities are checked at stage admission.

## Interpret the result

The final `ticket-result.json` separates investigation execution from claim support. A fully executed valid assessment has ticket status `completed`, while its claim may be `supported`, `contested` or `unsupported`. A measured predicate failure remains a completed investigation. Missing or expired observations, failed acquisition, unavailable MCP connections and unresolved qualifications produce a blocked or failed stage with retained evidence of what happened.

Artifacts and input snapshots remain under the workspace. The controller uses bounded summaries and files of at most 16 MiB per artifact, with 64 MiB across the retained artifact set. Scientific values remain in their exact artifact files. Ticket context and outcome summaries have separate 64 KiB bounds.

Completed stages can be inspected again without repeating their effects. A reserved stage with an uncertain outcome is never automatically rerun. Inspect its exact receipts and artifacts before submitting a separately identified investigation. `ticket-cancel` stops admission of new work and propagates cancellation through the supervised process tree.

## Native acceptance

`tests/ticket_native.rs` exercises every topology through a newly registered local MCP, a compiled Rust collector and actual EAL-MCP collection and registered assessment. Its Docker variant generates and compiles the missing Rust implementation inside the ticket. Negative tests cover unavailable MCP acquisition, rejected compilation and measured counter-evidence. The finite fixture's reviewer checks its declared finite interpretation and source facts independently. These tests exercise the workflow mechanics rather than establishing the adequacy of an arbitrary engineering claim.
