//! Lower pure plan projections to bounded EAL expressions and input bindings.
use super::{DerivedSpec, ProgramPlan, invalid, write_bind, write_derived};
use crate::{
    Error,
    eal_export::{name, quoted},
    export_policy::ExportPolicy,
};
use eal_collector_contracts::execution::{DerivationOperation, OutputReference};
use serde_json::{Value, json};

pub(super) fn compile(
    plan: &ProgramPlan,
    policy: &ExportPolicy,
    source: &mut String,
) -> Result<(), Error> {
    for step in &plan.derivations {
        let declaration = name("derivation", &step.id);
        let mut input = json!({"values":{}});
        let mut binds = String::new();
        let expression = match &step.operation {
            DerivationOperation::ProjectValue { fields }
            | DerivationOperation::Project { fields } => {
                let [selected] = step.inputs.as_slice() else {
                    return Err(invalid("projection requires one input"));
                };
                if matches!(step.operation, DerivationOperation::Project { .. })
                    && plan
                        .acquisitions
                        .iter()
                        .any(|source| source.id == selected.step_id && source.for_each.is_some())
                {
                    return Err(invalid(
                        "array projection requires an explicit bounded batch operation",
                    ));
                }
                for (field, pointer) in fields {
                    input["values"][field] = Value::Null;
                    write_bind(
                        &mut binds,
                        &format!("/values/{}", escape(field)),
                        &OutputReference {
                            step_id: selected.step_id.clone(),
                            pointer: format!("{}{}", selected.pointer, pointer),
                        },
                        plan,
                    );
                }
                "values".to_owned()
            }
            DerivationOperation::Extract { pointer } => {
                let [selected] = step.inputs.as_slice() else {
                    return Err(invalid("extraction requires one input"));
                };
                input["values"]["value"] = Value::Null;
                write_bind(
                    &mut binds,
                    "/values/value",
                    &OutputReference {
                        step_id: selected.step_id.clone(),
                        pointer: format!("{}{}", selected.pointer, pointer),
                    },
                    plan,
                );
                "values".to_owned()
            }
            operation => {
                input = json!({"values":vec![Value::Null;step.inputs.len()]});
                for (index, selected) in step.inputs.iter().enumerate() {
                    write_bind(&mut binds, &format!("/values/{index}"), selected, plan);
                }
                let expressions: Vec<_> = (0..step.inputs.len())
                    .map(|i| format!("at(values, {i})"))
                    .collect();
                let value = match operation {
                    DerivationOperation::Collect => "values".to_owned(),
                    DerivationOperation::Count => "exact_count(values)".to_owned(),
                    DerivationOperation::Sum => "exact_sum(values)".to_owned(),
                    DerivationOperation::Mean => "exact_mean(values)".to_owned(),
                    DerivationOperation::Minimum => "exact_min(values)".to_owned(),
                    DerivationOperation::Maximum => "exact_max(values)".to_owned(),
                    DerivationOperation::IdentityEqual => {
                        if expressions.len() < 2 {
                            return Err(invalid("identity comparison requires two inputs"));
                        }
                        expressions
                            .windows(2)
                            .map(|p| format!("equal({}, {})", p[0], p[1]))
                            .collect::<Vec<_>>()
                            .join(" and ")
                    }
                    DerivationOperation::AnyIdentityEqual => {
                        if expressions.len() != 2 {
                            return Err(invalid("identity membership requires two inputs"));
                        }
                        format!("contains({}, {})", expressions[1], expressions[0])
                    }
                    DerivationOperation::Scale {
                        numerator,
                        denominator,
                        from_unit,
                        to_unit,
                    } => {
                        if expressions.len() != 1 {
                            return Err(invalid("scaling requires one input"));
                        }
                        format!(
                            "exact_scale({}, {}, {}, {}, {})",
                            expressions[0],
                            quoted(numerator),
                            quoted(denominator),
                            quoted(from_unit),
                            quoted(to_unit)
                        )
                    }
                    _ => unreachable!("projection/extraction handled above"),
                };
                format!("object(\"value\", {value})")
            }
        };
        write_derived(
            source,
            DerivedSpec {
                name: &declaration,
                kind: "calculation",
                max_age: policy.acquisition_max_age_seconds,
                input: &input,
                bindings: &binds,
                expression: &expression,
                predicates: &[],
            },
        )?;
    }
    Ok(())
}

pub(super) fn wrapped(plan: &ProgramPlan, id: &str) -> bool {
    plan.derivations
        .iter()
        .find(|step| step.id == id)
        .is_some_and(|step| {
            !matches!(
                step.operation,
                DerivationOperation::Project { .. } | DerivationOperation::ProjectValue { .. }
            )
        })
}

pub(super) fn insert_slot(value: &mut Value, pointer: &str) -> Result<(), Error> {
    if !eal_collector_contracts::execution::valid_pointer(pointer) || pointer.is_empty() {
        return Err(invalid(
            "method input destination must be a nonempty JSON pointer",
        ));
    }
    let segments: Vec<_> = pointer[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect();
    let mut current = value;
    for (index, segment) in segments.iter().enumerate() {
        if index + 1 == segments.len() {
            match current {
                Value::Object(object) => {
                    object.insert(segment.clone(), Value::Null);
                }
                Value::Array(array) => {
                    let i = segment
                        .parse::<usize>()
                        .map_err(|_| invalid("array slot requires an index"))?;
                    if i > 128 {
                        return Err(invalid("array input slot exceeds finite bound"));
                    }
                    while array.len() <= i {
                        array.push(Value::Null);
                    }
                    array[i] = Value::Null;
                }
                _ => {
                    return Err(invalid(
                        "method input destination conflicts with another field",
                    ));
                }
            }
            return Ok(());
        }
        let next_is_index = segments[index + 1].parse::<usize>().is_ok();
        let placeholder = if next_is_index { json!([]) } else { json!({}) };
        current = match current {
            Value::Object(object) => object.entry(segment.clone()).or_insert(placeholder),
            Value::Array(array) => {
                let i = segment
                    .parse::<usize>()
                    .map_err(|_| invalid("array slot requires index"))?;
                if i > 128 {
                    return Err(invalid("array input slot exceeds bound"));
                }
                while array.len() <= i {
                    array.push(Value::Null);
                }
                if array[i].is_null() {
                    array[i] = placeholder;
                }
                &mut array[i]
            }
            _ => return Err(invalid("method input destinations overlap")),
        };
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
