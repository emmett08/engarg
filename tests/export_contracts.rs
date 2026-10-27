//! Shared collector admission and authored freshness remain independently checked.
use engarg::{domain::ArgumentCase, export_policy::ExportPolicy};
use serde_json::Value;

fn case() -> ArgumentCase {
    serde_json::from_str(include_str!("../examples/case.json")).unwrap()
}

#[test]
fn resolved_freshness_covers_each_need_and_preserves_explicit_bounds() {
    let case = case();
    let mut policy = ExportPolicy {
        review_max_age_seconds: 120,
        acquisition_max_age_seconds: 300,
        ..Default::default()
    };
    policy
        .acquisition_per_need
        .insert(case.evidence_plan[0].id.clone(), 30);
    let resolved = policy.resolve(&case).unwrap();
    assert_eq!(resolved.review_max_age_seconds, 120);
    assert_eq!(
        resolved.acquisition_per_need.len(),
        case.evidence_plan.len()
    );
    for (index, need) in case.evidence_plan.iter().enumerate() {
        assert_eq!(
            resolved.acquisition_per_need[&need.id],
            if index == 0 { 30 } else { 300 }
        );
    }
}

#[test]
fn default_is_explicit_and_invalid_freshness_is_rejected() {
    let case = case();
    assert_eq!(
        ExportPolicy::default()
            .resolve(&case)
            .unwrap()
            .review_max_age_seconds,
        86400
    );
    for age in [0, 315_360_001, u64::MAX] {
        let policy = ExportPolicy {
            acquisition_max_age_seconds: age,
            ..Default::default()
        };
        assert!(policy.resolve(&case).is_err());
    }
    let mut policy = ExportPolicy::default();
    policy.acquisition_per_need.insert("unknown-need".into(), 1);
    assert!(policy.resolve(&case).is_err());
    policy = ExportPolicy::default();
    policy.schema = "engarg.export-policy/999".into();
    assert!(policy.resolve(&case).is_err());
}

#[test]
fn duplicate_structural_keys_are_not_silently_replaced() {
    let raw = br#"{"schema":"a","schema":"b"}"#;
    assert!(eal_collector_contracts::strict_json(raw, Default::default()).is_err());
}

#[test]
fn registry_contract_corpus_agrees_with_authoritative_owner() {
    let corpus: Vec<Value> =
        serde_json::from_str(eal_collector_contracts::REGISTRY_CORPUS).unwrap();
    for entry in corpus {
        assert_eq!(
            eal_collector_contracts::RegistryDocument::from_toml(
                entry["registry"].as_str().unwrap()
            )
            .is_ok(),
            entry["accepted"].as_bool().unwrap(),
            "{}",
            entry["name"],
        );
    }
}

#[test]
fn authoring_catalogue_agrees_with_shared_contract_corpus() {
    let corpus: Vec<Value> =
        serde_json::from_str(eal_collector_contracts::CATALOGUE_CORPUS).unwrap();
    for entry in corpus {
        let bytes = serde_json::to_vec(&entry["catalogue"]).unwrap();
        assert_eq!(
            engarg::collector_catalogue::CollectorCatalogue::from_json(&bytes).is_ok(),
            entry["profile_accepted"].as_bool().unwrap(),
            "{}",
            entry["name"],
        );
    }
}
