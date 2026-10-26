//! Topology authoring crosses the provider schema and native EAL compiler boundaries.

use std::sync::atomic::{AtomicUsize, Ordering};

use eal_collector_contracts::execution::EvidenceExecutionPlan;
use eal_core::{ExecutionLimits, composition, planning::EvidencePlanner, validation};
use engarg::{
    Error,
    domain::{ArgumentCase, Request},
    eal_acquisition, eal_export,
    engine::{
        RouteResolver, RunConfig, run, run_with_support, run_with_tools,
        topology::{DebateConfig, Topology},
    },
    export_policy::ExportPolicy,
    provider::{Completion, CompletionRequest, LanguageModel, Role},
    support_export,
    support_requirements::{self, SupportElaboration},
};
use serde_json::{Value, json};

#[path = "common/argument_assertions.rs"]
mod argument_assertions;
#[path = "common/live_output.rs"]
mod live_output;
#[path = "common/reasoning.rs"]
#[allow(dead_code)]
mod reasoning;
#[path = "common/reasoning_graph.rs"]
#[allow(dead_code)]
mod reasoning_graph;

const TOPOLOGIES: [&str; 9] = [
    "fully_connected",
    "grouped",
    "chain",
    "ring",
    "star",
    "tree",
    "layered",
    "sparse",
    "bipartite",
];

#[derive(Clone, Copy)]
enum Envelope {
    Case,
    Acquisition,
    Support,
}

struct SchemaProvider {
    case: ArgumentCase,
    elaboration: SupportElaboration,
    envelope: Envelope,
    calls: AtomicUsize,
}

fn assert_schema(schema: &Value, value: &Value, description: &str) {
    let validator = jsonschema::validator_for(schema)
        .unwrap_or_else(|error| panic!("{description} schema is invalid: {error}"));
    let errors = validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{description}: {}", errors.join("; "));
}

impl LanguageModel for SchemaProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let case_writer = matches!(
            request.role,
            Role::Single | Role::Synthesiser | Role::Adjudicator
        );
        let value = if case_writer {
            let case = live_output::generation_value(&self.case);
            match self.envelope {
                Envelope::Case => case.clone(),
                Envelope::Acquisition => json!({
                    "argument": case,
                    "acquisition_plan": self.case.evidence_plan.iter().map(|need| json!({
                        "need_id": need.id,
                        "tool_id": "custom",
                        "arguments": [],
                        "unresolved_arguments": ["adapter"],
                        "input_preparation": null,
                    })).collect::<Vec<_>>(),
                }),
                Envelope::Support => {
                    let elaboration = serde_json::to_value(&self.elaboration)?;
                    assert_schema(
                        &support_requirements::elaboration_schema(),
                        &elaboration,
                        "decoded support elaboration",
                    );
                    json!({
                        "argument": case,
                        "support_elaboration_json": serde_json::to_string(&elaboration)?,
                    })
                }
            }
        } else {
            assert!(request.schema["properties"].get("findings").is_some());
            assert!(request.schema["properties"].get("argument").is_none());
            json!({
                "findings": ["The declared formal query still requires its identified input and independent applicability observations."],
                "objections": [],
                "evidence_gaps": ["The source record and applicability record remain unacquired."],
            })
        };
        assert_schema(&request.schema, &value, request.role.as_str());
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Completion {
            content: serde_json::to_string(&value)?,
            model: "schema-validated-native-boundary-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

struct FixtureResolver(EvidenceExecutionPlan);

impl RouteResolver for FixtureResolver {
    type Plan = EvidenceExecutionPlan;

    async fn resolve<'a>(
        &'a self,
        _: &'a ArgumentCase,
        _: &'a SupportElaboration,
    ) -> Result<Self::Plan, Error> {
        Ok(self.0.clone())
    }
}

fn config(topology: &str) -> RunConfig {
    RunConfig {
        debate: Some(DebateConfig {
            participants: 4,
            rounds: 1,
            topology: Topology::preset(topology, 4).unwrap(),
            adaptation: Default::default(),
        }),
        max_calls: 11,
        ..RunConfig::default()
    }
}

fn assert_native_eal_plan(source: &str, claim: &str, expected_method: &str) {
    let program = composition::compile(source, None, "", ExecutionLimits::default()).unwrap();
    let diagnostics = validation::validate_default(&program);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(
        program
            .reasoning
            .values()
            .any(|reasoning| reasoning.method == expected_method),
        "the compiled EAL programme must retain the selected computational method"
    );
    let plan = EvidencePlanner::new(&program).unwrap().plan(claim).unwrap();
    assert!(!plan.calls.is_empty());
    assert!(
        plan.calls
            .iter()
            .any(|call| call.tool == "engarg_reasoning_input")
    );
}

#[tokio::test]
async fn every_topology_and_authoring_envelope_produces_schema_valid_native_eal() {
    let mode = reasoning::cases().remove(0);
    let (request, expected, elaboration, execution, routes, locations) =
        reasoning_graph::graph_fixture(&mode);
    let canonical = serde_json::to_value(&expected).unwrap();

    for topology in TOPOLOGIES {
        for envelope in [Envelope::Case, Envelope::Acquisition, Envelope::Support] {
            let provider = SchemaProvider {
                case: expected.clone(),
                elaboration: elaboration.clone(),
                envelope,
                calls: AtomicUsize::new(0),
            };
            let config = config(topology);
            let (report, source, claim) = match envelope {
                Envelope::Case | Envelope::Acquisition => {
                    let report = match envelope {
                        Envelope::Case => run(&provider, &request, &config).await.unwrap(),
                        _ => run_with_tools(&provider, &request, &config, &|_| Ok(()))
                            .await
                            .unwrap(),
                    };
                    let bundle = if matches!(envelope, Envelope::Acquisition) {
                        let planned = engarg::acquisition::parse_completion(
                            &report.transcript.last().unwrap().response.content,
                        )
                        .unwrap();
                        let artifacts = eal_acquisition::export_with_plan(
                            &request,
                            &report.argument,
                            &planned.acquisition_plan,
                        )
                        .unwrap();
                        assert_eq!(
                            eal_acquisition::plan_from_bundle(&artifacts.bundle)
                                .unwrap()
                                .unwrap()
                                .len(),
                            expected.evidence_plan.len()
                        );
                        artifacts.bundle
                    } else {
                        eal_export::export(&request, &report.argument).unwrap()
                    };
                    let claim = bundle.claim_names["current_claim"].clone();
                    (report, bundle.source, claim)
                }
                Envelope::Support => {
                    let result = run_with_support(
                        &provider,
                        &request,
                        &config,
                        &|_| Ok(()),
                        &FixtureResolver(execution.clone()),
                    )
                    .await
                    .unwrap();
                    let artifacts = support_export::export(
                        &request,
                        &result.report.argument,
                        &result.evidence_planning.elaboration,
                        &result.evidence_planning.routes,
                        &routes,
                        &locations,
                        &ExportPolicy::default(),
                    )
                    .unwrap();
                    let claim = artifacts.bundle.claim_names["current_claim"].clone();
                    (result.report, artifacts.bundle.source, claim)
                }
            };
            argument_assertions::assert_authored_argument(
                &serde_json::to_value(&report).unwrap(),
                &canonical,
            );
            assert_eq!(provider.calls.load(Ordering::SeqCst), 11, "{topology}");
            assert_native_eal_plan(&source, &claim, "deductive/1");
        }
    }
}

#[test]
fn all_native_method_bindings_survive_provider_schema_and_eal_planning() {
    for mode in reasoning::cases() {
        let (request, case): (Request, ArgumentCase) = reasoning::fixture(&mode);
        let generated = live_output::generation_value(&case);
        assert_schema(&engarg::domain::case_schema(), &generated, mode.kind);
        let bundle = eal_export::export(&request, &case).unwrap();
        assert_native_eal_plan(
            &bundle.source,
            &bundle.claim_names["current_claim"],
            &format!("{}/1", mode.kind),
        );
    }
}
