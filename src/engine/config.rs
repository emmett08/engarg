//! Workflow selection, persisted configuration and completion-budget admission.

use serde::{Deserialize, Serialize};

use super::topology::DebateConfig;
use crate::Error;

/// Architecture selected for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Independent specialists, optional routed debate, synthesis, critique and adjudication.
    Hybrid,
    /// A one-call baseline for evaluation and inexpensive straightforward cases.
    Single,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hybrid" => Ok(Self::Hybrid),
            "single" => Ok(Self::Single),
            _ => Err("mode must be hybrid or single".to_owned()),
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Hybrid => "hybrid",
            Self::Single => "single",
        })
    }
}

/// Logical-call and byte budgets. HTTP time and output-token budgets belong to the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub mode: Mode,
    /// Optional routed specialist debate before global synthesis and review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate: Option<DebateConfig>,
    pub max_calls: u32,
    /// Optional maximum number of warrant layers on every root-to-ground route.
    #[serde(default)]
    pub max_argument_depth: Option<u8>,
    /// Extra validation-guided calls, reserved within max_calls before execution.
    #[serde(default)]
    pub max_repairs: u8,
    /// Optional operator-selected collector descriptions, retained for reproducible planning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_catalogue: Option<crate::collector_catalogue::CollectorCatalogue>,
    /// Maximum encoded bytes of any complete provider request, including its schema.
    pub max_input_bytes: usize,
    /// Maximum bytes in any returned model content.
    pub max_response_bytes: usize,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Hybrid,
            debate: None,
            max_calls: 6,
            max_argument_depth: None,
            max_repairs: 0,
            collector_catalogue: None,
            max_input_bytes: 1_048_576,
            max_response_bytes: 131_072,
        }
    }
}

impl RunConfig {
    /// Reserve every planned completion and the full repair allowance up front.
    pub fn required_calls(&self) -> Result<u32, Error> {
        let calls = match (self.mode, &self.debate) {
            (Mode::Single, Some(_)) => {
                return Err(Error::InvalidInput(
                    "specialist debate requires hybrid mode".into(),
                ));
            }
            (Mode::Single, None) => 1,
            (Mode::Hybrid, None) => 6,
            (Mode::Hybrid, Some(debate)) => debate.required_calls()?,
        };
        Ok(calls + u32::from(self.max_repairs))
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if self
            .max_argument_depth
            .is_some_and(|depth| !(1..=16).contains(&depth))
            || self.max_repairs > 3
        {
            return Err(Error::InvalidInput(
                "argument-depth must be 1–16 and max-repairs must be 0–3".into(),
            ));
        }
        let required_calls = self.required_calls()?;
        if self.max_calls < required_calls {
            return Err(Error::Budget(format!(
                "{} mode including the repair allowance requires {required_calls} calls but max_calls is {}",
                self.mode, self.max_calls,
            )));
        }
        if self.max_input_bytes == 0 || self.max_response_bytes == 0 {
            return Err(Error::Budget("byte budgets must be positive".into()));
        }
        Ok(())
    }
}
