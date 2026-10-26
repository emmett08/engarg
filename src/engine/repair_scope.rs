//! Limit a dissent-only correction to the affected targets and their dispositions.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::{
    Error,
    domain::ArgumentCase,
    output_validation::{self, RetainedObjection},
};

/// Both authoring modes enforce the same boundary once a correction is scoped.
pub(super) fn constrain(
    active: &mut Option<RepairScope>,
    case: &ArgumentCase,
    retained: &[RetainedObjection],
    diagnostics: &mut Vec<String>,
    auxiliary: Value,
) -> Result<Value, Error> {
    let candidate = serde_json::to_value(case)?;
    if active.is_none() {
        let contexts = output_validation::dissent_changes(case, retained);
        *active = RepairScope::for_contexts(
            &candidate,
            contexts.iter().map(|change| {
                (
                    change.target_id.as_str(),
                    &change.original_context,
                    &change.current_context,
                )
            }),
            diagnostics,
        );
        if let Some(scope) = active {
            scope.protect_auxiliary(auxiliary.clone());
        }
    }
    let Some(scope) = active else {
        return Ok(Value::Null);
    };
    diagnostics.extend(scope.errors(&candidate));
    let auxiliary_errors = scope.auxiliary_errors(&auxiliary);
    let mut feedback = scope.instructions(&candidate);
    if !auxiliary_errors.is_empty() {
        feedback["protected_auxiliary"] = scope.auxiliary().cloned().unwrap_or(Value::Null);
    }
    diagnostics.extend(auxiliary_errors);
    Ok(feedback)
}

/// An exact candidate with a small, explicit set of editable JSON properties.
/// General structural errors retain the ordinary fully validated repair path.
pub(super) struct RepairScope {
    baseline: Value,
    frozen: Value,
    editable: BTreeSet<String>,
    auxiliary: Option<Value>,
}

impl RepairScope {
    pub(super) fn for_contexts<'a>(
        candidate: &Value,
        contexts: impl IntoIterator<Item = (&'a str, &'a Value, &'a Value)>,
        diagnostics: &[String],
    ) -> Option<Self> {
        if diagnostics.is_empty()
            || !diagnostics
                .iter()
                .all(|message| message.starts_with("DISSENT_TARGET_CHANGED"))
        {
            return None;
        }
        // Reopening such an objection may need new support requirements as well
        // as its evidence plan. Leave that coordinated change to general repair.
        if candidate["defeaters"].as_array()?.iter().any(|defeater| {
            defeater["status"] == "inapplicable_after_revision"
                && defeater["material"] == true
                && !candidate["evidence_plan"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|need| need["target_id"] == defeater["id"])
        }) {
            return None;
        }
        let mut editable = BTreeSet::from([
            "/dissent_revisions".into(),
            "/reading".into(),
            "/adjudication".into(),
            "/status".into(),
            "/residual_risks".into(),
            "/decision_limits".into(),
        ]);
        let mut affected_targets = BTreeSet::new();
        for (target_id, before, after) in contexts {
            if before == after
                && !candidate["defeaters"].as_array()?.iter().any(|defeater| {
                    defeater["target_id"] == target_id
                        && defeater["status"] == "inapplicable_after_revision"
                })
            {
                continue;
            }
            affected_targets.insert(target_id.to_owned());
            changed_paths(&before["scope"], &after["scope"], "/scope", &mut editable);
            if let Some(path) = node_path(candidate, target_id) {
                changed_paths(&before["target"], &after["target"], &path, &mut editable);
            }
            if let Some(related) = before["related_targets"].as_object() {
                for (id, original) in related {
                    if let Some(path) = node_path(candidate, id) {
                        changed_paths(
                            original,
                            &after["related_targets"][id],
                            &path,
                            &mut editable,
                        );
                    }
                }
            }
        }
        for (index, defeater) in candidate["defeaters"].as_array()?.iter().enumerate() {
            if !defeater["target_id"]
                .as_str()
                .is_some_and(|id| affected_targets.contains(id))
            {
                continue;
            }
            for field in ["status", "response", "evidence_ids"] {
                editable.insert(format!("/defeaters/{index}/{field}"));
            }
        }
        let mut frozen = candidate.clone();
        mask(&mut frozen, &editable);
        Some(Self {
            baseline: candidate.clone(),
            frozen,
            editable,
            auxiliary: None,
        })
    }

    pub(super) fn protect_auxiliary(&mut self, auxiliary: Value) {
        self.auxiliary = Some(auxiliary);
    }

    pub(super) fn instructions(&self, candidate: &Value) -> Value {
        let masked = self.masked(candidate);
        let mut differences = BTreeSet::new();
        changed_paths(&self.frozen, &masked, "", &mut differences);
        let restore: Vec<_> = differences
            .iter()
            .map(|path| {
                json!({
                    "path":path, "present":self.baseline.pointer(path).is_some(),
                    "value":self.baseline.pointer(path),
                })
            })
            .collect();
        json!({
            "schema":"engarg.repair-scope/1",
            "editable_paths":self.editable,
            "protected_restorations":restore,
            "rule":"Return the complete envelope. Preserve every other candidate argument field exactly. Resolve dissent by recording its explicit application or restoring the identified changed fields. Retain every existing evidence-plan item unchanged. Preserve support elaboration and acquisition plans. Disposition changes still require their usual evidence and revision validation."
        })
    }

    pub(super) fn auxiliary_errors(&self, auxiliary: &Value) -> Vec<String> {
        if self
            .auxiliary
            .as_ref()
            .is_some_and(|expected| expected != auxiliary)
        {
            vec!["REPAIR_SCOPE_CHANGED dissent-only repair changed the protected support elaboration or acquisition plan; restore it from repair_scope.protected_auxiliary".into()]
        } else {
            vec![]
        }
    }

    pub(super) fn auxiliary(&self) -> Option<&Value> {
        self.auxiliary.as_ref()
    }

    pub(super) fn errors(&self, candidate: &Value) -> Vec<String> {
        let masked = self.masked(candidate);
        if masked == self.frozen {
            return Vec::new();
        }
        let mut changed = BTreeSet::new();
        changed_paths(&self.frozen, &masked, "", &mut changed);
        vec![format!(
            "REPAIR_SCOPE_CHANGED dissent-only repair changed protected candidate fields {}; restore them from candidate_json before repairing the identified dissent",
            changed.into_iter().take(16).collect::<Vec<_>>().join(", ")
        )]
    }

    fn masked(&self, candidate: &Value) -> Value {
        let mut candidate = candidate.clone();
        mask(&mut candidate, &self.editable);
        candidate
    }
}

fn node_path(candidate: &Value, id: &str) -> Option<String> {
    if matches!(id, "current_claim" | "stronger_target") {
        return Some(format!("/{id}"));
    }
    for collection in [
        "subclaims",
        "grounds",
        "warrants",
        "assumptions",
        "defeaters",
    ] {
        if let Some(index) = candidate[collection]
            .as_array()
            .and_then(|items| items.iter().position(|node| node["id"] == id))
        {
            return Some(format!("/{collection}/{index}"));
        }
    }
    None
}

fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn changed_paths(before: &Value, after: &Value, path: &str, changed: &mut BTreeSet<String>) {
    if before == after {
        return;
    }
    if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
        let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
        for key in keys {
            let next = format!("{path}/{}", escaped(key));
            match (before.get(key), after.get(key)) {
                (Some(before), Some(after)) => changed_paths(before, after, &next, changed),
                _ => {
                    changed.insert(next);
                }
            }
        }
    } else {
        // Arrays are atomic here. Permitting an index edit must never permit
        // insertion/removal to shift the meaning of another protected index.
        changed.insert(path.into());
    }
}

fn mask(candidate: &mut Value, editable: &BTreeSet<String>) {
    for path in editable {
        if let Some((parent, key)) = path.rsplit_once('/')
            && let Some(object) = candidate.pointer_mut(parent).and_then(Value::as_object_mut)
        {
            object.remove(&key.replace("~1", "/").replace("~0", "~"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> Value {
        json!({"scope":{"configuration":"fixed"},"warrants":[
            {"id":"W1","rule":"revised","reasoning_method":{"method":"deductive","query":"p"}},
            {"id":"W2","rule":"unrelated"}],"grounds":[{"id":"G1","statement":"fixed"}],
            "defeaters":[{"id":"D1","target_id":"W1","challenge":"fixed","material":true,"status":"open","response":"pending","evidence_ids":[]}],
            "evidence_plan":[{"id":"N1","target_id":"D1","acceptance_criterion":"fixed"}],
            "reading":[],"status":"conditional","adjudication":"pending"})
    }

    fn scope() -> RepairScope {
        let before = json!({"scope":{"configuration":"fixed"},"target":{"id":"W1","rule":"original","reasoning_method":{"method":"deductive","query":"p"}}});
        let after = json!({"scope":{"configuration":"fixed"},"target":{"id":"W1","rule":"revised","reasoning_method":{"method":"deductive","query":"p"}}});
        RepairScope::for_contexts(
            &case(),
            [("W1", &before, &after)],
            &["DISSENT_TARGET_CHANGED D1".into()],
        )
        .unwrap()
    }

    #[test]
    fn explicit_application_or_exact_target_restoration_preserves_other_fields() {
        let scope = scope();
        let mut repaired = case();
        repaired["dissent_revisions"] =
            json!({"applications":[{"objection_id":"D1","effect":"still_applies"}]});
        repaired["reading"] = json!(["updated explanation"]);
        assert!(scope.errors(&repaired).is_empty());
        repaired["warrants"][0]["rule"] = json!("original");
        assert!(scope.errors(&repaired).is_empty());
    }

    #[test]
    fn unrelated_warrant_method_and_acceptance_drift_are_rejected() {
        for pointer in [
            "/warrants/1/rule",
            "/warrants/0/reasoning_method/query",
            "/grounds/0/statement",
            "/evidence_plan/0/acceptance_criterion",
            "/defeaters/0/challenge",
            "/defeaters/0/material",
        ] {
            let mut repaired = case();
            *repaired.pointer_mut(pointer).unwrap() = json!("changed");
            assert!(!scope().errors(&repaired).is_empty(), "{pointer}");
        }
    }

    #[test]
    fn related_warrant_changes_map_to_the_same_node_and_only_the_changed_leaf() {
        let before = json!({"target":{},"related_targets":{"W1":{"reasoning_method":{"method":"abductive","query":"p"}}}});
        let after = json!({"target":{},"related_targets":{"W1":{"reasoning_method":{"method":"deductive","query":"p"}}}});
        let scope = RepairScope::for_contexts(
            &case(),
            [("D1", &before, &after)],
            &["DISSENT_TARGET_CHANGED D1".into()],
        )
        .unwrap();
        let mut repaired = case();
        repaired["warrants"][0]["reasoning_method"]["method"] = json!("abductive");
        assert!(scope.errors(&repaired).is_empty());
        repaired["warrants"][0]["reasoning_method"]["query"] = json!("q");
        assert!(!scope.errors(&repaired).is_empty());
    }

    #[test]
    fn mixed_structural_failures_keep_the_general_repair_path() {
        assert!(
            RepairScope::for_contexts(
                &case(),
                [],
                &[
                    "DISSENT_TARGET_CHANGED D1".into(),
                    "missing evidence plan".into()
                ]
            )
            .is_none()
        );
    }

    #[test]
    fn repeated_repair_receives_exact_protected_restorations() {
        let mut changed = case();
        changed["grounds"][0]["statement"] = json!("invented");
        let instructions = scope().instructions(&changed);
        assert_eq!(
            instructions["protected_restorations"][0]["path"],
            "/grounds"
        );
        assert_eq!(
            instructions["protected_restorations"][0]["value"],
            case()["grounds"]
        );
    }

    #[test]
    fn changed_support_cannot_escape_a_dissent_only_repair() {
        let mut scope = scope();
        scope.protect_auxiliary(json!({"requirements":["original"]}));
        assert!(
            scope
                .auxiliary_errors(&json!({"requirements":["original"]}))
                .is_empty()
        );
        assert!(
            !scope
                .auxiliary_errors(&json!({"requirements":[]}))
                .is_empty()
        );
    }

    #[test]
    fn reopening_without_an_existing_plan_uses_general_repair() {
        let mut candidate = case();
        candidate["defeaters"][0]["status"] = json!("inapplicable_after_revision");
        candidate["evidence_plan"] = json!([]);
        assert!(
            RepairScope::for_contexts(&candidate, [], &["DISSENT_TARGET_CHANGED D1".into()])
                .is_none()
        );
    }
}
