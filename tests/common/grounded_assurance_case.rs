//! Independently authored observed-identifier and closed-theorem cases.
//!
//! The observed case executes an operator-owned snapshot reader. Device names
//! identify synthetic fixtures; neither case establishes physical supply health.
use super::{executable_case, reasoning_graph};
use eal_collector_contracts::execution::EvaluationCriterion;
use engarg::{
    direct_planning::DirectPlan,
    domain::{ArgumentCase, ReasoningMethodBinding, Request},
    engine::{Mode, RunReport},
    optimal_argument::assurance::{self, AssurancePlan},
    support_requirements::{SupportElaboration, SupportedArgument},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub const DEVICE: &str = "identifier-device-17";
pub const PROTOCOL: &str = "identifier-snapshot/1";
pub const START: &str = "2026-01-01T00:00:00Z";
pub const END: &str = "2100-01-01T00:00:00Z";

pub struct Fixture {
    pub original: RunReport,
    pub proposal: SupportedArgument,
    pub plan: AssurancePlan,
}

pub fn report(request: Request, argument: ArgumentCase, support: SupportElaboration) -> RunReport {
    RunReport {
        schema_version: "engarg.run.v3".into(),
        execution_mode: "offline_synthetic_fixture".into(),
        input_sha256: assurance::digest(&request).unwrap(),
        provider_config: None,
        mode: Mode::Single,
        run_config: None,
        request,
        argument,
        transcript: vec![],
        tooling: None,
        direct_bindings: Some(DirectPlan::unbound(&support)),
        support_elaboration: Some(support),
        design_review: None,
        argument_revisions: vec![],
        target_runtime: None,
        authoring_recovery: None,
        evidence_planning: None,
        replanning: None,
        support_recovery: None,
        limitations: vec!["Independently authored synthetic identifier snapshot or mathematical theorem; no physical deployment claim.".into()],
    }
}

fn policy(checks: Vec<Value>, verification: Vec<Value>) -> AssurancePlan {
    serde_json::from_value(json!({
        "schema":"engarg.assurance-plan/2", "checks":checks, "verification":verification
    }))
    .unwrap()
}

fn predicate(path: &str, expected: Value) -> Value {
    json!({"path":path,"operator":"==","expected":expected,"expression":null})
}

fn count_predicate() -> Value {
    json!({"path":"","operator":"expression","expected":null,"expression":{
        "kind":"binary","value":"==","arguments":[
            {"kind":"call","value":"count","arguments":[{"kind":"field","value":"identifiers"}]},
            {"kind":"literal","value":3}
        ]
    }})
}

fn distinct_predicate() -> Value {
    json!({"path":"","operator":"expression","expected":null,
        "expression":{"kind":"call","value":"distinct","arguments":[{"kind":"field","value":"identifiers"}]}})
}

fn applicability() -> Value {
    json!({
        "observation_id":"premise_source",
        "identity_pointer":"/identity","identity":DEVICE,
        "unit_pointer":"/unit","unit":"1",
        "scope_pointer":"/scope","scope":{"fixture":"frozen-fixture-v1"},
        "valid_from_pointer":"/valid_from","valid_until_pointer":"/valid_until",
        "valid_from":START,"valid_until":END,"max_age_seconds":3600
    })
}

fn contract(target: &str, requirement: &str, role: &str, condition: Value) -> Value {
    let atoms = if target == "W1" {
        json!([
            {"symbol":"count_matches","observation_id":"premise_source",
             "predicate":count_predicate()},
            {"symbol":"identifiers_distinct","observation_id":"premise_source",
             "predicate":distinct_predicate()}
        ])
    } else {
        json!([])
    };
    json!({
        "schema":"eal.verification-contract/1","target_id":target,"mode":"empirical",
        "atoms":atoms,
        "obligations":[{"id":requirement,"role":role,"observation_id":"premise_source",
            "predicate":condition}],
        "applicability":[applicability()]
    })
}

/// A real subprocess reads a retained identifier array and computes its count
/// and distinctness. `fault` changes observed source data, never frozen policy.
pub fn empirical(root: &Path, fault: &str) -> Fixture {
    let (mut request, mut argument, mut support) = executable_case::fixture();
    let claim =
        "The identified synthetic device snapshot contains exactly three distinct identifiers.";
    request.claim = claim.into();
    request.context = "A bounded synthetic identifier-snapshot protocol. No physical supply, sampling population or deployment is represented.".into();
    argument.original_claim = claim.into();
    argument.current_claim.text = claim.into();
    argument.current_claim.qualifier = "Only device identifier-device-17, fixture frozen-fixture-v1, dimensionless snapshot quantities and the declared validity interval.".into();
    argument.scope.system_boundary = DEVICE.into();
    argument.scope.configuration = format!("{DEVICE}; {PROTOCOL}");
    argument.scope.operating_conditions =
        "The actual persisted identifier array returned by the operator-owned snapshot reader."
            .into();
    argument.scope.time_horizon = format!("The interval [{START}, {END}).");
    argument.grounds[0].statement = "The actual identifier array has length three and has no repeated identifier. count_matches denotes count(identifiers)==3 and identifiers_distinct denotes distinct(identifiers); the returned summary flags do not define either atom.".into();
    argument.warrants[0].rule = "Conjoin the two source-grounded predicates. A conjunction is true precisely when both named measured predicates are true.".into();
    argument.warrants[0].applicability = "The observed device, unit, scope and validity interval must equal the frozen snapshot contract.".into();
    argument.assumptions[0].statement = "The observed record belongs to identifier-device-17, has unit 1 and scope frozen-fixture-v1, and its original observation time is current within the declared interval.".into();
    argument.assumptions[0].validation = "Compare actual source metadata with the frozen applicability fields; a bare accepted or applies flag supplies no substitute.".into();
    let Some(ReasoningMethodBinding::Bound(bound)) = &mut argument.warrants[0].reasoning_method
    else {
        unreachable!()
    };
    bound.method.query = json!({"premises":["count_matches","identifiers_distinct"],
        "conclusion":{"and":["count_matches","identifiers_distinct"]}});
    bound.formal_premises[0].interpretation =
        "count_matches means the native count of the actual source record's identifiers array equals integer 3, independently of the returned measurement summary.".into();
    bound.formal_premises[1].interpretation = "identifiers_distinct means the native distinct predicate over the actual source record's identifiers array is true, independently of the returned measurement summary.".into();
    bound.conclusion.valid_from = START.into();
    bound.conclusion.valid_until = END.into();
    bound.conclusion.subject = DEVICE.into();
    let method = bound.clone();
    for section in &mut argument.reading {
        for paragraph in &mut section.paragraphs {
            paragraph.text = claim.into();
        }
    }
    argument.adjudication = "The authored labels describe planning. Runtime observations, independent native replay and source-to-formula verification decide whether this exact current snapshot claim is supported.".into();

    for need in &mut argument.evidence_plan {
        need.method = "Read the operator-owned persisted snapshot and compare its actual fields with the frozen contract.".into();
        need.measurement = "Identifier count, distinctness, device identity, protocol identity, scope, unit and original observation time.".into();
        need.comparator =
            "The independently fixed snapshot and source-to-formula verification contract.".into();
        need.acceptance_criterion = match need.target_id.as_str() {
            "G1" => "measurement equals {count:3, distinct:true}.",
            "A1" => "identity equals identifier-device-17; native applicability checks unit, scope, interval and original clock.",
            "W1" => "protocol equals identifier-snapshot/1 and the native conjunction inference is valid.",
            _ => "The independently fixed snapshot contract is met.",
        }.into();
    }
    support.acquisitions.retain(|a| a.id == "premise_source");
    let source = &mut support.acquisitions[0];
    source.arguments = json!({"fixture":"frozen-fixture-v1","operation":"premise_source"});
    source.contract.scope.subject = DEVICE.into();
    source.contract.quantities = serde_json::from_value(json!([
        {"name":"measurement","unit":"1"},{"name":"identity","unit":"1"},
        {"name":"protocol","unit":"1"},{"name":"scope","unit":"1"},
        {"name":"identifiers","unit":"1"}
    ]))
    .unwrap();
    for requirement in &mut support.requirements {
        let (quantity, pointer, expected) = match requirement.target_id.as_str() {
            "G1" => (
                "measurement",
                "/measurement",
                json!({"count":3,"distinct":true}),
            ),
            "A1" => ("identity", "/identity", json!(DEVICE)),
            "W1" => ("protocol", "/protocol", json!(PROTOCOL)),
            "current_claim" => ("scope", "/scope", json!({"fixture":"frozen-fixture-v1"})),
            _ => continue,
        };
        requirement.contract = Some(reasoning_graph::contract(
            "observation",
            "existential_witness",
            quantity,
            "1",
        ));
        requirement.contract.as_mut().unwrap().scope.subject = DEVICE.into();
        requirement.acceptance = EvaluationCriterion::Equals { expected };
        requirement.acceptance_rationale = "Evaluate the actual selected snapshot field; grounding and applicability have separate executable verification records.".into();
        let mut needs: Vec<_> = argument
            .evidence_plan
            .iter()
            .filter(|n| n.target_id == requirement.target_id)
            .collect();
        needs.sort_by_key(|n| &n.id);
        requirement.original_acceptance_criteria = needs
            .iter()
            .map(|n| n.acceptance_criterion.clone())
            .collect();
        let evaluation = support
            .evaluations
            .iter_mut()
            .find(|e| e.requirement_id == requirement.id)
            .unwrap();
        evaluation.inputs = vec![
            serde_json::from_value(json!({"step_id":"premise_source","pointer":pointer})).unwrap(),
        ];
        evaluation.criterion = requirement.acceptance.clone();
    }
    let mut checks: Vec<_> = support.requirements.iter()
        .filter(|r| matches!(r.target_id.as_str(),"G1"|"A1"|"W1"|"current_claim"))
        .map(|r|json!({"id":format!("source_{}",r.id),
            "role":if r.target_id=="A1"{"applicability"}else{"premise"},
            "target_id":r.target_id,"procedure":{"kind":"requirement","requirement_id":r.id},"discharges":[]}))
        .collect();
    checks.push(json!({"id":"intended_inference","role":"formalisation","target_id":"current_claim",
        "procedure":{"kind":"method","conclusion_id":"current_claim","identifier":"deductive/1",
            "reference_query":method.method.query,"result":method.conclusion.result},"discharges":[]}));
    checks.push(json!({"id":"intended_source_mapping","role":"formalisation","target_id":"current_claim",
        "procedure":{"kind":"value","pointer":"/argument/warrants/0/reasoning_method/formal_premises",
            "expected":method.formal_premises},"discharges":[]}));
    let mut verification = vec![
        contract("G1", "requirement_G1", "premise", count_predicate()),
        contract(
            "A1",
            "requirement_A1",
            "applicability",
            predicate("identity", json!(DEVICE)),
        ),
        contract(
            "W1",
            "requirement_W1",
            "warrant",
            predicate("protocol", json!(PROTOCOL)),
        ),
        contract(
            "current_claim",
            "composition_condition",
            "premise",
            predicate("scope.fixture", json!("frozen-fixture-v1")),
        ),
    ];
    verification[0]["obligations"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"measured_distinct_G1","role":"premise","observation_id":"premise_source",
            "predicate":distinct_predicate()
        }));
    let original = report(request, argument, support);
    let proposal = SupportedArgument {
        argument: original.argument.clone(),
        support_elaboration: original.support_elaboration.clone().unwrap(),
    };
    let mut plan = policy(checks, verification);
    if fault == "missing_symbol" {
        // Mutate policy alone: the measured record and wrapper remain positive.
        for contract in &mut plan.verification {
            if contract.target_id == "W1" {
                contract
                    .atoms
                    .retain(|a| a.symbol != "identifiers_distinct");
            }
        }
    }
    write_sources(root, fault);
    write_fixture(root, &original, &plan);
    Fixture {
        original,
        proposal,
        plan,
    }
}

/// A derived conclusion gets its support from checked warrants and their
/// conditions; it requires no duplicate observation of the conclusion itself.
pub fn empirical_composed(root: &Path, fault: &str) -> Fixture {
    let mut fixture = empirical(root, fault);
    let support = fixture.original.support_elaboration.as_mut().unwrap();
    support
        .requirements
        .retain(|r| r.id != "composition_condition");
    support
        .evaluations
        .retain(|e| e.requirement_id != "composition_condition");
    fixture.original.direct_bindings = Some(DirectPlan::unbound(support));
    fixture.proposal.support_elaboration = support.clone();
    fixture
        .plan
        .checks
        .retain(|c| c.id != "source_composition_condition");
    fixture
        .plan
        .verification
        .retain(|c| c.target_id != "current_claim");
    write_fixture(root, &fixture.original, &fixture.plan);
    fixture
}

pub fn theorem(root: &Path) -> Fixture {
    let formula = json!({"implies":[{"and":["p",{"implies":["p","q"]}]},"q"]});
    let query = json!({"premises":[],"conclusion":formula});
    let claim = "For every classical Boolean valuation, ((p and (p implies q)) implies q) is true.";
    let request = Request { claim:claim.into(), context:"A closed mathematical theorem over uninterpreted Boolean atoms; no external device or empirical premises.".into(), evidence:vec![] };
    let argument: ArgumentCase = serde_json::from_value(json!({
        "original_claim":claim,"current_claim":{"text":claim,"qualifier":"Classical propositional semantics only."},
        "stronger_target":{"text":"A physical device satisfies this logical interpretation.","qualifier":"No empirical correspondence is asserted."},
        "status":"conditional",
        "scope":{"system_boundary":"Closed Boolean formula","configuration":"Uninterpreted atoms p and q.","operating_conditions":"Every classical truth valuation.","units":"1","time_horizon":"Immutable mathematical programme data.","consequences":"A mathematical consequence with no physical deployment inference."},
        "subclaims":[],"grounds":[],"assumptions":[],"defeaters":[],"alternatives":[],"evidence_plan":[],
        "warrants":[{"id":"W1","rule":"Exhaustively check the closed Boolean theorem under classical truth semantics.","conclusion_id":"current_claim","premise_ids":[],"backing_evidence_ids":[],"inference":"deductive","applicability":"Only the explicitly declared mathematical model.",
            "reasoning_method":{"status":"bound","claim_mode":"mathematical_consequence","method":{"identifier":"deductive/1","query":query},"formal_premises":[],"evidence_requirements":[],"assumption_ids":[],
                "conclusion":{"claim_id":"current_claim","subject":"modus-ponens-formula","quantity":"proposition","unit":"1","scope":{"kind":"mathematical_model","selectors":[{"name":"model_id","value":"classical-modus-ponens-v1"}],"identity_dimensions":["model_id"]},"valid_from":START,"valid_until":END,"result":{"path":"entailed","operator":"==","expected":true}}}}],
        "compositions":[{"conclusion_id":"current_claim","relationship":"jointly_required","selected_warrant_ids":["W1"],"rationale":"One closed native theorem, without empirical support obligations."}],
        "reading":[{"claim_id":"current_claim","title":"A mathematical theorem","paragraphs":[{"purpose":"answer","text":claim,"reference_ids":["current_claim","W1"]}]}],
        "residual_risks":["The theorem supplies no empirical source correspondence."],
        "decision_limits":["Use only as a consequence of the stated classical model."],
        "adjudication":"Verify the immutable closed formula natively. There are no observations to collect or source clocks to refresh."
    })).unwrap();
    let support: SupportElaboration = serde_json::from_value(json!({
        "schema":engarg::support_requirements::SUPPORT_ELABORATION_SCHEMA,"requirements":[],"acquisitions":[],
        "derivations":[],"evaluations":[],"revisions":[]
    })).unwrap();
    let plan = policy(
        vec![
            json!({"id":"closed_theorem","role":"formalisation","target_id":"current_claim",
        "procedure":{"kind":"method","conclusion_id":"current_claim","identifier":"deductive/1",
            "reference_query":query,"result":{"path":"entailed","operator":"==","expected":true}},"discharges":[]}),
        ],
        ["current_claim", "W1"]
            .into_iter()
            .map(|target| {
                json!({
                    "schema":"eal.verification-contract/1","target_id":target,"mode":"mathematical",
                    "atoms":[],"obligations":[],"applicability":[]
                })
            })
            .collect(),
    );
    let original = report(request, argument, support);
    let proposal = SupportedArgument {
        argument: original.argument.clone(),
        support_elaboration: original.support_elaboration.clone().unwrap(),
    };
    fs::write(root.join("tools.toml"), "[tools]\n").unwrap();
    write_fixture(root, &original, &plan);
    Fixture {
        original,
        proposal,
        plan,
    }
}

fn write_fixture(root: &Path, original: &RunReport, plan: &AssurancePlan) {
    fs::write(
        root.join("input.json"),
        serde_json::to_vec_pretty(original).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("verification.json"),
        serde_json::to_vec_pretty(plan).unwrap(),
    )
    .unwrap();
}

fn write_sources(root: &Path, fault: &str) {
    fs::create_dir(root.join("tools")).unwrap();
    let mut snapshot = json!({"identifiers":["alpha","beta","gamma"],"identity":DEVICE,
        "protocol":PROTOCOL,"unit":"1","scope":{"fixture":"frozen-fixture-v1"},
        "valid_from":START,"valid_until":END,"accepted":true});
    match fault {
        "wrong_device" => snapshot["identity"] = json!("another-device"),
        "wrong_unit" => snapshot["unit"] = json!("V"),
        "wrong_scope" => snapshot["scope"] = json!({"fixture":"another-fixture"}),
        "wrong_interval" => snapshot["valid_until"] = json!("2099-01-01T00:00:00Z"),
        "stale" => snapshot["observed_at"] = json!("2026-01-01T00:00:00Z"),
        "future" => snapshot["observed_at"] = json!("2100-01-01T00:00:00Z"),
        "deceptive_accepted" => {
            snapshot["identifiers"] = json!(["alpha", "alpha", "gamma"]);
            snapshot["deceptive_summary"] = json!(true);
        }
        "missing_measurement" => snapshot["omit_measurement"] = json!(true),
        "positive" | "missing_symbol" => {}
        other => panic!("Unknown independent fixture fault {other}"),
    }
    fs::write(
        root.join("tools/snapshot.json"),
        serde_json::to_vec_pretty(&snapshot).unwrap(),
    )
    .unwrap();
    fs::write(root.join("tools/read_snapshot.py"),r#"import datetime,json,sys
request=json.load(sys.stdin)
assert request['input']=={'fixture':'frozen-fixture-v1','operation':'premise_source'}
with open(sys.argv[1],encoding='utf-8') as source:
    value=json.load(source)
identifiers=value['identifiers']
value['measurement']={'count':len(identifiers),'distinct':len(set(identifiers))==len(identifiers)}
if value.pop('deceptive_summary',False):
    value['measurement']={'count':3,'distinct':True}
if value.pop('omit_measurement',False):
    del value['measurement']
observed=value.pop('observed_at',None) or datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z')
print(json.dumps({'value':value,'observed_at':observed}))
"#).unwrap();
    fs::write(root.join("tools.toml"),"[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/read_snapshot.py','tools/snapshot.json']\ninherit_env=['PATH']\n").unwrap();
}
