//! Conflict-checked activation of explicitly approved MCP connection records.
use engarg::Error;
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn entries<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], Error> {
    match value.get(field) {
        None => Ok(&[]),
        Some(Value::Array(entries)) => Ok(entries),
        _ => Err(invalid("evidence registration fields require arrays")),
    }
}
fn merge_identity(target: &mut Value, field: &str, entry: Value, key: &str) -> Result<(), Error> {
    let identity = entry
        .get(key)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("approved registration has no identity"))?;
    if target.get(field).is_none() {
        target[field] = json!([]);
    }
    let values = target[field]
        .as_array_mut()
        .ok_or_else(|| invalid("existing evidence registrations require arrays"))?;
    let matches = values
        .iter()
        .filter(|prior| prior.get(key).and_then(Value::as_str) == Some(identity))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => values.push(entry),
        [prior] if **prior == entry => {}
        _ => {
            return Err(invalid(
                "approved registration conflicts with an existing identity",
            ));
        }
    }
    Ok(())
}
pub(super) fn activate(
    target: &mut Value,
    additions: Option<&Value>,
) -> Result<Vec<String>, Error> {
    if !target.is_object() || target["schema"] != "eal.evidence-config/1" {
        return Err(invalid(
            "integration requires eal.evidence-config/1 connections",
        ));
    }
    let Some(additions) = additions else {
        return Ok(Vec::new());
    };
    let object = additions
        .as_object()
        .ok_or_else(|| invalid("approved MCP registrations require an object"))?;
    if additions["schema"] != "eal.evidence-config/1"
        || object.keys().any(|field| {
            ![
                "schema",
                "connections",
                "local_catalogues",
                "approved_catalogues",
            ]
            .contains(&field.as_str())
        })
    {
        return Err(invalid(
            "MCP registrations accept only approved connections and catalogues; native providers require verified package registration",
        ));
    }
    let connections = entries(additions, "connections")?;
    if connections.len() > 128 {
        return Err(invalid("too many approved MCP registrations"));
    }
    let mut ids = BTreeSet::new();
    for connection in connections {
        let id = connection["id"]
            .as_str()
            .ok_or_else(|| invalid("approved MCP connection requires an ID"))?;
        if !ids.insert(id.to_owned()) {
            return Err(invalid("approved MCP registrations repeat a connection ID"));
        }
        // EAL discovery performs the authoritative protocol/schema validation.
        // Do not rewrite transports or qualification pins in this host adapter.
        merge_identity(target, "connections", connection.clone(), "id")?;
    }
    for catalogue in entries(additions, "local_catalogues")? {
        if catalogue["schema"] != "eal.capability-catalogue/1"
            || !catalogue["capabilities"].is_array()
        {
            return Err(invalid("approved local catalogue has an invalid schema"));
        }
        if target.get("local_catalogues").is_none() {
            target["local_catalogues"] = json!([]);
        }
        let existing = target["local_catalogues"]
            .as_array_mut()
            .ok_or_else(|| invalid("local catalogues require an array"))?;
        if !existing.contains(catalogue) {
            existing.push(catalogue.clone());
        }
    }
    for catalogue in entries(additions, "approved_catalogues")? {
        merge_identity(target, "approved_catalogues", catalogue.clone(), "url")?;
    }
    Ok(ids.into_iter().collect())
}
pub(super) fn environment(config: &Value, names: &mut Vec<String>) -> Result<(), Error> {
    let mut selected = names.iter().cloned().collect::<BTreeSet<_>>();
    for connection in entries(config, "connections")? {
        let transport = &connection["transport"];
        match transport["kind"].as_str() {
            Some("stdio") => {
                if !transport["command"]
                    .as_str()
                    .is_some_and(|path| std::path::Path::new(path).is_absolute())
                    || transport.get("cwd").is_some_and(|path| {
                        !path.is_null()
                            && !path
                                .as_str()
                                .is_some_and(|path| std::path::Path::new(path).is_absolute())
                    })
                {
                    return Err(invalid("approved MCP stdio paths must be absolute"));
                }
                for name in entries(transport, "inherit_env")? {
                    selected.insert(
                        name.as_str()
                            .ok_or_else(|| invalid("environment references must be names"))?
                            .to_owned(),
                    );
                }
            }
            Some("http") => {
                if let Some(name) = transport.get("token_env").filter(|name| !name.is_null()) {
                    selected.insert(
                        name.as_str()
                            .ok_or_else(|| invalid("credential references must be names"))?
                            .to_owned(),
                    );
                }
            }
            _ => return Err(invalid("unknown approved MCP transport")),
        }
    }
    *names = selected.into_iter().collect();
    eal_collector_contracts::validate_environment_names(names, 64).map_err(Error::InvalidInput)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_is_explicit_idempotent_and_conflict_checked() {
        let mut base = json!({"schema":"eal.evidence-config/1","connections":[]});
        let approved = json!({"schema":"eal.evidence-config/1","connections":[{"id":"new_mcp","transport":{"kind":"stdio","command":"/operator/server","arguments":[],"cwd":"/operator","inherit_env":["SOURCE_TOKEN"]}}]});
        assert_eq!(
            activate(&mut base, Some(&approved)).unwrap(),
            vec!["new_mcp"]
        );
        activate(&mut base, Some(&approved)).unwrap();
        assert_eq!(base["connections"].as_array().unwrap().len(), 1);
        let mut names = Vec::new();
        environment(&base, &mut names).unwrap();
        assert_eq!(names, vec!["SOURCE_TOKEN"]);
        let mut changed = approved.clone();
        changed["connections"][0]["transport"]["command"] = json!("/different");
        assert!(activate(&mut base, Some(&changed)).is_err());
        assert!(
            activate(
                &mut base,
                Some(&json!({"schema":"eal.evidence-config/1","native_providers":[{}]}))
            )
            .is_err()
        );
    }
}
