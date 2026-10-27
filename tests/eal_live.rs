//! Optional integration checks against the installed EAL text host and MCP server.
//!
//! Install `eal-host` together with its sibling EAL executables, then run:
//! `ENGARG_EAL_HOST=/path/to/eal-host cargo test --test eal_live -- --ignored`.
//! These tests never invoke evidence collection or install a collector.

use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use engarg::{
    domain::{ArgumentCase, Request, SupportRelationship},
    eal_export::{EalExport, export},
    eal_host::{HostConfig, invoke},
};
use serde_json::{Value, json};
use tempfile::TempDir;

const NOW: &str = "2026-10-23T12:00:00Z";

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn host() -> (TempDir, HostConfig) {
    let executable =
        PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect(
            "set ENGARG_EAL_HOST to a real installed eal-host before running ignored tests",
        ));
    assert!(executable.is_file(), "eal-host must exist: {executable:?}");
    let workspace = tempfile::tempdir().unwrap();
    let config = HostConfig {
        executable,
        workspace: workspace.path().to_path_buf(),
        database: Some(workspace.path().join("assessments.sqlite")),
        registry: None,
        collector_env: Vec::new(),
        public_collector_env: Vec::new(),
        url: None,
        token_env: None,
        timeout: Duration::from_secs(15),
    };
    (workspace, config)
}

async fn result(config: &HostConfig, request: Value) -> Value {
    let envelope = invoke(config, &request)
        .await
        .unwrap_or_else(|error| panic!("{} failed: {error}", request["operation"]));
    assert_eq!(envelope["operation"], request["operation"]);
    assert_eq!(envelope["is_error"], false, "{envelope}");
    assert!(
        envelope["protocol_version"]
            .as_str()
            .is_some_and(|version| !version.is_empty()),
        "the actual MCP host must retain its negotiated protocol version"
    );
    assert!(envelope["result"].is_object(), "{envelope}");
    envelope["result"].clone()
}

fn ids(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .expect("identifier array")
        .iter()
        .map(|value| value.as_str().expect("string identifier"))
        .collect()
}

fn declaration<'a>(bundle: &'a EalExport, id: &str, role: &str) -> &'a str {
    &bundle.source_map[id]
        .declarations
        .iter()
        .find(|declaration| declaration.role == role)
        .unwrap_or_else(|| panic!("missing {role} declaration for {id}"))
        .name
}

#[tokio::test]
#[ignore = "requires a real EAL installation; set ENGARG_EAL_HOST"]
async fn exported_case_validates_and_plans_unfulfilled_review_contracts() {
    let (_workspace, config) = host();
    let (request, case) = fixtures();
    let bundle = export(&request, &case).unwrap();
    let validation = result(
        &config,
        json!({"operation":"validate", "source":bundle.source}),
    )
    .await;
    assert_eq!(validation["valid"], true, "{validation}");
    assert!(validation["diagnostics"].as_array().unwrap().is_empty());

    let current = &bundle.claim_names["current_claim"];
    let plan = result(
        &config,
        json!({"operation":"plan", "source":bundle.source, "claim":current}),
    )
    .await;
    assert_eq!(plan["claim"], *current);
    assert_eq!(plan["source_digest"], validation["source_digest"]);
    let planned = ids(&plan["evidence_ids"]);
    assert!(!planned.is_empty());
    assert_eq!(plan["estimated_calls"].as_u64(), Some(planned.len() as u64));
    let obligations: BTreeSet<&str> = bundle
        .obligations
        .iter()
        .map(|obligation| obligation.eal_evidence_name.as_str())
        .collect();
    assert!(planned.is_subset(&obligations));
    let composition_review = bundle
        .obligations
        .iter()
        .find(|obligation| {
            obligation.target_id == "current_claim" && obligation.kind == "composition_adequacy"
        })
        .unwrap();
    assert!(planned.contains(composition_review.eal_evidence_name.as_str()));
    assert!(plan["calls"].as_array().unwrap().iter().all(|call| {
        call["tool"] == "engarg_review_contract" && call["evidence_kind"] == "scoped_review"
    }));
    assert!(ids(&plan["dependencies"]["claims"]).contains(current.as_str()));
}

#[tokio::test]
#[ignore = "requires a real EAL installation; set ENGARG_EAL_HOST"]
async fn absent_observations_remain_unsupported_and_assessments_survive_host_restarts() {
    let (_workspace, config) = host();
    let (mut request, case) = fixtures();
    // Even a supplied checked flag is authored metadata, not a retained EAL observation.
    request.evidence[0].checked_by_user = true;
    let bundle = export(&request, &case).unwrap();
    let current = &bundle.claim_names["current_claim"];
    let assessment = result(
        &config,
        json!({"operation":"reason", "source":bundle.source,
               "context":bundle.context, "now":NOW}),
    )
    .await;
    assert_eq!(assessment["valid"], true, "{assessment}");
    assert_eq!(assessment["assessed_at"], NOW);
    assert_eq!(assessment["collection_id"], Value::Null);
    assert_eq!(
        assessment["environments"]["engarg_case"]["status"],
        "matched"
    );
    let evidence = assessment["evidence"].as_object().unwrap();
    assert_eq!(evidence.len(), bundle.obligations.len());
    for (id, item) in evidence {
        assert_eq!(item["status"], "unavailable", "{id}: {item}");
        assert!(ids(&item["availability_issues"]).contains("missing_observation"));
    }
    for (id, claim) in assessment["claims"].as_object().unwrap() {
        assert_eq!(claim["status"], "unsupported", "{id}: {claim}");
        assert_eq!(claim["prose_verified"], false);
    }
    assert_eq!(assessment["claims"][current]["status"], "unsupported");
    let assessment_id = assessment["assessment_id"].as_str().unwrap();
    assert!(!assessment_id.is_empty());

    // Each invocation starts another text host and MCP server against the same database.
    let explanation = result(
        &config,
        json!({"operation":"explain", "assessment_id":assessment_id}),
    )
    .await;
    assert_eq!(explanation, assessment);
    let claim_explanation = result(
        &config,
        json!({"operation":"explain", "assessment_id":assessment_id, "claim":current}),
    )
    .await;
    assert_eq!(claim_explanation["result"], assessment["claims"][current]);
    let packet = result(
        &config,
        json!({"operation":"packet", "assessment_id":assessment_id, "claim":current,
               "packet_limits":{"max_bytes":65536,"max_statement_bytes":4096}}),
    )
    .await;
    assert_eq!(packet["schema"], "EAL/assessment-packet/2");
    assert_eq!(packet["assessment_id"], assessment_id);
    assert_eq!(packet["source_digest"], assessment["source_digest"]);
    assert_eq!(
        packet["context_fingerprint"],
        assessment["context_fingerprint"]
    );
    assert_eq!(packet["claims"][current]["status"], "unsupported");
    assert_eq!(packet["claims"][current]["prose_verified"], false);
    assert_eq!(packet["full_explanation"]["operation"], "explain");
    assert_eq!(packet["full_explanation"]["assessment_id"], assessment_id);
}

#[tokio::test]
#[ignore = "requires a real EAL installation; set ENGARG_EAL_HOST"]
async fn semantic_validation_failure_is_preserved_inside_a_successful_host_envelope() {
    let (_workspace, config) = host();
    let (request, case) = fixtures();
    let bundle = export(&request, &case).unwrap();
    let conclusion = format!("=> {}", bundle.claim_names["current_claim"]);
    assert!(bundle.source.contains(&conclusion));
    let invalid = bundle
        .source
        .replace(&conclusion, "=> undeclared_live_claim");
    let validation = result(&config, json!({"operation":"validate", "source":invalid})).await;
    assert_eq!(validation["valid"], false);
    assert!(
        validation["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| {
                diagnostic["code"] == "unknown_reference"
                    && diagnostic["actual"] == "undeclared_live_claim"
            })
    );
}

#[tokio::test]
#[ignore = "requires a real EAL installation; set ENGARG_EAL_HOST"]
async fn upstream_planning_follows_joint_cumulative_and_selected_alternative_routes() {
    for relationship in [
        SupportRelationship::JointlyRequired,
        SupportRelationship::Cumulative,
        SupportRelationship::AlternativeRoutes,
    ] {
        let (_workspace, config) = host();
        let (request, mut case) = fixtures();
        let mut candidate = case
            .warrants
            .iter()
            .find(|warrant| warrant.conclusion_id == "current_claim")
            .unwrap()
            .clone();
        candidate.id = "W-live-alternative".into();
        case.warrants.push(candidate);
        let composition = case
            .compositions
            .iter_mut()
            .find(|composition| composition.conclusion_id == "current_claim")
            .unwrap();
        composition.relationship = relationship;
        composition.selected_warrant_ids = vec!["W2".into()];
        if relationship != SupportRelationship::AlternativeRoutes {
            composition
                .selected_warrant_ids
                .push("W-live-alternative".into());
        }
        let bundle = export(&request, &case).unwrap();
        let validation = result(
            &config,
            json!({"operation":"validate", "source":bundle.source}),
        )
        .await;
        assert_eq!(validation["valid"], true, "{relationship:?}: {validation}");
        let plan = result(
            &config,
            json!({"operation":"plan", "source":bundle.source,
                   "claim":bundle.claim_names["current_claim"]}),
        )
        .await;
        let claims = ids(&plan["dependencies"]["claims"]);
        assert!(claims.contains(declaration(&bundle, "W2", "candidate_route")));
        let alternative = declaration(&bundle, "W-live-alternative", "candidate_route");
        assert_eq!(
            claims.contains(alternative),
            relationship != SupportRelationship::AlternativeRoutes
        );
        let alternative_review = bundle
            .obligations
            .iter()
            .find(|obligation| {
                obligation.target_id == "W-live-alternative"
                    && obligation.kind == "warrant_adequacy"
            })
            .unwrap();
        assert_eq!(
            ids(&plan["evidence_ids"]).contains(alternative_review.eal_evidence_name.as_str()),
            relationship != SupportRelationship::AlternativeRoutes
        );
        // The unused alternative remains a declared, queryable claim.
        assert!(bundle.source.contains(&format!("claim {alternative} {{")));
    }
}
