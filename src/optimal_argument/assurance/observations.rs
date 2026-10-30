//! Project original native acquisition records into EAL verification observations.
use crate::support_requirements::SupportedArgument;
use eal_core::verification::{self, Observation, ObservationStatus};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn context(proposal: &SupportedArgument, execution: &Value) -> verification::Context {
    let run = &execution["result"];
    let observations = proposal
        .support_elaboration
        .acquisitions
        .iter()
        .filter_map(|a| {
            let declaration = crate::eal_export::name("acquisition", &a.id);
            let record = &run["collection"]["records"][&declaration];
            if record.is_null() {
                return None;
            }
            let evidence = &run["assessment"]["evidence"][&declaration];
            let status = if record["status"] == "ok" && evidence["status"] == "available" {
                ObservationStatus::Available
            } else if record["status"] == "ok" {
                ObservationStatus::Unavailable
            } else {
                ObservationStatus::Failed
            };
            Some((
                a.id.clone(),
                Observation {
                    status,
                    value: record["value"].clone(),
                    observed_at: record["collected_at"].as_str().map(str::to_owned),
                    source_digest: record["source_digest"].as_str().map(str::to_owned),
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    verification::Context {
        observations,
        assessed_at: run["assessment"]["assessed_at"]
            .as_str()
            .unwrap_or("")
            .into(),
    }
}
