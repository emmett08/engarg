//! Synchronous specialist rounds with graph-limited peer context and a complete dissent ledger.

use std::{
    collections::BTreeSet,
    future::{Future, poll_fn},
    task::Poll,
};

use serde_json::{Value, json};

use super::{CallRecord, RunConfig, check_prompt, context, execute, parse_assessment};
use crate::{
    Error,
    domain::Request,
    output_validation::RetainedObjection,
    provider::{CompletionRequest, LanguageModel, Role},
};

const SPECIALISTS: [Role; 3] = [Role::Constructor, Role::Sceptic, Role::EvidenceAuditor];
const LEGACY_PREFIXES: [&str; 3] = ["constructor", "sceptic", "auditor"];
const PEER_INSTRUCTIONS: &str = "PEER EXCHANGE: This is a dependent discussion round. Use your previous_assessment and only the peer_assessments supplied in this message to reconsider your findings, objections and evidence gaps. Peer assessments identify their participant and source round. Their objection_ids resolve to the exact retained_objections entries in this message. Return the same three-list assessment schema. State every concern that remains material in full, including concerns raised earlier. Agreement and repeated assertions supply no independent evidence. The final synthesis retains every earlier objection even if a later assessment omits it.";

pub(super) struct Assessments {
    pub(super) transcript: Vec<CallRecord>,
    pub(super) retained: Vec<RetainedObjection>,
    pub(super) independent: Vec<Value>,
    pub(super) discussion: Vec<Value>,
}

/// Run independent first passes, followed by any configured peer exchanges.
/// Prompt creation is injected so flat and support-graph planning share routing,
/// concurrency, participant identity and dissent handling without sharing schemas.
pub(super) async fn assess<M, O, F>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    make_prompt: F,
) -> Result<Assessments, Error>
where
    M: LanguageModel,
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
    F: Fn(Role, Value) -> Result<CompletionRequest, Error> + Sync,
{
    let graphs = config
        .debate
        .as_ref()
        .map(|debate| debate.graphs())
        .transpose()?;
    let participants = config
        .debate
        .as_ref()
        .map_or(SPECIALISTS.len(), |debate| usize::from(debate.participants));
    let prompts = (0..participants)
        .map(|participant| {
            let mut data = json!({"request": request});
            if graphs.is_some() {
                data["debate"] = json!({"participant": participant, "round": 0});
            }
            make_prompt(role(participant), data)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let records = execute_round(model, prompts, config, observer).await?;
    let mut result = Assessments {
        transcript: Vec::new(),
        retained: Vec::new(),
        independent: Vec::new(),
        discussion: Vec::new(),
    };
    for (participant, record) in records.into_iter().enumerate() {
        let prefix = if graphs.is_some() {
            format!("agent-{participant}-round-0")
        } else {
            LEGACY_PREFIXES[participant].to_owned()
        };
        let mut assessment = context::assessment(
            record.role,
            &prefix,
            parse_assessment(&record)?,
            &mut result.retained,
        );
        if graphs.is_some() {
            identify(&mut assessment, participant, 0);
        }
        result.independent.push(assessment);
        result.transcript.push(record);
    }
    let Some(graphs) = graphs else {
        return Ok(result);
    };
    let mut previous = result.independent.clone();
    for (index, graph) in graphs.into_iter().enumerate() {
        let round = index + 1;
        let prompts = (0..participants)
            .map(|participant| {
                let incoming = graph
                    .edges
                    .iter()
                    .filter(|edge| edge.to == participant)
                    .map(|edge| edge.from)
                    .collect::<Vec<_>>();
                let peers = incoming
                    .iter()
                    .map(|&source| previous[source].clone())
                    .collect::<Vec<_>>();
                let visible_ids = std::iter::once(&previous[participant])
                    .chain(peers.iter())
                    .flat_map(objection_ids)
                    .collect::<BTreeSet<_>>();
                let visible_objections = result
                    .retained
                    .iter()
                    .filter(|objection| visible_ids.contains(objection.id.as_str()))
                    .collect::<Vec<_>>();
                let data = json!({
                    "request": request,
                    "debate": {"participant": participant, "round": round, "edges": graph.edges, "incoming": incoming},
                    "previous_assessment": previous[participant],
                    "peer_assessments": peers,
                    "retained_objections": visible_objections,
                });
                let mut prompt = make_prompt(role(participant), data)?;
                dependent_instructions(&mut prompt.system);
                Ok(prompt)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let records = execute_round(model, prompts, config, observer).await?;
        let mut next = Vec::with_capacity(participants);
        for (participant, record) in records.into_iter().enumerate() {
            let mut assessment = context::assessment(
                record.role,
                &format!("agent-{participant}-round-{round}"),
                parse_assessment(&record)?,
                &mut result.retained,
            );
            identify(&mut assessment, participant, round);
            next.push(assessment);
            result.transcript.push(record);
        }
        result.discussion.extend(next.iter().cloned());
        previous = next;
    }
    Ok(result)
}

/// Add graph discussion only when configured, preserving legacy prompt envelopes.
pub(super) fn with_discussion(mut data: Value, discussion: &[Value]) -> Value {
    if !discussion.is_empty() {
        data["discussion_assessments"] = json!(discussion);
    }
    data
}

fn role(participant: usize) -> Role {
    SPECIALISTS[participant % SPECIALISTS.len()]
}

fn identify(assessment: &mut Value, participant: usize, round: usize) {
    assessment["participant"] = json!(participant);
    assessment["round"] = json!(round);
}

fn objection_ids(assessment: &Value) -> impl Iterator<Item = &str> {
    assessment["assessment"]["objection_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn dependent_instructions(system: &mut String) {
    system.push_str("\n\n");
    system.push_str(PEER_INSTRUCTIONS);
}

/// Poll every bounded round member to completion, including siblings of a failed
/// call. Observers retain their receipts; results retain participant order.
async fn execute_round<M, O>(
    model: &M,
    prompts: Vec<CompletionRequest>,
    config: &RunConfig,
    observer: &O,
) -> Result<Vec<CallRecord>, Error>
where
    M: LanguageModel,
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    for prompt in &prompts {
        check_prompt(prompt, config)?;
    }
    let mut pending = prompts
        .into_iter()
        .map(|prompt| Some(Box::pin(execute(model, prompt, config, observer))))
        .collect::<Vec<_>>();
    let mut completed = std::iter::repeat_with(|| None)
        .take(pending.len())
        .collect::<Vec<_>>();
    poll_fn(|cx| {
        for (pending, completed) in pending.iter_mut().zip(&mut completed) {
            if let Some(future) = pending
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                *completed = Some(result);
                *pending = None;
            }
        }
        if pending.iter().all(Option::is_none) {
            Poll::Ready(
                completed
                    .iter_mut()
                    .map(|result| result.take().expect("every round member completed"))
                    .collect(),
            )
        } else {
            Poll::Pending
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        engine::{
            assessment_schema,
            topology::{Adaptation, DebateConfig, Edge, Topology},
        },
        provider::Completion,
    };

    struct Model {
        prompts: Mutex<Vec<CompletionRequest>>,
        completed: Mutex<Vec<(usize, usize)>>,
        participants: usize,
        fail: Option<(usize, usize)>,
        malformed: Option<(usize, usize)>,
    }

    impl Model {
        fn new(participants: usize) -> Self {
            Self {
                prompts: Mutex::new(Vec::new()),
                completed: Mutex::new(Vec::new()),
                participants,
                fail: None,
                malformed: None,
            }
        }
    }

    impl LanguageModel for Model {
        async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
            let data: Value = serde_json::from_str(&request.user).unwrap();
            let participant = data["debate"]["participant"].as_u64().map_or_else(
                || {
                    SPECIALISTS
                        .iter()
                        .position(|role| *role == request.role)
                        .unwrap()
                },
                |value| usize::try_from(value).unwrap(),
            );
            let round = data["debate"]["round"].as_u64().unwrap_or(0) as usize;
            if round > 0 {
                assert_eq!(
                    self.completed
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(_, r)| *r == round - 1)
                        .count(),
                    self.participants,
                    "dependent round started before its predecessor completed",
                );
            }
            self.prompts.lock().unwrap().push(request);
            // Every member is polled; force reverse completion deterministically.
            while self
                .completed
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, r)| *r == round)
                .count()
                < self.participants - participant - 1
            {
                tokio::task::yield_now().await;
            }
            self.completed.lock().unwrap().push((participant, round));
            if self.fail == Some((participant, round)) {
                return Err(Error::InvalidOutput("fixture provider failure".into()));
            }
            let content = if self.malformed == Some((participant, round)) {
                json!({"findings": [], "objections": ["   "], "evidence_gaps": []})
            } else {
                json!({
                    "findings": [format!("finding-{participant}-{round}")],
                    "objections": [format!("challenge-{participant}-{round}")],
                    "evidence_gaps": [],
                })
            };
            Ok(Completion {
                content: content.to_string(),
                model: "debate-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            })
        }
    }

    fn config() -> RunConfig {
        RunConfig {
            debate: Some(DebateConfig {
                participants: 4,
                rounds: 2,
                topology: Topology::Ring {},
                adaptation: Default::default(),
            }),
            max_calls: 15,
            ..RunConfig::default()
        }
    }

    fn make_prompt(role: Role, data: Value) -> Result<CompletionRequest, Error> {
        Ok(CompletionRequest {
            role,
            system: if data["debate"]["round"]
                .as_u64()
                .is_some_and(|round| round > 0)
            {
                crate::prompts::discussion_system(role)
            } else {
                crate::prompts::system(role)
            },
            user: data.to_string(),
            schema: assessment_schema(),
        })
    }

    fn request() -> Request {
        serde_json::from_value(json!({"claim": "Test the graph communication contract."})).unwrap()
    }

    #[tokio::test]
    async fn ring_prompts_expose_only_own_and_incoming_previous_assessments() {
        let model = Model::new(4);
        let result = assess(&model, &request(), &config(), &|_| Ok(()), make_prompt)
            .await
            .unwrap();
        assert_eq!(result.transcript.len(), 12);
        assert_eq!(result.independent.len(), 4);
        assert_eq!(result.discussion.len(), 8);
        assert_eq!(result.retained.len(), 12);
        for (index, record) in result.transcript.iter().enumerate() {
            let data: Value = serde_json::from_str(&record.prompt.user).unwrap();
            let participant = index % 4;
            let round = index / 4;
            assert_eq!(data["debate"]["participant"], participant);
            assert_eq!(data["debate"]["round"], round);
            if round == 0 {
                assert!(data.get("peer_assessments").is_none());
                assert!(data.get("retained_objections").is_none());
                continue;
            }
            let source = (participant + 3) % 4;
            let peers = data["peer_assessments"].as_array().unwrap();
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0]["participant"], source);
            assert_eq!(peers[0]["round"], round - 1);
            assert_eq!(data["previous_assessment"]["participant"], participant);
            assert_eq!(data["previous_assessment"]["round"], round - 1);
            let mut challenges = data["retained_objections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value["challenge"].as_str().unwrap())
                .collect::<Vec<_>>();
            challenges.sort_unstable();
            let mut expected = vec![
                format!("challenge-{source}-{}", round - 1),
                format!("challenge-{participant}-{}", round - 1),
            ];
            expected.sort_unstable();
            assert_eq!(challenges, expected);
            assert!(!record.prompt.system.contains("You cannot see"));
            assert!(!record.prompt.system.contains("Independently construct"));
            assert!(record.prompt.system.contains("PEER EXCHANGE"));
        }
        assert_ne!(model.completed.lock().unwrap()[0].0, 0);
        let ids = result
            .retained
            .iter()
            .map(|objection| &objection.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), 12);
    }

    #[tokio::test]
    async fn a_scheduled_replacement_changes_peer_visibility_at_the_next_round() {
        let model = Model::new(4);
        let mut config = config();
        config.debate.as_mut().unwrap().adaptation = Adaptation::Scheduled {
            round_edges: vec![vec![Edge { from: 0, to: 2 }]],
        };
        let result = assess(&model, &request(), &config, &|_| Ok(()), make_prompt)
            .await
            .unwrap();
        for record in &result.transcript[8..12] {
            let data: Value = serde_json::from_str(&record.prompt.user).unwrap();
            let participant = data["debate"]["participant"].as_u64().unwrap() as usize;
            let peers = data["peer_assessments"].as_array().unwrap();
            assert_eq!(peers.len(), usize::from(participant == 2));
            if participant == 2 {
                assert_eq!(peers[0]["participant"], 0);
                assert_eq!(peers[0]["round"], 1);
            }
            assert_eq!(
                data["retained_objections"].as_array().unwrap().len(),
                1 + usize::from(participant == 2),
            );
            assert_eq!(data["debate"]["edges"], json!([{"from": 0, "to": 2}]));
        }
        // Earlier assessments and dissent survive even though later peer visibility changes.
        assert_eq!(result.independent.len(), 4);
        assert_eq!(result.discussion.len(), 8);
        assert_eq!(result.retained.len(), 12);
    }

    #[tokio::test]
    async fn legacy_round_keeps_original_payload_prefixes_and_role_order() {
        let model = Model::new(3);
        let result = assess(
            &model,
            &request(),
            &RunConfig::default(),
            &|_| Ok(()),
            make_prompt,
        )
        .await
        .unwrap();
        assert!(result.discussion.is_empty());
        assert_eq!(result.transcript.len(), 3);
        for (index, record) in result.transcript.iter().enumerate() {
            assert_eq!(record.role, SPECIALISTS[index]);
            let data: Value = serde_json::from_str(&record.prompt.user).unwrap();
            assert_eq!(data.as_object().unwrap().len(), 1);
            assert!(data.get("request").is_some());
            assert_eq!(
                result.retained[index].id,
                format!("{}-1", LEGACY_PREFIXES[index])
            );
            assert!(result.independent[index].get("participant").is_none());
        }
    }

    #[tokio::test]
    async fn every_round_prompt_is_checked_before_any_provider_call() {
        let model = Model::new(4);
        let config = config();
        let error = assess(&model, &request(), &config, &|_| Ok(()), |role, data| {
            let oversized = data["debate"]["participant"] == 3;
            let mut prompt = make_prompt(role, data)?;
            if oversized {
                prompt.user = "x".repeat(config.max_input_bytes);
            }
            Ok(prompt)
        })
        .await
        .err()
        .unwrap();
        assert!(matches!(error, Error::Budget(_)));
        assert!(model.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_member_still_allows_sibling_completion_receipts() {
        let mut model = Model::new(4);
        model.fail = Some((3, 0));
        let receipts = Mutex::new(Vec::new());
        let error = assess(
            &model,
            &request(),
            &config(),
            &|record| {
                receipts.lock().unwrap().push(record.role);
                Ok(())
            },
            make_prompt,
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(error, Error::InvalidOutput(_)));
        assert_eq!(model.completed.lock().unwrap().len(), 4);
        assert_eq!(receipts.lock().unwrap().len(), 3);
        assert_eq!(model.prompts.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn malformed_peer_assessment_stops_following_round_after_all_current_receipts() {
        let mut model = Model::new(4);
        model.malformed = Some((0, 1));
        let receipts = Mutex::new(Vec::new());
        let error = assess(
            &model,
            &request(),
            &config(),
            &|record| {
                receipts.lock().unwrap().push(record.role);
                Ok(())
            },
            make_prompt,
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(error, Error::InvalidOutput(_)));
        assert_eq!(model.completed.lock().unwrap().len(), 8);
        assert_eq!(receipts.lock().unwrap().len(), 8);
        assert_eq!(model.prompts.lock().unwrap().len(), 8);
    }

    #[tokio::test]
    async fn observer_failure_does_not_cancel_sibling_receipts_or_start_the_next_round() {
        let model = Model::new(4);
        let receipts = Mutex::new(Vec::new());
        let error = assess(
            &model,
            &request(),
            &config(),
            &|record| {
                receipts.lock().unwrap().push(record.role);
                Err(Error::InvalidOutput("fixture observer failure".into()))
            },
            make_prompt,
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(error, Error::InvalidOutput(_)));
        assert_eq!(model.completed.lock().unwrap().len(), 4);
        assert_eq!(receipts.lock().unwrap().len(), 4);
        assert_eq!(model.prompts.lock().unwrap().len(), 4);
    }

    #[test]
    fn discussion_is_added_only_for_configured_graph_runs() {
        let original = json!({"request": "unchanged"});
        assert_eq!(with_discussion(original.clone(), &[]), original);
        assert_eq!(
            with_discussion(original, &[json!({"round": 1})])["discussion_assessments"],
            json!([{"round": 1}])
        );
    }
}
