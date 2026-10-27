//! Persisted structural authority and freshness are independently inspectable.
use engarg::{
    domain::{ArgumentCase, Request},
    eal_acquisition::{export_with_policy, reconstruct_bundle},
    export_policy::ExportPolicy,
};
use serde_json::{Value, json};
fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}
#[test]
fn structural_plan_and_resolved_policy_are_bound_without_source_markers() {
    let (request, case) = fixture();
    let mut policy = ExportPolicy {
        review_max_age_seconds: 120,
        acquisition_max_age_seconds: 300,
        ..Default::default()
    };
    policy
        .acquisition_per_need
        .insert(case.evidence_plan[0].id.clone(), 30);
    let bundle = export_with_policy(&request, &case, &[], None, &policy)
        .unwrap()
        .bundle;
    assert_eq!(bundle.schema_version, "engarg.eal-export/3");
    assert!(!bundle.source.contains("engarg_acquisition_plan"));
    assert!(bundle.source.contains("max_age 120\n"));
    assert!(bundle.source.contains("max_age 30\n"));
    assert!(bundle.source.contains("max_age 300\n"));
    let resolved = bundle.export_policy.as_ref().unwrap();
    assert_eq!(
        resolved.acquisition_per_need.len(),
        case.evidence_plan.len()
    );
    assert_eq!(
        bundle.acquisition_plan.as_ref().unwrap().entries.len(),
        case.evidence_plan.len()
    );
    assert_eq!(
        serde_json::to_value(reconstruct_bundle(&bundle).unwrap()).unwrap(),
        serde_json::to_value(&bundle).unwrap()
    );
    for mutation in 0..5 {
        let mut changed = bundle.clone();
        match mutation {
            0 => {
                changed.acquisition_plan.as_mut().unwrap().entries[0].tool_id =
                    "artifact_json".into()
            }
            1 => {
                changed
                    .export_policy
                    .as_mut()
                    .unwrap()
                    .review_max_age_seconds += 1
            }
            2 => changed.authoring_profile = Some("unknown/9".into()),
            3 => changed
                .source
                .push_str("\n // engarg_acquisition_plan []\n"),
            _ => {
                changed
                    .export_policy
                    .as_mut()
                    .unwrap()
                    .acquisition_per_need
                    .clear();
            }
        }
        assert!(reconstruct_bundle(&changed).is_err(), "mutation {mutation}");
    }
}
#[test]
fn default_is_explicit_and_zero_unknown_and_excessive_ages_are_rejected() {
    let (request, case) = fixture();
    let bundle = export_with_policy(&request, &case, &[], None, &ExportPolicy::default())
        .unwrap()
        .bundle;
    assert_eq!(
        bundle
            .export_policy
            .as_ref()
            .unwrap()
            .review_max_age_seconds,
        86400
    );
    for age in [0, 315_360_001, u64::MAX] {
        let policy = ExportPolicy {
            acquisition_max_age_seconds: age,
            ..Default::default()
        };
        assert!(export_with_policy(&request, &case, &[], None, &policy).is_err());
    }
    let mut policy = ExportPolicy::default();
    policy.acquisition_per_need.insert("unknown-need".into(), 1);
    assert!(policy.resolve(&case).is_err());
}
#[test]
fn version_and_duplicate_structural_keys_fail_clearly() {
    let (request, case) = fixture();
    let bundle = export_with_policy(&request, &case, &[], None, &ExportPolicy::default())
        .unwrap()
        .bundle;
    let mut value = serde_json::to_value(bundle).unwrap();
    value["schema_version"] = json!("engarg.eal-export/999");
    let changed = serde_json::from_value(value).unwrap();
    assert!(reconstruct_bundle(&changed).is_err());
    let raw = br#"{"schema":"a","schema":"b"}"#;
    assert!(eal_collector_contracts::strict_json(raw, Default::default()).is_err());
}
#[test]
fn registry_contract_corpus_agrees_with_authoritative_owner() {
    let corpus: Vec<Value> =
        serde_json::from_str(eal_collector_contracts::REGISTRY_CORPUS).unwrap();
    for entry in corpus {
        let accepted = eal_collector_contracts::RegistryDocument::from_toml(
            entry["registry"].as_str().unwrap(),
        )
        .is_ok();
        assert_eq!(
            accepted,
            entry["accepted"].as_bool().unwrap(),
            "{}",
            entry["name"]
        );
    }
}

#[test]
fn unsupported_export_formats_require_explicit_reexport() {
    let (request, case) = fixture();
    let current = export_with_policy(&request, &case, &[], None, &ExportPolicy::default())
        .unwrap()
        .bundle;
    for variant in 0..6 {
        let mut obsolete = current.clone();
        match variant {
            0 => obsolete.schema_version = "engarg.eal-export/1".into(),
            1 => obsolete.schema_version = "engarg.eal-export/2".into(),
            2 => obsolete.acquisition_plan = None,
            3 => obsolete.export_policy = None,
            4 => obsolete.semantics_version = None,
            _ => obsolete
                .source
                .push_str("\n// engarg_acquisition_plan []\n"),
        }
        let error = reconstruct_bundle(&obsolete).unwrap_err().to_string();
        assert!(
            error.contains("unsupported export schema"),
            "variant {variant}: {error}"
        );
    }
}

#[test]
fn authoring_profile_agrees_with_the_shared_catalogue_corpus() {
    let corpus: Vec<Value> =
        serde_json::from_str(eal_collector_contracts::CATALOGUE_CORPUS).unwrap();
    for entry in corpus {
        let bytes = serde_json::to_vec(&entry["catalogue"]).unwrap();
        assert_eq!(
            engarg::collector_catalogue::CollectorCatalogue::from_json(&bytes).is_ok(),
            entry["profile_accepted"].as_bool().unwrap(),
            "{}",
            entry["name"]
        );
    }
}
