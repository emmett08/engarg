//! Current export gates retain authored identity and require a supported review.
use eal_core::{ExecutionLimits, composition, expressions};
use engarg::eal_export::EalExport;
use serde_json::json;

pub fn assert_scoped_review_gates(bundle: &EalExport) {
    let program =
        composition::compile(&bundle.source, None, "", ExecutionLimits::default()).unwrap();
    assert_eq!(
        program
            .evidence
            .values()
            .filter(|e| e.kind == "scoped_review")
            .count(),
        bundle.obligations.len()
    );
    assert!(!bundle.source.contains("require accepted == true"));
    for obligation in &bundle.obligations {
        let evidence = &program.evidence[&obligation.eal_evidence_name];
        assert_eq!(evidence.kind, "scoped_review");
        assert_eq!(evidence.tool.as_deref(), Some("engarg_review_contract"));
        assert_eq!(evidence.input["criterion"], obligation.requirement);
        assert_eq!(evidence.input["target"], obligation.target_id);
        assert_eq!(
            evidence.input["scope"],
            json!({"case_digest":bundle.revision,"target_id":obligation.target_id})
        );
        for verdict in ["supported", "rejected", "inconclusive"] {
            let value = json!({"criterion":obligation.requirement,"target":obligation.target_id,"scope":evidence.input["scope"],
                "reviewer":"synthetic-gate-reviewer","verdict":verdict,"reasons":["Synthetic predicate regression only."],"reviewed_at":"2026-10-23T12:00:00Z"});
            assert_eq!(
                evidence
                    .predicates
                    .iter()
                    .all(|predicate| expressions::evaluate_expression(
                        &predicate.expression.clone().unwrap_or_else(|| {
                            eal_core::Expression::Binary {
                                operator: expressions::comparison_operator(&predicate.operator)
                                    .unwrap(),
                                left: Box::new(eal_core::Expression::Field {
                                    name: predicate.path.clone(),
                                }),
                                right: Box::new(eal_core::Expression::Literal {
                                    value: expressions::try_json_to_literal(
                                        &predicate.expected,
                                        &ExecutionLimits::default(),
                                    )
                                    .unwrap(),
                                }),
                            }
                        }),
                        &value
                    )
                    .value
                        == Some(json!(true))),
                verdict == "supported",
                "{}: {verdict}",
                obligation.id
            );
        }
    }
}
