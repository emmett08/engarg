//! Independent seven-mode compiler acceptance through a real EAL installation.
use eal_collector_contracts::execution::EvaluationCriterion;
use engarg::{
    domain::{ArgumentCase, Request},
    export_policy::ExportPolicy,
    package,
    support_requirements::SupportElaboration,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use tempfile::TempDir;
#[path = "common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn fixture(index: usize, scenario: &str) -> (Request, ArgumentCase, SupportElaboration, Value) {
    let modes = reasoning::cases();
    let (mut request, mut case, mut elaboration, measurement) = if index == 0 || index >= 5 {
        let (request, case, elaboration) = executable_case::fixture();
        (
            request,
            case,
            elaboration,
            json!({"count":3,"distinct":true}),
        )
    } else {
        let (request, case, elaboration, _, _) = reasoning_graph::graph_fixture(&modes[index]);
        (request, case, elaboration, modes[index].payload.clone())
    };
    let mut value = json!({"measurement":measurement,"applies":true});
    if index == 5 {
        let payload = json!({"relevant_features":["interface","load"],"source":{"interface":"v1","load":3},"target":{"interface":"v1","load":3}});
        let mut authored = serde_json::to_value(&case).unwrap();
        authored["warrants"][0]["inference"] = json!("analogical");
        let bound = &mut authored["warrants"][0]["reasoning_method"];
        bound["method"] = json!({"identifier":"analogical/1","query":payload});
        bound["formal_premises"] = json!(["relevant_features","source","target"].into_iter().map(|field|json!({
            "premise_id":"G1","input_pointer":format!("/{field}"),"evidence_need_id":"N7",
            "correspondence":{"kind":"identity_value","source_input_index":0,"source_pointer":format!("/{field}")},
            "interpretation":"Exact features of the two retained fixture records only."})).collect::<Vec<_>>());
        bound["conclusion"]["quantity"] = json!("dimensionless");
        bound["conclusion"]["result"] =
            json!({"path":"match_fraction","operator":"==","expected":1});
        case = serde_json::from_value(authored).unwrap();
        value["measurement"] = payload.clone();
        let requirement = elaboration
            .requirements
            .iter_mut()
            .find(|r| r.target_id == "G1")
            .unwrap();
        requirement.acceptance = EvaluationCriterion::Equals { expected: payload };
        elaboration
            .evaluations
            .iter_mut()
            .find(|e| e.requirement_id == requirement.id)
            .unwrap()
            .criterion = requirement.acceptance.clone();
        request.claim = "The two retained fixtures match on both declared relevant features; transfer outside those features is unassessed.".into();
    }
    if index == 6 {
        case.warrants[0].inference = engarg::domain::InferenceKind::Practical;
        case.warrants[0].reasoning_method = None;
        let requirement = elaboration
            .requirements
            .iter_mut()
            .find(|r| r.target_id == "W1")
            .unwrap();
        requirement.contract = Some(reasoning_graph::contract(
            "judgement",
            "judgement",
            "review",
            "judgement_record",
        ));
        let scope = requirement.contract.as_ref().unwrap().scope.clone();
        requirement.acceptance = EvaluationCriterion::Judgement {
            criterion: "The bounded fixture action meets its declared decision criterion.".into(),
            scope: Some(scope.clone()),
        };
        let evaluation = elaboration
            .evaluations
            .iter_mut()
            .find(|e| e.requirement_id == requirement.id)
            .unwrap();
        evaluation.criterion = requirement.acceptance.clone();
        evaluation.required_coverage =
            eal_collector_contracts::execution::RequiredCoverage::Unspecified;
        evaluation.inputs =
            serde_json::from_value(json!([{"step_id":"review_source","pointer":"/review"}]))
                .unwrap();
        value["review"] = json!({"criterion":"The bounded fixture action meets its declared decision criterion.","target":"W1","scope":scope,"reviewer":"fixture-reviewer-v1","verdict":"supported","reasons":["The supplied fixture decision satisfies the specified criterion in this scope only."],"reviewed_at":reasoning::NOW});
        request.claim = "The fixture action satisfies the explicitly scoped retained decision review, conditional on its applicability observation.".into();
    }
    case.original_claim = request.claim.clone();
    case.current_claim.text = request.claim.clone();
    case.reading[0].paragraphs[0].text = request.claim.clone();
    // A single actual operation supplies several obligations without inventing
    // independent observations or runtime capability registrations.
    elaboration
        .acquisitions
        .retain(|a| a.id == "premise_source");
    let mut source_contract = serde_json::to_value(&elaboration.acquisitions[0].contract).unwrap();
    source_contract["quantities"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"name":"applies","unit":"boolean"}),
            json!({"name":"review","unit":"judgement_record"}),
        ]);
    elaboration.acquisitions[0].contract = serde_json::from_value(source_contract).unwrap();
    elaboration.acquisitions[0].arguments =
        json!({"fixture":"frozen-fixture-v1","scenario":scenario});
    for evaluation in &mut elaboration.evaluations {
        for reference in evaluation
            .inputs
            .iter_mut()
            .chain(&mut evaluation.applicability)
            .chain(&mut evaluation.challenges)
        {
            if reference.step_id == "applicability_source" {
                reference.step_id = "premise_source".into();
            }
        }
    }
    if index == 6 {
        let mut review_source = elaboration.acquisitions[0].clone();
        review_source.id = "review_source".into();
        review_source.contract = elaboration
            .requirements
            .iter()
            .find(|r| r.target_id == "W1")
            .unwrap()
            .contract
            .clone()
            .unwrap();
        elaboration.acquisitions.push(review_source);
    }
    engarg::domain::validate_case(&request, &case).unwrap();
    (request, case, elaboration, value)
}

fn execute(index: usize, scenario: &str) -> (Value, String) {
    let original = TempDir::new().unwrap();
    let (request, case, elaboration, value) = fixture(index, scenario);
    fs::create_dir(original.path().join("tools")).unwrap();
    let script = format!(
        r#"import json,sys
request=json.load(sys.stdin)
value=json.loads({payload})
scenario=request['input']['scenario']
if scenario=='error': raise RuntimeError('deliberate source failure')
if scenario=='negative': value['applies']=False
if scenario=='wrong-scope': value['review']['scope']['subject']='different-fixture'
if scenario=='rejected': value['review']['verdict']='rejected'
if scenario=='missing-review-time': value['review'].pop('reviewed_at')
if scenario=='missing-reviewer': value['review'].pop('reviewer')
if scenario=='old-nested-review': value['review']['reviewed_at']='2020-01-01T00:00:00Z'
if scenario=='malformed-review-reasons': value['review']['reasons']=[None]
if scenario=='blank-reviewer': value['review']['reviewer']='  '
print(json.dumps({{'value':value,'observed_at':'2020-01-01T00:00:00Z' if scenario=='stale' else '2026-10-25T12:00:00Z','context':request['context'],'request':{{k:request[k] for k in ['tool','tool_version','input','context']}}}}))
"#,
        payload = serde_json::to_string(&value.to_string()).unwrap()
    );
    fs::write(original.path().join("tools/source.py"), script).unwrap();
    let manifest = original.path().join("tools.toml");
    fs::write(&manifest,"[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n").unwrap();
    if index == 6 {
        let mut tools = fs::read_to_string(&manifest).unwrap();
        tools.push_str("[tools.review_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n");
        fs::write(&manifest, tools).unwrap();
    }
    let built = original.path().join("built");
    let receipt = package::build(
        &request,
        &case,
        &elaboration,
        &manifest,
        &built,
        &ExportPolicy::default(),
    )
    .unwrap();
    let moved = TempDir::new().unwrap();
    fs::create_dir(moved.path().join("tools")).unwrap();
    for file in ["case.eal", "tools.toml", "tools/source.py"] {
        fs::copy(built.join(file), moved.path().join(file)).unwrap();
    }
    let source = fs::read_to_string(moved.path().join("case.eal")).unwrap();
    assert!(!source.contains("formal_source"));
    assert!(!source.contains("engarg_support_graph"));
    drop(original);
    let claim = receipt["claims"]["current_claim"]
        .as_str()
        .unwrap()
        .to_owned();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let run = Command::new(host)
        .current_dir(moved.path())
        .args([
            "run",
            "case.eal",
            "--tools",
            "tools.toml",
            "--workspace",
            "run",
            "--claim",
            &claim,
            "--now",
            reasoning::NOW,
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    let response: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(response["is_error"], false, "{response}");
    (response["result"].clone(), claim)
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn seven_compiled_modes_execute_from_moved_source_tools_and_toml_only() {
    for index in 0..7 {
        let (result, claim) = execute(index, "positive");
        assert_eq!(
            result["assessment"]["claims"][claim]["status"], "supported",
            "mode {index}: {result}"
        );
        assert_eq!(
            result["collection"]["records"]
                .as_object()
                .unwrap()
                .values()
                .filter(|r| r["tool"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("source_")))
                .count(),
            if index == 6 { 2 } else { 1 }
        );
        if index < 6 {
            let method = if index == 5 {
                "analogical/1".to_owned()
            } else {
                format!("{}/1", reasoning::cases()[index].kind)
            };
            let computation = result["assessment"]["arguments"]
                .as_object()
                .unwrap()
                .values()
                .find(|a| a["reasoning_result"]["method"] == method)
                .unwrap();
            let details = &computation["reasoning_result"]["details"];
            match index {
                0 => assert_eq!(details["entailed"], true),
                1 => {
                    assert!((details["lower"].as_f64().unwrap() - 0.8256343384950865).abs() < 1e-8)
                }
                2 => assert!((details["best_posterior"].as_f64().unwrap() - 0.9).abs() < 1e-12),
                3 => assert_eq!(details["estimate"], 6.0),
                4 => assert_eq!(details["difference"], 6.0),
                5 => assert_eq!(details["match_fraction"], 1.0),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn failed_applicability_and_unusable_or_misscoped_observations_cannot_support() {
    for (index, scenario) in [
        (1, "negative"),
        (3, "stale"),
        (4, "error"),
        (6, "wrong-scope"),
        (6, "rejected"),
        (6, "missing-review-time"),
        (6, "missing-reviewer"),
        (6, "old-nested-review"),
        (6, "malformed-review-reasons"),
        (6, "blank-reviewer"),
    ] {
        let (result, claim) = execute(index, scenario);
        assert_ne!(
            result["assessment"]["claims"][claim]["status"], "supported",
            "{index}/{scenario}: {result}"
        );
    }
}
