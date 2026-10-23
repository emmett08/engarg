# EAL/3 translation contract

EngArg exports an authored engineering argument into EAL/3. The result is a structural projection with explicit review obligations. It is suitable for inspection and EAL assessment after admissible observations have been collected. Conversion alone supplies neither those observations nor a proof of the natural-language argument.

The integration was developed against [eal-mcp revision d311d5a4f9e294ba3228127bc5210b48c5de3a12](https://github.com/emmett08/eal-mcp/tree/d311d5a4f9e294ba3228127bc5210b48c5de3a12). EngArg invokes that project's official `eal-host`, which negotiates MCP with `eal-mcp`. This reuses its tool-schema validation, authentication and exact-number handling rather than maintaining another MCP implementation.

## Representation

| EngArg concept | EAL/3 projection | Qualification |
| --- | --- | --- |
| Current claim, stronger target and subclaims | Named claims with scope and qualifier in the statement | Wording does not become a formal predicate |
| Ground | Claim supported by source and interpretation review evidence | Reported or user-checked contents remain unverified by conversion |
| Warrant | `structured/1` reasoning and a candidate-route claim | A deductive label alone cannot justify a computational deduction method |
| Warrant premises | Joint premises of one route argument | All required premises travel together |
| Jointly required contributions | One composition argument with all selected route claims as premises | Several independent EAL arguments would mean alternatives and would change the meaning |
| Cumulative support | The same conjunction plus a composition adequacy obligation | No numerical combination, independence or sufficiency is inferred |
| Alternative routes | Only the selected route connects to the canonical conclusion | Unselected candidates remain queryable as route claims. Reselection requires a new authored assessment |
| Critical assumption | Validated EAL assumption attached to affected support | A model's supported label is retained as metadata |
| Challenge | Evidence-gated objection at the matching claim, reasoning, assumption or objection | Its activation requires admissible support |
| Open or sustained material challenge | Additional resolution gate on affected support | An unsupported objection must not silently remove an existing EngArg reliance condition |
| Evidence plan, alternatives and reading prose | Preserved in the JSON bundle | No executable experiment or decision authority is fabricated |

An objection that relies on a critical assumption receives an auxiliary premise claim. That claim has a native EAL argument requiring the assumption and its validation evidence. An accepted attack on the assumption can therefore defeat the dependent objection, as well as ordinary arguments that require it. The auxiliary bridge has no claim premises of its own and adds no premise cycle.

Source identity, ground interpretation, warrant adequacy, assumption validation, composition adequacy, challenge activation and consequential challenge resolution are separate review contracts. Each requires `accepted == true` from its own observation. `engarg_review_contract` declares an interface. It does not install a collector or create a trusted reviewer.

Review contracts use an explicit 86,400-second maximum age. This is an export policy for the review observations, not a claim that the original artefacts expire daily. The generated context digest binds observations to the authored case revision. It does not establish that the real environment meets the declared conditions. Review-policy changes require a reviewed revision of the exporter and a new assessment.

## Identity and preservation

Identifiers use category prefixes and injective hexadecimal encoding of their original UTF-8 bytes. Text and contract inputs use JSON string encoding. User text cannot introduce EAL declarations. The original claim identifiers map to queryable EAL names in `claim_names`, and `source_map` identifies the generated declarations and their roles.

The JSON bundle contains the exact original request and case, generated source, context, source map, obligations and limitations. It therefore preserves information that the executable projection cannot express. The export revision hashes the canonical input, exporter version and schema version. This is an identity mechanism, not an authentication signature.

Keep the source bytes unchanged between collection and assessment. A source formatting change can change the EAL source identity and invalidate observation reuse. EngArg does not format or repair a source before sending it to the host. When reading its JSON bundle, it checks the canonical projection for consistency. Edited EAL can be supplied explicitly as raw source.

## Assessment and boundaries

`engarg eal assess` invokes EAL's `reason` operation. Without a collection identifier, the server evaluates an empty observation set. Missing observations leave support unavailable. That result does not refute the engineering proposition. A successful CLI exit indicates a completed operation, not acceptance of every claim. Inspect the returned claim statuses and explanations.

EngArg does not automatically collect observations or execute commands described in an argument. Operators can configure trusted collectors and perform explicit collection with the EAL tools, retaining the exact source, context and database, then pass the resulting collection identifier to assessment. The declared review interface must have an appropriate implementation before those contracts can be discharged. Inventing accepted records would destroy the evidential meaning of the case.

The adapter uses bounded input and output, a deadline, process cleanup and no shell or hidden retry. The host deadline is followed by a one-second cleanup allowance before forced termination. Remote HTTP endpoints require TLS and a named bearer-token environment variable. Child stderr and credential values are not incorporated into error reports. Semantic validation diagnostics and tool errors are returned before the CLI reports failure.

## Architecture decisions

The pure exporter owns graph translation and has no process, network or model dependency. The host adapter owns the external process boundary. The CLI module owns argument parsing, bounded file input and operation selection. The model adapter remains separate. This applies single responsibility and dependency inversion without introducing an inheritance hierarchy or a generic agent framework.

The adapter pattern is justified by a separately versioned external host. A fixed workflow remains appropriate for the six bounded model stages. The exporter uses a private builder to retain one source map, obligation register and declaration order during translation. Its orchestration methods remain cohesive because each follows a single translation sequence and delegates declaration encoding to focused helpers. CLI host dispatch and file handling are separated from model analysis, rather than extending the analysis function with unrelated EAL operations.

## Reproduce integration checks

Install the reference EAL binaries as described in the README, then run:

```sh
ENGARG_EAL_HOST=/absolute/path/to/eal-host cargo test --locked --test eal_live -- --ignored
```

These tests require the real host and sibling server/runtime binaries. The normal test suite uses deterministic local doubles for process and API failure cases. Neither kind of test establishes the truth of a proposed engineering claim or comparative argument quality.
