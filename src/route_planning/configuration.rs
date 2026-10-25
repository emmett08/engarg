//! Validate operator input projection policy before discovery or model calls.
use super::*;
use crate::Error;
use eal_collector_contracts::execution::valid_pointer;

impl PlanningConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != PLANNING_CONFIG_SCHEMA
            || self.batch_correspondences.len() > 64
            || self
                .batch_correspondences
                .iter()
                .map(|batch| &batch.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.batch_correspondences.len()
            || self.bindings.len() > 4096
            || self
                .bindings
                .iter()
                .map(|b| &b.capability_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.bindings.len()
        {
            return Err(Error::InvalidInput(
                "Unsupported or duplicate capability planning configuration".into(),
            ));
        }
        for batch in &self.batch_correspondences {
            batch.validate()?;
        }
        self.ranking.validate().map_err(Error::InvalidInput)?;
        eal_collector_contracts::execution::EvidenceExecutionPlan {
            schema: eal_collector_contracts::execution::PLAN_SCHEMA.into(),
            acquisitions: vec![],
            derivations: vec![],
            evaluations: vec![],
            scheduling: self.scheduling.clone(),
        }
        .validate()
        .map_err(Error::InvalidInput)?;
        for binding in &self.bindings {
            if !binding.arguments.is_object()
                || binding
                    .input_bindings
                    .values()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != binding.input_bindings.len()
                || binding
                    .input_bindings
                    .iter()
                    .any(|(to, from)| !valid_pointer(to) || !valid_pointer(from))
            {
                return Err(Error::InvalidInput(
                    "Capability bindings require object arguments and valid JSON pointers".into(),
                ));
            }
        }
        Ok(())
    }
}
