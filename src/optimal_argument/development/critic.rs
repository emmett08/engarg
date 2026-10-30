//! Admit closed, bounded sceptic findings without accepting model-owned resolutions.
use super::{ArtifactReference, CriticalIssue, Criticism};
use crate::Error;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

fn bounded(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum
}

fn references(values: &[String]) -> bool {
    values.len() <= 32
        && values.iter().all(|value| bounded(value, 128))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

pub(super) fn valid_artifact(artifact: &ArtifactReference) -> bool {
    bounded(&artifact.path, 2048)
        && !artifact.path.contains('\\')
        && !artifact.path.chars().any(char::is_control)
        && Path::new(&artifact.path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && artifact.pointer.as_ref().is_none_or(|pointer| {
            pointer.len() <= 2048 && eal_collector_contracts::execution::valid_pointer(pointer)
        })
        && artifact
            .sha256
            .as_deref()
            .is_none_or(eal_collector_contracts::execution::valid_digest)
}

fn valid_issue(issue: &CriticalIssue) -> bool {
    bounded(&issue.id, 128)
        && bounded(&issue.target_id, 128)
        && bounded(&issue.description, 4096)
        && references(&issue.obligation_ids)
        && references(&issue.check_ids)
        && issue.artifacts.len() <= 16
        && issue.artifacts.iter().all(valid_artifact)
}

pub(super) fn schema() -> Value {
    let reference = json!({"type":"string","minLength":1,"maxLength":128});
    let references = json!({"type":"array","maxItems":32,"items":reference});
    let artifact = json!({"type":"object","additionalProperties":false,
        "properties":{
            "path":{"type":"string","minLength":1,"maxLength":2048},
            "pointer":{"type":["string","null"],"maxLength":2048},
            "sha256":{"type":["string","null"],"pattern":"^[0-9a-f]{64}$"}
        },"required":["path","pointer","sha256"]});
    let issue = json!({"type":"object","additionalProperties":false,
        "properties":{
            "id":reference,"target_id":reference,
            "kind":{"type":"string","enum":["missing_evidence","missing_verification","lifecycle_conflict","binding_mismatch","semantic","other"]},
            "description":{"type":"string","minLength":1,"maxLength":4096},
            "obligation_ids":references,"check_ids":references,
            "artifacts":{"type":"array","maxItems":16,"items":artifact}
        },"required":["id","kind","target_id","description","obligation_ids","check_ids","artifacts"]});
    json!({"type":"object","additionalProperties":false,
        "properties":{
            "critical_issues":{"type":"array","maxItems":32,"items":issue},
            "observations":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":4096}}
        },"required":["critical_issues","observations"]})
}

pub(super) fn parse(content: &str, maximum: usize) -> Result<Criticism, Error> {
    let value = eal_collector_contracts::strict_json_exact(
        content.as_bytes(),
        eal_collector_contracts::JsonLimits {
            max_bytes: maximum,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidOutput)?;
    if value["critical_issues"]
        .as_array()
        .is_some_and(|issues| issues.iter().any(|issue| !issue.is_object()))
    {
        return Err(Error::InvalidOutput("live sceptic responses require typed critical issues; legacy strings are accepted only when reading retained records".into()));
    }
    let criticism: Criticism =
        serde_json::from_value(value).map_err(|error| Error::InvalidOutput(error.to_string()))?;
    if criticism.critical_issues.len() > 32
        || !criticism.critical_issues.iter().all(valid_issue)
        || criticism.observations.len() > 32
        || !criticism
            .observations
            .iter()
            .all(|value| bounded(value, 4096))
    {
        return Err(Error::InvalidOutput(
            "sceptic findings exceed the closed typed-reference and text bounds".into(),
        ));
    }
    Ok(criticism)
}
