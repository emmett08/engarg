//! Admit generation preferences separately from the shared runtime toolchain.
pub use eal_tool_runtime::profile::{TOOLCHAIN_SCHEMA, ToolchainProfile};

use super::library_policy::LibraryPolicy;
use crate::Error;
use serde::Deserialize;
use std::path::Path;

pub const GENERATION_TOOLCHAIN_SCHEMA: &str = "engarg.toolchain/2";

/// Authoring-only preferences never enter the standalone runtime profile.
#[derive(Debug, Clone)]
pub struct ToolchainConfiguration {
    pub profile: ToolchainProfile,
    pub libraries: LibraryPolicy,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerationConfiguration {
    #[serde(default)]
    libraries: LibraryPolicy,
}

impl ToolchainConfiguration {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let bytes = crate::bounded_file::read(path, 65_536, "toolchain configuration")?;
        let mut value = eal_collector_contracts::strict_json_exact(
            &bytes,
            eal_collector_contracts::JsonLimits {
                max_bytes: 65_536,
                ..Default::default()
            },
        )
        .map_err(Error::InvalidInput)?;
        let object = value.as_object_mut().ok_or_else(|| {
            Error::InvalidInput("toolchain configuration must be an object".into())
        })?;
        let generation = match object.get("schema").and_then(serde_json::Value::as_str) {
            Some(TOOLCHAIN_SCHEMA) if object.contains_key("generation") => {
                return Err(Error::InvalidInput(
                    "generation preferences require engarg.toolchain/2".into(),
                ));
            }
            Some(TOOLCHAIN_SCHEMA) => None,
            Some(GENERATION_TOOLCHAIN_SCHEMA) => {
                object.insert("schema".into(), TOOLCHAIN_SCHEMA.into());
                object.remove("generation")
            }
            _ => {
                return Err(Error::InvalidInput(
                    "unsupported toolchain configuration schema".into(),
                ));
            }
        }
        .map(serde_json::from_value::<GenerationConfiguration>)
        .transpose()?
        .unwrap_or_default();
        let selected = Self {
            profile: serde_json::from_value(value)?,
            libraries: generation.libraries,
        };
        selected.profile.validate()?;
        selected.libraries.resolve(&selected.profile.language)?;
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn profile() -> Value {
        json!({
            "schema":"engarg.toolchain/1","language":"python","docker":"docker",
            "image":"python:3.13-slim","build_argv":[],
            "run_argv":["python3","/src/collector.py"],"inherit_env":[],
            "runtime_network":"none","timeout_seconds":30,"memory_mb":128,
            "output_bytes":1048576
        })
    }

    fn load(text: &str) -> Result<ToolchainConfiguration, Error> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("toolchain.json");
        std::fs::write(&path, text).unwrap();
        ToolchainConfiguration::load(&path)
    }

    #[test]
    fn legacy_profiles_remain_exact_plain_runtime_profiles() {
        let original = profile();
        let loaded = load(&original.to_string()).unwrap();
        assert_eq!(serde_json::to_value(loaded.profile).unwrap(), original);
        assert_eq!(loaded.libraries, LibraryPolicy::default());
    }

    #[test]
    fn preferences_are_admitted_without_granting_runtime_authority() {
        let original = profile();
        let mut extended = original.clone();
        extended["schema"] = GENERATION_TOOLCHAIN_SCHEMA.into();
        extended["generation"] = json!({"libraries":{
            "use_defaults":false,"fallback":"standard_library",
            "tasks":{"causal_testing":[{"package":"dowhy","version":"0.14","guidance":"Use for the authored model's refutation checks."}]}
        }});
        let loaded = load(&extended.to_string()).unwrap();
        assert_eq!(serde_json::to_value(loaded.profile).unwrap(), original);
        assert!(!loaded.libraries.use_defaults);
        assert_eq!(loaded.libraries.tasks["causal_testing"][0].package, "dowhy");
    }

    #[test]
    fn duplicate_unknown_and_oversized_configuration_is_rejected() {
        let original = profile();
        let text = original.to_string();
        let duplicate = text.replacen('{', "{\"language\":\"python\",", 1);
        assert!(load(&duplicate).is_err());
        for extra in [
            json!({"libraries":{},"install":true}),
            json!({"libraries":{"fallback":"shell"}}),
            json!({"libraries":null}),
        ] {
            let mut extended = original.clone();
            extended["schema"] = GENERATION_TOOLCHAIN_SCHEMA.into();
            extended["generation"] = extra;
            assert!(load(&extended.to_string()).is_err());
        }
        let mut unknown = original.clone();
        unknown["install"] = json!(true);
        assert!(load(&unknown.to_string()).is_err());
        let mut duplicate_policy = original.clone();
        duplicate_policy["schema"] = GENERATION_TOOLCHAIN_SCHEMA.into();
        duplicate_policy["generation"] = json!({"libraries":{"tasks":{}}});
        let duplicate_policy = duplicate_policy
            .to_string()
            .replace("\"tasks\":{}", "\"tasks\":{},\"tasks\":{}");
        assert!(load(&duplicate_policy).is_err());
        assert!(load(&" ".repeat(65_537)).is_err());
    }

    #[test]
    fn authoring_schema_migration_does_not_expand_the_legacy_runtime_profile() {
        let mut legacy = profile();
        legacy["generation"] = json!({"libraries":{}});
        assert!(
            load(&legacy.to_string())
                .unwrap_err()
                .to_string()
                .contains("generation preferences require engarg.toolchain/2")
        );
        let mut unknown = profile();
        unknown["schema"] = json!("engarg.toolchain/3");
        assert!(load(&unknown.to_string()).is_err());
        let mut authoring = profile();
        authoring["schema"] = GENERATION_TOOLCHAIN_SCHEMA.into();
        let loaded = load(&authoring.to_string()).unwrap();
        assert_eq!(loaded.profile.schema, TOOLCHAIN_SCHEMA);
        assert_eq!(serde_json::to_value(loaded.profile).unwrap(), profile());
    }

    #[test]
    fn combined_default_and_operator_limits_fail_before_execution() {
        let mut configuration = profile();
        configuration["schema"] = GENERATION_TOOLCHAIN_SCHEMA.into();
        let tasks = (0..32)
            .map(|n| (format!("custom_{n}"), json!([])))
            .collect::<serde_json::Map<_, _>>();
        configuration["generation"] = json!({"libraries":{
            "use_defaults":true,"tasks":tasks
        }});
        assert!(load(&configuration.to_string()).is_err());
        configuration["generation"]["libraries"]["use_defaults"] = json!(false);
        assert!(load(&configuration.to_string()).is_ok());
    }

    #[test]
    fn all_current_examples_load_through_the_authoring_configuration() {
        for language in ["python", "rust", "go", "javascript", "typescript"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples/toolchains")
                .join(format!("{language}.json"));
            let loaded = ToolchainConfiguration::load(&path).unwrap();
            assert_eq!(loaded.profile.language, language);
        }
    }
}
