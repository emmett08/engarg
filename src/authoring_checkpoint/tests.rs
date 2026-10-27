use super::*;
use crate::provider::{Completion, CompletionRequest, Role};
fn checkpoint() -> Checkpoint {
    Checkpoint::new(
        serde_json::from_str(include_str!("../../examples/request.json")).unwrap(),
        RunConfig::default(),
        json!({}),
        "fixture-config".into(),
    )
}
fn call(checkpoint: &Checkpoint, role: Role) -> CallRecord {
    let content = "{}";
    CallRecord {
        role,
        prompt: CompletionRequest {
            role,
            system: "fixture".into(),
            user: json!({"request":checkpoint.request}).to_string(),
            schema: json!({}),
        },
        response: Completion {
            content: content.into(),
            model: "fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        },
        response_content_bytes: content.len(),
        response_content_sha256: digest(content),
        response_content_omitted: false,
        elapsed_ms: 0,
    }
}

#[test]
fn completion_failure_preserves_in_memory_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoint.json");
    let previous = directory.path().join("previous.json");
    let mut initial = checkpoint();
    initial.config.mode = Mode::Single;
    let exchange = call(&initial, Role::Single);
    let store = Store::create(&path, initial).unwrap();
    store.completion(&exchange).unwrap();
    let before = std::fs::read(&path).unwrap();
    std::fs::rename(&path, &previous).unwrap();
    std::fs::create_dir(&path).unwrap();
    // The destination directory makes publication fail after staging, so a
    // retry observes exactly the last durable call and failure state.
    assert!(store.completion(&exchange).is_err());
    assert!(
        store
            .failure(&Error::InvalidOutput("failed update".into()))
            .is_err()
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&previous, &path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    store.completion(&exchange).unwrap();
    let saved = Checkpoint::read(&path).unwrap();
    assert_eq!(saved.transcript.len(), 2);
    assert!(saved.failure.is_none());
}
#[test]
fn atomic_private_checkpoint_preserves_completed_exchange_before_parsing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoint.json");
    let original = checkpoint();
    let store = Store::create(&path, original.clone()).unwrap();
    let record = call(&original, Role::Constructor);
    store.completion(&record).unwrap();
    store
        .failure(&Error::InvalidOutput("later parse failure".into()))
        .unwrap();
    let saved = Checkpoint::read(&path).unwrap();
    assert_eq!(saved.transcript[0].response.content, "{}");
    assert!(saved.failure.unwrap().contains("later parse failure"));
    assert_eq!(directory.path().read_dir().unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(Store::create(&path, original).is_err());
}
#[test]
fn replays_specialists_in_workflow_order_and_rejects_duplicate_roles() {
    let mut saved = checkpoint();
    saved.transcript = vec![
        call(&saved, Role::EvidenceAuditor),
        call(&saved, Role::Constructor),
        call(&saved, Role::Sceptic),
    ];
    saved.validate().unwrap();
    assert_eq!(
        saved
            .ordered_transcript()
            .unwrap()
            .iter()
            .map(|call| call.role)
            .collect::<Vec<_>>(),
        vec![Role::Constructor, Role::Sceptic, Role::EvidenceAuditor]
    );
    saved.transcript[2] = call(&saved, Role::Constructor);
    assert!(
        saved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
}
#[test]
fn rejects_mutated_response_request_and_duplicate_json_keys() {
    let mut saved = checkpoint();
    saved.transcript.push(call(&saved, Role::Constructor));
    saved.transcript[0].response.content = "tampered".into();
    assert!(saved.validate().unwrap_err().to_string().contains("digest"));
    saved.transcript[0] = call(&saved, Role::Constructor);
    saved.transcript[0].prompt.user = "{\"request\":{}}".into();
    assert!(
        saved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("request")
    );
    assert!(exact_json(br#"{"outer":{"x":1,"x":2}}"#).is_err());
}

#[test]
fn rejects_skipped_final_stage_and_duplicate_prompt_metadata() {
    let mut saved = checkpoint();
    saved.transcript = vec![
        call(&saved, Role::Constructor),
        call(&saved, Role::Sceptic),
        call(&saved, Role::EvidenceAuditor),
        call(&saved, Role::Adjudicator),
    ];
    assert!(
        saved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("stage order")
    );
    saved.transcript = vec![call(&saved, Role::Constructor)];
    saved.transcript[0].prompt.user = format!(
        "{{\"request\":{},\"metadata\":{{\"x\":1,\"x\":2}}}}",
        serde_json::to_string(&saved.request).unwrap()
    );
    assert!(saved.validate().is_err());
}

#[test]
fn compact_parent_recovers_original_parallel_arrival_order_without_copying_calls() {
    let mut saved = checkpoint();
    saved.transcript = vec![
        call(&saved, Role::EvidenceAuditor),
        call(&saved, Role::Constructor),
        call(&saved, Role::Sceptic),
    ];
    let (mut metadata, order) = saved.compact_parent().unwrap();
    assert!(metadata.transcript.is_empty());
    assert_eq!(order, vec![2, 0, 1]);
    let ordered = saved.ordered_transcript().unwrap();
    metadata.transcript = order.iter().map(|index| ordered[*index].clone()).collect();
    assert_eq!(
        digest(serde_json::to_vec(&metadata).unwrap()),
        digest(serde_json::to_vec(&saved).unwrap())
    );
}
#[test]
fn accepted_report_bytes_preserve_the_existing_consumer_limit() {
    let oversized = json!({"retained_history":"x".repeat(MAX_REPORT_BYTES)});
    assert!(
        admitted_report_bytes(&oversized)
            .unwrap_err()
            .to_string()
            .contains("16 MiB consumer limit")
    );
    assert_eq!(admitted_report_bytes(&json!({})).unwrap(), b"{}\n");
}

fn routed_checkpoint() -> Checkpoint {
    use crate::engine::topology::{DebateConfig, Topology};
    let mut saved = checkpoint();
    saved.config.debate = Some(DebateConfig {
        participants: 4,
        rounds: 1,
        topology: Topology::preset("ring", 4).unwrap(),
        adaptation: Default::default(),
    });
    saved.config.max_calls = 11;
    let roles = [Role::Constructor, Role::Sceptic, Role::EvidenceAuditor];
    for (round, participants) in [[3, 2, 0, 1], [1, 3, 2, 0]].into_iter().enumerate() {
        for participant in participants {
            let mut record = call(&saved, roles[participant % roles.len()]);
            record.prompt.user =
                json!({"request":saved.request,"debate":{"participant":participant,"round":round}})
                    .to_string();
            saved.transcript.push(record);
        }
    }
    for role in [Role::Synthesiser, Role::FinalCritic, Role::Adjudicator] {
        saved.transcript.push(call(&saved, role));
    }
    saved
}

#[test]
fn routed_rounds_preserve_each_participant_identity_and_parallel_arrival_order() {
    let saved = routed_checkpoint();
    saved.validate().unwrap();
    let ordered = saved.ordered_transcript().unwrap();
    for (index, record) in ordered[..8].iter().enumerate() {
        let prompt = exact_json(record.prompt.user.as_bytes()).unwrap();
        assert_eq!(prompt["debate"]["participant"], index % 4);
        assert_eq!(prompt["debate"]["round"], index / 4);
    }
    assert_eq!(ordered[8].role, Role::Synthesiser);
    let (mut metadata, order) = saved.compact_parent().unwrap();
    assert_eq!(order, vec![3, 2, 0, 1, 5, 7, 6, 4, 8, 9, 10]);
    metadata.transcript = order.iter().map(|index| ordered[*index].clone()).collect();
    assert_eq!(
        digest(serde_json::to_vec(&metadata).unwrap()),
        digest(serde_json::to_vec(&saved).unwrap())
    );
    assert_eq!(saved.inspection()["resumable"], true);
    for incomplete in [6, 8, 9, 10] {
        let mut partial = saved.clone();
        partial.transcript.truncate(incomplete);
        partial.validate().unwrap();
        assert_eq!(partial.inspection()["resumable"], false);
    }
}

#[test]
fn routed_checkpoint_rejects_duplicate_cross_round_and_wrong_role_calls() {
    let saved = routed_checkpoint();
    let mut duplicate = saved.clone();
    duplicate.transcript[1] = duplicate.transcript[0].clone();
    assert!(
        duplicate
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    let mut crossed = saved.clone();
    crossed.transcript.swap(0, 4);
    assert!(
        crossed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("round")
    );
    let mut wrong_role = saved.clone();
    wrong_role.transcript[0].role = Role::Sceptic;
    wrong_role.transcript[0].prompt.role = Role::Sceptic;
    assert!(
        wrong_role
            .validate()
            .unwrap_err()
            .to_string()
            .contains("role")
    );
    let mut skipped = saved;
    skipped.transcript.remove(0);
    assert!(skipped.validate().is_err());
}
