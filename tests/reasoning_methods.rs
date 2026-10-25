//! Method-specific acceptance through the exported EAL/3 source and native kernels.
//!
//! Observations below come from a deterministic test bench, not production data.
//! Independent prose/application reviews are explicit fixture observations. They
//! cannot replace the typed method input, model, design or result predicate.

use eal_core::{
    ExecutionLimits, canonical, composition, evaluation, methods::default_registry,
    semantic::Program, validation,
};
use engarg::{
    domain::{ArgumentCase, validate_case},
    eal_export::{EalExport, export},
};
use serde_json::{Map, Value, json};

#[path = "common/reasoning.rs"]
mod fixtures;
use fixtures::{ModeCase, NOW, cases, envelope, fixture};

fn compile(bundle: &EalExport) -> Program {
    let program =
        composition::compile(&bundle.source, None, "", ExecutionLimits::default()).unwrap();
    let diagnostics = validation::validate(&program, default_registry());
    assert!(diagnostics.is_empty(), "{diagnostics:?}\n{}", bundle.source);
    program
}

fn digest(program: &Program, value: &Value) -> String {
    canonical::canonical_digest(value, &program.limits).unwrap()
}

fn record(program: &Program, id: &str, context: &Value, value: Value) -> Value {
    let evidence = &program.evidence[id];
    let tool = &program.tools[&evidence.tool];
    let acquisition = json!({"tool":tool.name,"tool_version":tool.version,"input":evidence.input,"context":context});
    let request = json!({"evidence_id":id,"environment":evidence.environment,"tool":tool.name,"tool_version":tool.version,"input":evidence.input,"context":context});
    json!({
        "evidence_id":id,"source_digest":program.source_digest,
        "tool":tool.name,"tool_version":tool.version,"tool_binding_digest":"0".repeat(64),
        "evidence_kind":evidence.kind,"environment":evidence.environment,
        "environment_fingerprint":evaluation::environment_fingerprint(&evidence.environment,context,&program.limits).unwrap(),
        "input_digest":digest(program,&evidence.input),"input":evidence.input,"context":context,
        "acquisition_request_digest":digest(program,&acquisition),"acquisition_request":acquisition,
        "request_digest":digest(program,&request),"collected_at":NOW,
        "run_id":"deterministic-mode-acceptance-test-bench","status":"ok",
        "data_digest":digest(program,&value),"value":value
    })
}

fn collect(program: &Program, context: &Value, mode: &ModeCase) -> Value {
    let mut records = Map::new();
    for (id, evidence) in &program.evidence {
        let value = if evidence.kind == mode.evidence_kind {
            // This deliberately uses the acquisition's retained request rather
            // than looking up evidence solely by its authored prose label.
            assert!(
                serde_json::to_string(&evidence.input)
                    .unwrap()
                    .contains("N5")
            );
            envelope(mode)
        } else {
            json!({"accepted":true,"fixture":"Independent application/prose review in the deterministic acceptance test bench only."})
        };
        records.insert(id.clone(), record(program, id, context, value));
    }
    Value::Object(records)
}

fn typed_input_id<'a>(program: &'a Program, mode: &ModeCase) -> &'a str {
    program
        .evidence
        .iter()
        .find(|(_, evidence)| evidence.kind == mode.evidence_kind)
        .unwrap()
        .0
}

fn computation<'a>(assessment: &'a Value, mode: &ModeCase) -> &'a Value {
    let method = format!("{}/1", mode.kind);
    &assessment["arguments"]
        .as_object()
        .unwrap()
        .values()
        .find(|argument| argument["reasoning_result"]["method"] == method)
        .unwrap()["reasoning_result"]
}

fn claim_name(bundle: &EalExport) -> &str {
    &bundle.claim_names["current_claim"]
}

#[test]
fn all_five_modes_project_typed_evidence_and_execute_their_native_kernel() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        let bundle = export(&request, &case).unwrap();
        let program = compile(&bundle);
        let input_id = typed_input_id(&program, &mode);
        let input = &program.evidence[input_id];
        assert_eq!(input.kind, mode.evidence_kind);
        assert!(
            bundle
                .source
                .contains(&format!("method \"{}/1\"", mode.kind))
        );
        assert!(input.predicates.iter().any(
            |predicate| predicate.path == "schema" && predicate.expected == "EAL/typed-input/1"
        ));
        let records = collect(&program, &bundle.context, &mode);
        let assessment =
            evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
        assert_eq!(assessment["valid"], true, "{}: {assessment}", mode.kind);
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["status"],
            "supported",
            "{}: {assessment}",
            mode.kind
        );
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["prose_verified"],
            false
        );
        let result = computation(&assessment, &mode);
        assert_eq!(result["status"], "supported", "{}: {result}", mode.kind);
        assert_eq!(
            result["binding"]["status"], "supported",
            "{}: {result}",
            mode.kind
        );
        assert_eq!(result["binding"]["prose_verified"], false);
        match mode.kind {
            "deductive" => {
                assert_eq!(result["details"]["entailed"], true);
                assert_eq!(result["details"]["consistent_premises"], true);
                assert_eq!(result["details"]["valuations"], 4);
            }
            "inductive" => {
                assert_eq!(result["details"]["sample_size"], 100);
                let lower = result["details"]["lower"].as_f64().unwrap();
                assert!((lower - 0.8256343385).abs() < 1e-8, "{result}");
                assert_eq!(result["details"]["assumptions_verified"], false);
            }
            "abductive" => {
                assert_eq!(result["details"]["best"], "A");
                assert!(
                    (result["details"]["posterior"]["A"].as_f64().unwrap() - 0.9).abs() < 1e-12
                );
                assert_eq!(result["details"]["assumptions_verified"], false);
            }
            "causal" => {
                assert_eq!(result["details"]["estimate"].as_f64(), Some(6.0));
                assert!(
                    (result["details"]["standard_error"].as_f64().unwrap() - 2_f64.sqrt()).abs()
                        < 1e-12
                );
                assert_eq!(result["details"]["assumptions_verified"], false);
            }
            "counterfactual" => {
                assert_eq!(result["details"]["factual"].as_f64(), Some(3.0));
                assert_eq!(result["details"]["counterfactual"].as_f64(), Some(9.0));
                assert_eq!(result["details"]["difference"].as_f64(), Some(6.0));
                assert_eq!(result["details"]["assumptions_verified"], false);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn each_mode_requires_its_acquired_input_even_when_all_reviews_are_accepted() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        let bundle = export(&request, &case).unwrap();
        let program = compile(&bundle);
        let id = typed_input_id(&program, &mode);
        let mut records = collect(&program, &bundle.context, &mode);
        records.as_object_mut().unwrap().remove(id);
        let assessment =
            evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
        assert_eq!(assessment["valid"], true, "{}: {assessment}", mode.kind);
        assert_eq!(assessment["evidence"][id]["status"], "unavailable");
        assert!(
            assessment["evidence"][id]["availability_issues"]
                .as_array()
                .unwrap()
                .contains(&json!("missing_observation"))
        );
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["status"],
            "unsupported",
            "{}: {assessment}",
            mode.kind
        );
        assert_eq!(computation(&assessment, &mode)["status"], "not_run");
    }
}

#[test]
fn wrong_scope_method_interval_and_query_cannot_supply_a_typed_input() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        let bundle = export(&request, &case).unwrap();
        let program = compile(&bundle);
        let id = typed_input_id(&program, &mode);
        for (field, wrong) in [
            ("scope", json!("another-fixture")),
            ("subject", json!("another-subject")),
            ("method", json!("structured/1")),
            ("valid_until", json!("2026-10-25T01:00:00Z")),
        ] {
            let mut records = collect(&program, &bundle.context, &mode);
            let mut value = envelope(&mode);
            value[field] = wrong;
            records[id] = record(&program, id, &bundle.context, value);
            let assessment =
                evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
            assert_eq!(assessment["valid"], true);
            assert_eq!(
                assessment["claims"][claim_name(&bundle)]["status"],
                "unsupported",
                "{} {field}: {assessment}",
                mode.kind
            );
            let result = computation(&assessment, &mode);
            assert_eq!(
                result["binding"]["status"], "unsupported",
                "{} {field}: {result}",
                mode.kind
            );
            assert!(
                result["binding"]["reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reason| reason.as_str().unwrap().contains(
                        if field.starts_with("valid_") {
                            "interval"
                        } else {
                            field
                        }
                    )),
                "{result}"
            );
        }
        let mut records = collect(&program, &bundle.context, &mode);
        let mut value = envelope(&mode);
        let (query_field, wrong) = match mode.kind {
            "deductive" => ("conclusion", json!("another_conclusion")),
            "inductive" => ("confidence", json!(0.9)),
            "abductive" => ("observed", json!("A different alarm observation.")),
            "causal" => ("assignment", json!("observational")),
            "counterfactual" => ("outcome", json!("X")),
            _ => unreachable!(),
        };
        value["payload"][query_field] = wrong;
        records[id] = record(&program, id, &bundle.context, value);
        let assessment =
            evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
        assert_eq!(assessment["valid"], true);
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["status"],
            "unsupported"
        );
        let result = computation(&assessment, &mode);
        assert_eq!(result["binding"]["status"], "unsupported", "{result}");
        assert!(
            result["binding"]["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason.as_str().unwrap().contains("query field")),
            "{result}"
        );
    }
}

#[test]
fn inconsistent_deductive_premises_do_not_exploit_vacuous_entailment() {
    let mut mode = cases().remove(0);
    mode.method["premises"] = json!(["p",{"not":"p"}]);
    mode.payload["premises"] = json!(["p",{"not":"p"}]);
    let (request, case) = fixture(&mode);
    let bundle = export(&request, &case).unwrap();
    let program = compile(&bundle);
    let records = collect(&program, &bundle.context, &mode);
    let assessment =
        evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
    let result = computation(&assessment, &mode);
    assert_eq!(assessment["valid"], true);
    assert_eq!(result["details"]["entailed"], true);
    assert_eq!(result["details"]["consistent_premises"], false);
    assert_eq!(result["status"], "unsupported");
    assert_eq!(
        assessment["claims"][claim_name(&bundle)]["status"],
        "unsupported"
    );
}

#[test]
fn missing_applicability_evidence_blocks_each_method_instead_of_certifying_assumptions() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        let bundle = export(&request, &case).unwrap();
        let program = compile(&bundle);
        let assumption = bundle.source_map["A1"]
            .declarations
            .iter()
            .find(|declaration| declaration.role == "authored_assumption")
            .unwrap();
        let validation = &program.assumptions[&assumption.name].validation;
        let mut records = collect(&program, &bundle.context, &mode);
        records.as_object_mut().unwrap().remove(validation);
        let assessment =
            evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
        assert_eq!(assessment["valid"], true);
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["status"],
            "unsupported",
            "{}: {assessment}",
            mode.kind
        );
        assert_eq!(computation(&assessment, &mode)["status"], "not_run");
    }
}

#[test]
fn completed_computations_can_fail_the_claim_predicate_without_becoming_tool_errors() {
    for mut mode in cases() {
        match mode.kind {
            "deductive" => {
                mode.method["premises"] = json!(["p"]);
                mode.payload["premises"] = json!(["p"]);
            }
            "inductive" => mode.payload["successes"] = json!(40),
            "abductive" => {
                for candidates in [
                    &mut mode.method["candidates"],
                    &mut mode.payload["candidates"],
                ] {
                    candidates[0]["likelihood"] = json!(0.5);
                    candidates[1]["likelihood"] = json!(0.5);
                }
            }
            "causal" => mode.payload["treatment"] = json!([3, 5]),
            "counterfactual" => {
                mode.method["intervention"]["value"] = json!(2.0);
                mode.payload["intervention"]["value"] = json!(2.0);
            }
            _ => unreachable!(),
        }
        let (request, case) = fixture(&mode);
        let bundle = export(&request, &case).unwrap();
        let program = compile(&bundle);
        let records = collect(&program, &bundle.context, &mode);
        let assessment =
            evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
        let result = computation(&assessment, &mode);
        assert_eq!(assessment["valid"], true, "{}: {assessment}", mode.kind);
        assert_eq!(
            result["status"], "supported",
            "The calculation itself completed: {result}"
        );
        assert_eq!(
            result["binding"]["status"], "unsupported",
            "The claim's predicate did not hold: {result}"
        );
        assert_eq!(result["binding"]["holds"], false);
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["status"],
            "unsupported",
            "{}: {assessment}",
            mode.kind
        );
        assert_eq!(
            assessment["evidence"][typed_input_id(&program, &mode)]["status"],
            "available"
        );
        match mode.kind {
            "deductive" => assert_eq!(
                result["details"]["counterexample"],
                json!({"p":true,"q":false})
            ),
            "inductive" => assert_eq!(result["details"]["estimate"].as_f64(), Some(0.4)),
            "abductive" => {
                assert_eq!(result["details"]["best"], Value::Null);
                assert_eq!(result["details"]["ties"], json!(["A", "B"]));
            }
            "causal" => assert_eq!(result["details"]["estimate"].as_f64(), Some(2.0)),
            "counterfactual" => assert_eq!(result["details"]["difference"].as_f64(), Some(2.0)),
            _ => unreachable!(),
        }
    }
}

#[test]
fn failed_acquisition_and_wrong_evidence_kind_are_unavailable_instead_of_negative_findings() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        let bundle = export(&request, &case).unwrap();
        let program = compile(&bundle);
        let id = typed_input_id(&program, &mode);
        for (field, value, issue) in [
            ("status", json!("error"), "tool_error"),
            (
                "evidence_kind",
                json!("unqualified_observation"),
                "invalid_observation",
            ),
        ] {
            let mut records = collect(&program, &bundle.context, &mode);
            records[id][field] = value;
            let assessment =
                evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
            assert_eq!(assessment["valid"], true);
            assert_eq!(assessment["evidence"][id]["status"], "unavailable");
            assert!(
                assessment["evidence"][id]["availability_issues"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(issue)),
                "{}: {assessment}",
                mode.kind
            );
            assert_eq!(computation(&assessment, &mode)["status"], "not_run");
            assert_eq!(
                assessment["claims"][claim_name(&bundle)]["status"],
                "unsupported"
            );
        }
    }
}

#[test]
fn unavailable_formal_models_remain_explicitly_unresolved_despite_generic_reviews() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        let mut value = serde_json::to_value(case).unwrap();
        value["warrants"][0]["reasoning_method"] = json!({
            "status":"unresolved","method":mode.kind,
            "reasons":["The necessary independently grounded formal model or evidence is unavailable."],
            "evidence_need_ids":["N5","N6"]
        });
        let case: ArgumentCase = serde_json::from_value(value).unwrap();
        validate_case(&request, &case).unwrap();
        let bundle = export(&request, &case).unwrap();
        assert!(
            !bundle
                .source
                .contains(&format!("method \"{}/1\"", mode.kind))
        );
        assert!(bundle.source.contains(
            "The necessary independently grounded formal model or evidence is unavailable."
        ));
        let program = compile(&bundle);
        let records = collect(&program, &bundle.context, &mode);
        let assessment =
            evaluation::evaluate_default(&program, &records, NOW, &bundle.context).unwrap();
        assert_eq!(assessment["valid"], true, "{}: {assessment}", mode.kind);
        assert_eq!(
            assessment["claims"][claim_name(&bundle)]["status"],
            "unsupported",
            "{}: {assessment}",
            mode.kind
        );
        assert!(
            assessment["evidence"]
                .as_object()
                .unwrap()
                .values()
                .any(|evidence| evidence["status"] == "unavailable")
        );
    }
}

#[test]
fn incompatible_methods_inputs_correspondence_and_design_roles_are_rejected_at_admission() {
    for mode in cases() {
        let (request, case) = fixture(&mode);
        for (pointer, wrong) in [
            ("/warrants/0/inference", json!("practical")),
            (
                "/warrants/0/reasoning_method/input_evidence_need_id",
                json!("missing-need"),
            ),
            (
                "/warrants/0/reasoning_method/formal_premises/0/premise_id",
                json!("G2"),
            ),
            (
                "/warrants/0/reasoning_method/formal_premises/0/input_pointer",
                json!("not-a-json-pointer"),
            ),
            (
                "/warrants/0/reasoning_method/formal_premises/0/evidence_need_id",
                json!("N5"),
            ),
            (
                "/warrants/0/reasoning_method/conclusion/claim_id",
                json!("stronger_target"),
            ),
            (
                "/warrants/0/reasoning_method/conclusion/unit",
                json!("not-a-unit"),
            ),
            (
                "/warrants/0/reasoning_method/evidence_requirements/1/role",
                json!("formal_input"),
            ),
        ] {
            let mut value = serde_json::to_value(&case).unwrap();
            *value.pointer_mut(pointer).unwrap() = wrong;
            let invalid: ArgumentCase = serde_json::from_value(value).unwrap();
            assert!(
                validate_case(&request, &invalid).is_err(),
                "{} accepted invalid {pointer}",
                mode.kind
            );
            assert!(
                export(&request, &invalid).is_err(),
                "{} exported invalid {pointer}",
                mode.kind
            );
        }
        // Retaining one mapping for G1 does not authorise extra formulae,
        // unobserved sample counts or unmatched model parameters.
        let mut incomplete = serde_json::to_value(&case).unwrap();
        incomplete["warrants"][0]["reasoning_method"]["formal_premises"]
            .as_array_mut()
            .unwrap()
            .pop();
        let invalid: ArgumentCase = serde_json::from_value(incomplete).unwrap();
        assert!(
            validate_case(&request, &invalid).is_err(),
            "{} admitted an unmapped formal input",
            mode.kind
        );
    }
}
