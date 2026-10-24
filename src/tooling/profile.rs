//! Operator-owned language and execution settings for generated collectors.
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

pub const TOOLCHAIN_SCHEMA: &str = "engarg.toolchain/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainProfile {
    pub schema: String,
    pub language: String,
    pub docker: String,
    pub image: String,
    pub build_argv: Vec<String>,
    pub run_argv: Vec<String>,
    pub inherit_env: Vec<String>,
    pub runtime_network: String,
    pub timeout_seconds: u64,
    pub memory_mb: u64,
    pub output_bytes: usize,
}

impl ToolchainProfile {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let bytes = crate::bounded_file::read(path, 65_536, "toolchain profile")?;
        let value = eal_collector_contracts::strict_json_exact(
            &bytes,
            eal_collector_contracts::JsonLimits {
                max_bytes: 65_536,
                ..Default::default()
            },
        )
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
        let profile: Self = serde_json::from_value(value)?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), Error> {
        let invalid = |message: &str| Error::InvalidInput(message.to_owned());
        if self.schema != TOOLCHAIN_SCHEMA {
            return Err(invalid("unsupported toolchain profile schema"));
        }
        super::contracts::validate_language(&self.language)?;
        if self.docker.is_empty()
            || self.docker.len() > 4096
            || self.docker.chars().any(char::is_control)
            || self.image.is_empty()
            || self.image.starts_with('-')
            || self.image.len() > 4096
            || self.image.chars().any(char::is_whitespace)
            || self.image.chars().any(char::is_control)
        {
            return Err(invalid(
                "toolchain requires bounded Docker and image settings",
            ));
        }
        for argv in [&self.build_argv, &self.run_argv] {
            if argv.len() > 64
                || argv
                    .iter()
                    .any(|part| part.len() > 8192 || part.chars().any(char::is_control))
            {
                return Err(invalid("toolchain command arguments exceed their limits"));
            }
            if argv.first().is_some_and(String::is_empty) {
                return Err(invalid("toolchain executable must not be empty"));
            }
        }
        if self.run_argv.is_empty() {
            return Err(invalid("toolchain requires a run command"));
        }
        if !matches!(self.runtime_network.as_str(), "none" | "bridge") {
            return Err(invalid("runtime network must be none or bridge"));
        }
        if !(1..=3540).contains(&self.timeout_seconds)
            || !(16..=4096).contains(&self.memory_mb)
            || !(1024..=16_777_216).contains(&self.output_bytes)
        {
            return Err(invalid(
                "toolchain execution limits are outside the supported range",
            ));
        }
        let mut names = BTreeSet::new();
        if self.inherit_env.len() > 48
            || self
                .inherit_env
                .iter()
                .any(|name| !eal_collector_contracts::environment_name(name) || !names.insert(name))
        {
            return Err(invalid(
                "toolchain environment requires distinct valid names",
            ));
        }
        Ok(())
    }

    /// Only the container interface enters model prompts, never host paths or values.
    pub fn description(&self) -> Result<String, Error> {
        self.validate()?;
        Ok(serde_json::to_string(&serde_json::json!({
            "language": self.language,
            "build_argv": self.build_argv,
            "run_argv": self.run_argv,
            "source_directory": "/src",
            "fixture_directory": "/fixtures",
            "build_directory": "/build",
            "runtime_environment_names": self.inherit_env,
            "fixture_network": "none",
            "runtime_network": self.runtime_network,
            "timeout_seconds": self.timeout_seconds,
            "memory_mb": self.memory_mb,
            "output_bytes": self.output_bytes
        }))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ToolchainProfile {
        ToolchainProfile {
            schema: TOOLCHAIN_SCHEMA.into(),
            language: "python".into(),
            docker: "docker".into(),
            image: "python:3.13-slim".into(),
            build_argv: vec![
                "python3".into(),
                "-m".into(),
                "compileall".into(),
                "/src".into(),
            ],
            run_argv: vec!["python3".into(), "/src/collector.py".into()],
            inherit_env: vec!["SERVICE_TOKEN".into()],
            runtime_network: "none".into(),
            timeout_seconds: 30,
            memory_mb: 128,
            output_bytes: 1_048_576,
        }
    }

    #[test]
    fn profile_is_operator_owned_and_does_not_disclose_host_execution_paths() {
        let mut selected = profile();
        selected.docker = "/private/operator/docker".into();
        selected.validate().unwrap();
        let description = selected.description().unwrap();
        assert!(description.contains("SERVICE_TOKEN"));
        assert!(!description.contains("/private/operator"));
        selected.runtime_network = "host".into();
        assert!(selected.validate().is_err());
        selected.runtime_network = "none".into();
        selected.inherit_env.push("SERVICE_TOKEN".into());
        assert!(selected.validate().is_err());
    }
}
