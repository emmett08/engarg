//! Coherent, deterministic test-bench claims for the five native method profiles.

use engarg::domain::{ArgumentCase, Request, validate_case};
use serde_json::{Value, json};

pub const NOW: &str = "2026-10-25T12:00:00Z";
const START: &str = "2026-10-25T00:00:00Z";
const END: &str = "2026-10-26T00:00:00Z";
const SUBJECT: &str = "mode-acceptance-test-bench";
const SCOPE: &str = "frozen-fixture-v1";

#[derive(Clone)]
pub struct ModeCase {
    pub kind: &'static str,
    pub evidence_kind: &'static str,
    pub method: Value,
    pub payload: Value,
    pub result: Value,
    pub quantity: &'static str,
    pub unit: &'static str,
    pub design_role: &'static str,
}

pub fn cases() -> Vec<ModeCase> {
    vec![
        ModeCase {
            kind: "deductive",
            evidence_kind: "logical_case",
            method: json!({"kind":"deductive","premises":["p",{"implies":["p","q"]}],"conclusion":"q"}),
            payload: json!({"premises":["p",{"implies":["p","q"]}],"conclusion":"q"}),
            result: json!({"path":"entailed","operator":"==","expected":true}),
            quantity: "proposition",
            unit: "1",
            design_role: "applicability",
        },
        ModeCase {
            kind: "inductive",
            evidence_kind: "sample",
            method: json!({"kind":"inductive","confidence":0.95}),
            payload: json!({"successes":90,"trials":100,"confidence":0.95}),
            result: json!({"path":"lower","operator":">=","expected":0.8}),
            quantity: "probability",
            unit: "1",
            design_role: "sampling_design",
        },
        ModeCase {
            kind: "abductive",
            evidence_kind: "hypotheses",
            method: json!({"kind":"abductive","observed":"The test bench emitted the alarm.","candidates":[{"name":"A","prior":0.5,"likelihood":0.9},{"name":"B","prior":0.5,"likelihood":0.1}]}),
            payload: json!({"observed":"The test bench emitted the alarm.","candidates":[{"name":"A","prior":0.5,"likelihood":0.9},{"name":"B","prior":0.5,"likelihood":0.1}]}),
            result: json!({"path":"posterior.A","operator":">=","expected":0.8}),
            quantity: "probability",
            unit: "1",
            design_role: "hypothesis_model",
        },
        ModeCase {
            kind: "causal",
            evidence_kind: "experiment",
            method: json!({"kind":"causal","assignment":"randomised"}),
            payload: json!({"assignment":"randomised","treatment":[7,9],"control":[1,3]}),
            result: json!({"path":"estimate","operator":">=","expected":5}),
            quantity: "time",
            unit: "s",
            design_role: "intervention_design",
        },
        ModeCase {
            kind: "counterfactual",
            evidence_kind: "causal_model",
            method: json!({"kind":"counterfactual","variables":[{"name":"X","intercept":1.0,"coefficients":[],"noise":0.0},{"name":"Y","intercept":0.0,"coefficients":[{"variable":"X","coefficient":2.0}],"noise":1.0}],"intervention":{"variable":"X","value":4.0},"outcome":"Y"}),
            payload: json!({"variables":{"X":{"intercept":1.0,"coefficients":{},"noise":0.0},"Y":{"intercept":0.0,"coefficients":{"X":2.0},"noise":1.0}},"intervention":{"variable":"X","value":4.0},"outcome":"Y"}),
            result: json!({"path":"difference","operator":"==","expected":6}),
            quantity: "time",
            unit: "s",
            design_role: "structural_model",
        },
    ]
}

fn binding(mode: &ModeCase) -> Value {
    let pointers = match mode.kind {
        "deductive" => mode.payload["premises"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, _)| format!("/premises/{index}"))
            .collect::<Vec<_>>(),
        "inductive" => vec!["/successes".into(), "/trials".into()],
        "abductive" => vec!["/observed".into(), "/candidates".into()],
        "causal" => vec!["/assignment".into(), "/treatment".into(), "/control".into()],
        "counterfactual" => vec![
            "/variables".into(),
            "/intervention".into(),
            "/outcome".into(),
        ],
        _ => unreachable!(),
    };
    let premises: Vec<_> = pointers.into_iter().map(|pointer| {
        let correspondence = if mode.kind == "deductive" {
            json!({"kind":"accepted_premise"})
        } else {
            json!({"kind":"identity_value","source_input_index":0,"source_pointer":pointer})
        };
        json!({
            "premise_id":"G1","input_pointer":pointer,
            "evidence_need_id":"N7","correspondence":correspondence,
            "interpretation":"The independently identified source observation supplies this exact formal input; correspondence with prose is separately reviewed."
        })
    }).collect();
    json!({
        "status":"bound",
        "method":mode.method,
        "input_evidence_need_id":"N5",
        "formal_premises":premises,
        "evidence_requirements":[
            {"evidence_need_id":"N5","role":"formal_input","requirement":"Acquire the exact typed method input from the frozen test bench."},
            {"evidence_need_id":"N7","role":"premise_observation","requirement":"Acquire the independently identified source observation represented by G1, without treating computation input as proof of empirical truth."},
            {"evidence_need_id":"N6","role":mode.design_role,"requirement":"Acquire the identified design or model and check its applicability to the same test bench."}
        ],
        "assumption_ids":["A1"],
        "conclusion":{
            "claim_id":"current_claim","subject":SUBJECT,"quantity":mode.quantity,"unit":mode.unit,
            "scope":SCOPE,"valid_from":START,"valid_until":END,"result":mode.result
        }
    })
}

pub fn fixture(mode: &ModeCase) -> (Request, ArgumentCase) {
    let claim = match mode.kind {
        "deductive" => {
            "Within the supplied satisfiable propositional model, the declared premises entail q."
        }
        "inductive" => {
            "For the frozen binomial test-bench sample, the 95% Wilson lower confidence bound is at least 0.8."
        }
        "abductive" => {
            "Within the supplied two-hypothesis model and alarm observation, the posterior probability assigned to A is at least 0.8."
        }
        "causal" => {
            "For the declared randomised test-bench experiment, the treatment-minus-control mean duration contrast is at least five seconds."
        }
        "counterfactual" => {
            "Under the supplied affine model with unchanged exogenous noise, setting X to four increases Y by six seconds."
        }
        _ => unreachable!(),
    };
    let request: Request = serde_json::from_value(json!({"claim":claim})).unwrap();
    let needs: Vec<_> = [
        ("N5", "W1", "Acquire the method-specific formal input for W1."),
        ("N6", "A1", "Check the declared sampling, intervention or formal-model applicability contract."),
        ("N7", "G1", "Acquire the source observation represented by the formal premise, independently of the computation input."),
    ].into_iter().map(|(id,target,question)| json!({
            "id":id,"target_id":target,"question":question,
            "method":"Read the frozen deterministic test-bench fixture, retaining its source and scope.",
            "measurement":"The declared method input and independently identified applicability record.",
            "comparator":"The same frozen fixture and the preregistered formal query.",
            "acceptance_criterion":"Exact subject, scope, interval, method and query correspondence, with the separate applicability check satisfied.",
            "if_failed":"Retain the argument as unresolved without a computational certificate.",
            "enables_claim":"current_claim","priority":1,"cost":"One bounded fixture read."
        })).collect();
    let case: ArgumentCase = serde_json::from_value(json!({
        "original_claim":claim,
        "current_claim":{"text":claim,"qualifier":"A bounded test-bench conclusion conditional on the identified observations and declared model."},
        "stronger_target":{"text":"The conclusion also applies to an unmeasured physical deployment.","qualifier":"Outside this fixture's evidence and computation scope."},
        "status":"conditional",
        "scope":{"system_boundary":SUBJECT,"configuration":"Frozen deterministic test bench v1.","operating_conditions":"The supplied formal model and separately identified applicability record only.","units":mode.unit,"time_horizon":"The identified half-open fixture interval.","consequences":"Exercise bounded method dispatch without making a production claim."},
        "subclaims":[],
        "grounds":[{"id":"G1","statement":"The identified test bench supplies the observations represented by the formal input.","evidence_ids":[],"status":"missing"}],
        "warrants":[{"id":"W1","rule":"Apply the declared bounded method to the retained formal input after checking source correspondence and applicability, then test the declared result predicate.","conclusion_id":"current_claim","premise_ids":["G1"],"backing_evidence_ids":[],"inference":mode.kind,"applicability":"Only the frozen formal model, test-bench observations and stated interval.","reasoning_method":binding(mode)}],
        "compositions":[{"conclusion_id":"current_claim","relationship":"jointly_required","selected_warrant_ids":["W1"],"rationale":"The conclusion requires this explicitly typed computation and its identified source and applicability conditions."}],
        "assumptions":[{"id":"A1","statement":"The retained source observation and declared sampling, intervention or formal-model design apply to this exact fixture.","critical":true,"status":"untested","evidence_ids":[],"applies_to":["W1"],"validation":"Check the separately retained design or model against the same subject, scope, interval and formal query."}],
        "defeaters":[],"alternatives":[],"evidence_plan":needs,
        "reading":[{"claim_id":"current_claim","title":"The bounded test-bench conclusion","paragraphs":[{"purpose":"answer","text":claim,"reference_ids":["current_claim","G1","W1","A1","N5","N6","N7"]}]}],
        "residual_risks":["These deterministic fixtures do not validate an empirical model or sampling process in the physical world."],
        "decision_limits":["Rely only on the identified test-bench computation. Prose correspondence and real-world model adequacy remain separate assessments."],
        "adjudication":"Acquire the distinct source and applicability observations, execute the declared native method, and retain an unresolved conclusion whenever a required input is unavailable."
    })).unwrap();
    validate_case(&request, &case).unwrap();
    (request, case)
}

pub fn envelope(mode: &ModeCase) -> Value {
    json!({
        "schema":"EAL/typed-input/1","method":format!("{}/1",mode.kind),
        "subject":SUBJECT,"quantity":mode.quantity,"unit":mode.unit,
        "scope":SCOPE,"valid_from":START,"valid_until":END,"payload":premise_source(mode)
    })
}

/// The independent source acquisition selected by the G1 evaluation. Identity
/// correspondences compare its fields with the separately acquired formal input.
pub fn premise_source(mode: &ModeCase) -> Value {
    mode.payload.clone()
}
