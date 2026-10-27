//! Independently stated finite claims, observations, designs and adverse worlds.
use super::{executable_case, reasoning, reasoning_graph};
use eal_collector_contracts::execution::{EvaluationCriterion, RequiredCoverage};
use engarg::{
    domain::{ArgumentCase, Request},
    support_requirements::{SupportElaboration, SupportRequirementRole},
};
use serde_json::{Value, json};

pub const MODES: [&str; 7] = [
    "deductive",
    "inductive",
    "abductive",
    "causal",
    "counterfactual",
    "analogical",
    "practical",
];

pub struct Fixture {
    pub request: Request,
    pub case: ArgumentCase,
    pub elaboration: SupportElaboration,
    pub observation: Value,
}

fn requirement(
    elaboration: &mut SupportElaboration,
    target: &str,
    criterion: EvaluationCriterion,
    step: &str,
    pointer: &str,
) {
    let requirement = elaboration
        .requirements
        .iter_mut()
        .find(|r| r.target_id == target && r.role == SupportRequirementRole::Support)
        .unwrap_or_else(|| panic!("No support obligation for {target}"));
    requirement.acceptance = criterion.clone();
    let evaluation = elaboration
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == requirement.id)
        .unwrap();
    evaluation.criterion = criterion;
    evaluation.inputs =
        serde_json::from_value(json!([{"step_id":step,"pointer":pointer}])).unwrap();
    evaluation.required_coverage = RequiredCoverage::Witness;
}

fn native(index: usize, negative: bool) -> (Request, ArgumentCase, SupportElaboration, Value) {
    if index == 0 {
        let (mut request, mut case, elaboration) = executable_case::fixture();
        if negative {
            let mut encoded = serde_json::to_value(&case).unwrap();
            encoded["warrants"][0]["reasoning_method"]["method"]["query"]["conclusion"] =
                json!("r");
            case = serde_json::from_value(encoded).unwrap();
            request.claim =
                "The supplied premises p and p implies q entail the independent atom r.".into();
        }
        return (
            request,
            case,
            elaboration,
            json!({"count":3,"distinct":true}),
        );
    }
    let mut mode = reasoning::cases().remove(index);
    if negative {
        match index {
            1 => mode.payload["successes"] = json!(50),
            2 => {
                mode.payload["candidates"][0]["likelihood"] = json!(0.1);
                mode.payload["candidates"][1]["likelihood"] = json!(0.9);
                mode.method["query"] = mode.payload.clone();
            }
            3 => {
                mode.payload["treatment"] = json!([1, 3]);
                mode.payload["control"] = json!([7, 9]);
            }
            4 => {
                mode.payload["intervention"]["value"] = json!(1);
                mode.method["query"] = mode.payload.clone();
            }
            _ => unreachable!(),
        }
    }
    let (mut request, case, elaboration, _, _) = reasoning_graph::graph_fixture(&mode);
    if negative && index == 4 {
        request.claim =
            "Under the supplied affine model, setting X to one increases Y by six seconds.".into();
    }
    (request, case, elaboration, mode.payload)
}

fn design(index: usize) -> Value {
    match index {
        0 => {
            json!({"snapshot":"finite-snapshot-v1","equality":"case-sensitive","premise_interpretation":"count-three-and-distinct"})
        }
        1 => {
            json!({"sampling":"independent-Bernoulli","stopping":"fixed-100-trials","allocation_record":"sample-design-v1"})
        }
        2 => {
            json!({"hypothesis_space":["A","B"],"calibration_record":"binary-alarm-calibration-v1","prior_basis":"equal-priors"})
        }
        3 => {
            json!({"assignment":"randomised","allocation_record":"seed-17-block-v1","interference":"absent-in-fixture"})
        }
        4 => json!({"model":"affine-v1","exogenous_basis":{"X":0,"Y":1},"outcome_unit":"s"}),
        5 => {
            json!({"source_case":"source-v1","target_case":"target-v1","comparison_basis":"interface-and-bounded-load","source_outcome_s":2})
        }
        6 => {
            json!({"cost_basis":"GBP-fixed-quote-v1","duration_basis":"measured-single-job-v1","availability_snapshot":"options-v1"})
        }
        _ => unreachable!(),
    }
}

fn scoped_review(fixture: &mut Fixture, criterion: &str, reasons: Vec<&str>) {
    let contract =
        reasoning_graph::contract("judgement", "judgement", "review", "judgement_record");
    let scope = contract.scope.clone();
    fixture
        .elaboration
        .requirements
        .iter_mut()
        .find(|r| r.target_id == "W1")
        .unwrap()
        .contract = Some(contract.clone());
    requirement(
        &mut fixture.elaboration,
        "W1",
        EvaluationCriterion::Judgement {
            criterion: criterion.into(),
            scope: Some(scope.clone()),
        },
        "review_source",
        "/review",
    );
    fixture
        .elaboration
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == "requirement_W1")
        .unwrap()
        .required_coverage = RequiredCoverage::Unspecified;
    let mut acquisition = fixture.elaboration.acquisitions[0].clone();
    acquisition.id = "review_source".into();
    acquisition.arguments["operation"] = json!("review_source");
    acquisition.arguments["observed"] = Value::Null;
    acquisition.bindings = serde_json::from_value(json!([{
        "output":{"step_id":"premise_source","pointer":""},"argument_pointer":"/observed"
    }]))
    .unwrap();
    acquisition.contract = contract;
    fixture.elaboration.acquisitions.push(acquisition);
    fixture.observation["review"] = json!({
        "criterion":criterion,"target":"W1","scope":scope,
        "reviewer":"independent-fixture-reviewer-v1","verdict":"supported",
        "reasons":reasons,"reviewed_at":reasoning::NOW,
        "evidence_refs":["frozen-fixture-v1","design-review-v1"]
    });
}

fn analogy(fixture: &mut Fixture, negative: bool) {
    let measurement = json!({"relevant_features":["interface","load"],"source":{"interface":"v1","load":3},"target":{"interface":if negative {"v2"} else {"v1"},"load":3}});
    let mut authored = serde_json::to_value(&fixture.case).unwrap();
    authored["warrants"][0]["inference"] = json!("analogical");
    let binding = &mut authored["warrants"][0]["reasoning_method"];
    binding["method"] = json!({"identifier":"analogical/1","query":measurement});
    binding["formal_premises"] = json!(["relevant_features","source","target"].into_iter().map(|field|json!({
        "premise_id":"G1","input_pointer":format!("/{field}"),"evidence_need_id":"N7",
        "correspondence":{"kind":"identity_value","source_input_index":0,"source_pointer":format!("/{field}")},
        "interpretation":"Observed source and target attributes of the two identified deployments."
    })).collect::<Vec<_>>());
    binding["conclusion"]["quantity"] = json!("dimensionless");
    binding["conclusion"]["result"] = json!({"path":"match_fraction","operator":"==","expected":1});
    fixture.case = serde_json::from_value(authored).unwrap();
    fixture.observation["measurement"] = measurement.clone();
    fixture.elaboration.acquisitions[0]
        .contract
        .quantities
        .push(serde_json::from_value(json!({"name":"transfer","unit":"record"})).unwrap());
    fixture.observation["transfer"] = json!({"source":{"id":"source-v1","region":"A","observed_duration_s":2},"target":{"id":"target-v1","region":"B"},"differences":[{"property":"region","source":"A","target":"B","relevance":"excluded by the deterministic fixture's network-free computation contract"}],"proposed_limit_s":3});
    requirement(
        &mut fixture.elaboration,
        "G1",
        EvaluationCriterion::Equals {
            expected: measurement,
        },
        "premise_source",
        "/measurement",
    );
    scoped_review(
        fixture,
        "The source's three-second bound transfers to target-v1 when interface and load match; the identified region difference is irrelevant under the network-free fixture contract.",
        vec![
            "Source-v1 completed the identified workload in two seconds against a three-second criterion.",
            "The same interface and bounded load identify the relevant mechanism; the recorded region change adds no network work in this declared fixture.",
            "The transfer remains conditional on these two relevant features matching and on the retained design contract.",
        ],
    );
    fixture.request.claim = "Target-v1 meets the three-second fixture criterion by transfer from source-v1, conditional on matching interface/load and the scoped review of the recorded region difference.".into();
}

fn practical(fixture: &mut Fixture, negative: bool, exhausted: bool) {
    fixture.case.warrants[0].inference = engarg::domain::InferenceKind::Practical;
    fixture.case.warrants[0].reasoning_method = None;
    let measurement = json!({
        "options":[
            {"id":"A","duration_s":8,"cost_gbp":5,"available":true},
            {"id":"B","duration_s":5,"cost_gbp":if negative {20} else {8},"available":true},
            {"id":"C","duration_s":2,"cost_gbp":20,"available":true}
        ],
        "constraints":{"maximum_cost_gbp":10},
        "objective":"minimum duration among available options within cost cap; lexical id breaks ties"
    });
    fixture.observation["measurement"] = measurement.clone();
    requirement(
        &mut fixture.elaboration,
        "G1",
        EvaluationCriterion::Equals {
            expected: measurement,
        },
        "premise_source",
        "/measurement",
    );
    scoped_review(
        fixture,
        "For this single-job fixture, minimise observed duration subject to the ten-pound cost cap and current availability, with lexical option ID breaking ties.",
        vec![
            "The stated objective and ten-pound cap represent the decision-maker's supplied trade-off for this job.",
            "Each option has an identified duration consequence, cost quote and availability observation under the same retained basis.",
            "The executable policy must evaluate every option; this review does not certify the chosen option.",
        ],
    );
    let mut policy = fixture.elaboration.acquisitions[0].clone();
    policy.id = "policy_source".into();
    policy.contract = reasoning_graph::contract(
        "calculation",
        "existential_witness",
        "calculation",
        "record",
    );
    policy.arguments = json!({"data":null,"work_budget":if exhausted {1} else {3}});
    policy.bindings = serde_json::from_value(json!([{"output":{"step_id":"premise_source","pointer":"/measurement"},"argument_pointer":"/data"}])).unwrap();
    fixture.elaboration.acquisitions.push(policy);
    let mut obligation = fixture
        .elaboration
        .requirements
        .iter()
        .find(|r| r.target_id == "current_claim")
        .unwrap()
        .clone();
    let mut evaluation = fixture
        .elaboration
        .evaluations
        .iter()
        .find(|e| e.requirement_id == obligation.id)
        .unwrap()
        .clone();
    obligation.id = "decision_policy".into();
    obligation.evidence_need_ids.clear();
    obligation.original_acceptance_criteria.clear();
    obligation.contract = Some(reasoning_graph::contract(
        "calculation",
        "existential_witness",
        "calculation",
        "record",
    ));
    obligation.acceptance = EvaluationCriterion::Equals {
        expected: json!({"status":"complete","selected":"B"}),
    };
    obligation.acceptance_rationale = "The declared finite policy completes every option visit and chooses B from the acquired options, costs, availability and duration consequences.".into();
    evaluation.id = "decision_policy_check".into();
    evaluation.requirement_id = obligation.id.clone();
    evaluation.criterion = obligation.acceptance.clone();
    evaluation.inputs =
        serde_json::from_value(json!([{"step_id":"policy_source","pointer":"/calculation"}]))
            .unwrap();
    fixture.elaboration.requirements.push(obligation);
    fixture.elaboration.evaluations.push(evaluation);
    fixture.request.claim = "Choose B for this single job under the reviewed minimum-duration policy, the ten-pound cost cap, and the retained option consequences and availability.".into();
}

fn challenge(fixture: &mut Fixture, scenario: &str) {
    fixture.case.defeaters.push(serde_json::from_value(json!({
        "id":"D1","target_id":"A1","kind":"undercutting",
        "challenge":"A separately acquired design audit identifies a mismatch in the retained applicability basis.",
        "basis":"hypothetical","evidence_ids":[],"material":true,"status":"open",
        "response":"An identified resolution observation must settle this mismatch before reliance."
    })).unwrap());
    let mut need = fixture.case.evidence_plan[0].clone();
    need.id = "N8".into();
    need.target_id = "D1".into();
    let criterion_text = need.acceptance_criterion.clone();
    fixture.case.evidence_plan.push(need);
    let base = fixture
        .elaboration
        .requirements
        .iter()
        .find(|r| r.target_id == "A1")
        .unwrap()
        .clone();
    let check = fixture
        .elaboration
        .evaluations
        .iter()
        .find(|e| e.requirement_id == base.id)
        .unwrap()
        .clone();
    for (id, role, pointer) in [
        (
            "activation",
            SupportRequirementRole::ChallengeActivation,
            "/active",
        ),
        (
            "resolution",
            SupportRequirementRole::ChallengeResolution,
            "/resolved",
        ),
    ] {
        let mut obligation = base.clone();
        obligation.id = id.into();
        obligation.target_id = "D1".into();
        obligation.role = role;
        obligation.contract = Some(reasoning_graph::contract(
            "observation",
            "existential_witness",
            &pointer[1..],
            "boolean",
        ));
        obligation.acceptance = EvaluationCriterion::AllTrue;
        obligation.evidence_need_ids = if id == "activation" {
            vec!["N8".into()]
        } else {
            vec![]
        };
        obligation.original_acceptance_criteria = if id == "activation" {
            vec![criterion_text.clone()]
        } else {
            vec![]
        };
        let mut evaluation = check.clone();
        evaluation.id = format!("{id}_check");
        evaluation.requirement_id = id.into();
        evaluation.criterion = EvaluationCriterion::AllTrue;
        evaluation.inputs =
            serde_json::from_value(json!([{"step_id":"challenge_source","pointer":pointer}]))
                .unwrap();
        fixture.elaboration.requirements.push(obligation);
        fixture.elaboration.evaluations.push(evaluation);
    }
    let mut acquisition = fixture.elaboration.acquisitions[0].clone();
    acquisition.id = "challenge_source".into();
    acquisition.arguments =
        json!({"fixture":"frozen-fixture-v1","operation":"challenge_source","scenario":scenario});
    acquisition.contract =
        reasoning_graph::contract("observation", "existential_witness", "active", "boolean");
    acquisition
        .contract
        .quantities
        .push(serde_json::from_value(json!({"name":"resolved","unit":"boolean"})).unwrap());
    fixture.elaboration.acquisitions.push(acquisition);
}

pub fn fixture(index: usize, scenario: &str) -> Fixture {
    let negative = scenario == "negative";
    let (request, case, elaboration, measurement) =
        native(if index >= 5 { 0 } else { index }, negative && index < 5);
    let mut fixture = Fixture {
        request,
        case,
        elaboration,
        observation: json!({"measurement":measurement,"design":design(index)}),
    };
    // Native graph fixtures require ground, warrant and assumption support.
    // This composed matrix additionally makes the final composition condition
    // explicit for every mode, including those without a claim-targeted need.
    if !fixture
        .elaboration
        .requirements
        .iter()
        .any(|r| r.target_id == "current_claim")
    {
        let mut composition = fixture
            .elaboration
            .requirements
            .iter()
            .find(|r| r.target_id == "W1")
            .unwrap()
            .clone();
        let mut evaluation = fixture
            .elaboration
            .evaluations
            .iter()
            .find(|e| e.requirement_id == composition.id)
            .unwrap()
            .clone();
        composition.id = "composition_condition".into();
        composition.target_id = "current_claim".into();
        composition.evidence_need_ids.clear();
        composition.original_acceptance_criteria.clear();
        evaluation.id = "composition_condition_check".into();
        evaluation.requirement_id = composition.id.clone();
        fixture.elaboration.requirements.push(composition);
        fixture.elaboration.evaluations.push(evaluation);
    }
    fixture
        .elaboration
        .acquisitions
        .retain(|a| a.id == "premise_source");
    fixture.elaboration.acquisitions[0]
        .contract
        .quantities
        .push(serde_json::from_value(json!({"name":"design","unit":"record"})).unwrap());
    fixture.elaboration.acquisitions[0].arguments =
        json!({"fixture":"frozen-fixture-v1","scenario":scenario,"operation":"premise_source"});
    for target in ["A1", "W1", "current_claim"] {
        requirement(
            &mut fixture.elaboration,
            target,
            EvaluationCriterion::Equals {
                expected: design(index),
            },
            "premise_source",
            "/design",
        );
        fixture
            .elaboration
            .requirements
            .iter_mut()
            .find(|r| r.target_id == target)
            .unwrap()
            .contract = Some(reasoning_graph::contract(
            "observation",
            "existential_witness",
            "design",
            "record",
        ));
    }
    // The finite executable fixture also carries an explicit composition obligation.
    for evaluation in &mut fixture.elaboration.evaluations {
        for reference in evaluation
            .inputs
            .iter_mut()
            .chain(&mut evaluation.applicability)
            .chain(&mut evaluation.challenges)
        {
            if reference.step_id == "applicability_source" {
                reference.step_id = "premise_source".into();
                reference.pointer = "/design".into();
                evaluation.criterion = EvaluationCriterion::Equals {
                    expected: design(index),
                };
            }
        }
    }
    for obligation in &mut fixture.elaboration.requirements {
        if obligation.id == "composition_condition" {
            obligation.acceptance = EvaluationCriterion::Equals {
                expected: design(index),
            };
        }
    }
    if index == 5 {
        analogy(&mut fixture, negative);
    }
    if index == 6 {
        practical(&mut fixture, negative, scenario == "exhausted");
    }
    if ["challenged", "resolved-challenge", "challenge-unavailable"].contains(&scenario) {
        challenge(&mut fixture, scenario);
    }
    fixture.case.original_claim = fixture.request.claim.clone();
    fixture.case.current_claim.text = fixture.request.claim.clone();
    fixture.case.reading[0].paragraphs[0].text = fixture.request.claim.clone();
    engarg::domain::validate_case(&fixture.request, &fixture.case).unwrap();
    let errors =
        engarg::support_requirements::validate_elaboration(&fixture.case, &fixture.elaboration);
    assert!(
        errors.is_empty(),
        "{}: {}: {}",
        MODES[index],
        scenario,
        errors.join("; ")
    );
    fixture
}
