//! Bounded operator preferences for libraries used by generated evidence tools.
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const RESOLVED_LIBRARY_POLICY_SCHEMA: &str = "engarg.resolved-library-policy/1";
const MAX_POLICY_BYTES: usize = 16_384;
const MAX_TASKS: usize = 32;
const MAX_CHOICES: usize = 64;

pub const GENERATION_INSTRUCTIONS: &str = concat!(
    "LIBRARY POLICY: identify the evidence operation required by the frozen specification, then consult the language-specific task rules. ",
    "Task labels distinguish causal discovery, identification, estimation and assumption testing, abductive likelihood/posterior inputs, inductive sampling or predictive validation, and fixed-context structural counterfactual calculations; do not infer a method from a claim label or a package name. ",
    "An operator rule replaces the corresponding built-in default, including an empty list that disables that default. Preserve supplied package/version choices and use their guidance only when applicable to this operation. ",
    "A task rule may name complementary packages with different roles, not interchangeable alternatives. For example a discovery algorithm may be paired with DoWhy for explicit modelling and assumption/refutation checks; DoWhy alone is not a generic graph-discovery algorithm. ",
    "For an empty or unmatched rule, fallback model permits choosing a suitable available library or standard library; fallback standard_library permits only the language standard library. ",
    "Recommendations and requested versions do not establish that a dependency is installed. Dependencies must already be supplied by the operator's trusted image and build configuration; an unavailable dependency is a configuration problem, never permission to invent results. ",
    "Library policy grants no package installation, download, shell, network, filesystem or credential authority. Preserve the configured image, build/run interface and resource limits. ",
    "Libraries implement the requested source retrieval, calculation or proof check; they do not replace EAL's authored native reasoning methods or claim assessment. ",
    "Library selection is a generation instruction subject to source review and function tests, not proof that an import correctly implements an algorithm. Using a library does not qualify a scientific method, establish causal assumptions, supply independent judgement or make observational association causal. Preserve the authored evidence scope, assumptions, identity, original times, frozen operation and acceptance criteria."
);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryFallback {
    #[default]
    Model,
    StandardLibrary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryChoice {
    pub package: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default)]
    pub guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryPolicy {
    #[serde(default = "enabled")]
    pub use_defaults: bool,
    #[serde(default)]
    pub fallback: LibraryFallback,
    #[serde(default)]
    pub tasks: BTreeMap<String, Vec<LibraryChoice>>,
}

fn enabled() -> bool {
    true
}

impl Default for LibraryPolicy {
    fn default() -> Self {
        Self {
            use_defaults: true,
            fallback: LibraryFallback::Model,
            tasks: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySource {
    Operator,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedLibraryTask {
    pub source: LibrarySource,
    pub choices: Vec<LibraryChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedLibraryPolicy {
    pub schema: String,
    pub language: String,
    pub fallback: LibraryFallback,
    pub tasks: BTreeMap<String, ResolvedLibraryTask>,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn text(value: &str, limit: usize, empty: bool) -> bool {
    value.len() <= limit
        && (empty || !value.trim().is_empty())
        && !value.chars().any(char::is_control)
}

fn task_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
}

impl LibraryChoice {
    fn validate(&self) -> Result<(), Error> {
        if self.package.is_empty()
            || self.package.len() > 256
            || self.package.starts_with('-')
            || !self
                .package
                .bytes()
                .any(|byte| byte.is_ascii_alphanumeric())
            || !self
                .package
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_./@+-".contains(&byte))
            || self
                .version
                .as_ref()
                .is_some_and(|value| !text(value, 128, false))
            || self
                .reference
                .as_ref()
                .is_some_and(|value| !text(value, 2048, false))
            || !text(&self.guidance, 2048, true)
        {
            return Err(invalid(
                "library choices require a bounded package identifier, optional version/reference and control-free guidance",
            ));
        }
        Ok(())
    }
}

fn validate_tasks<'a>(
    tasks: impl Iterator<Item = (&'a String, &'a Vec<LibraryChoice>)>,
) -> Result<(), Error> {
    let mut count = 0usize;
    let mut choices = 0usize;
    for (task, libraries) in tasks {
        count += 1;
        choices = choices.saturating_add(libraries.len());
        if !task_name(task) || count > MAX_TASKS || choices > MAX_CHOICES {
            return Err(invalid(
                "library policy exceeds task/choice bounds or has an invalid task identifier",
            ));
        }
        let mut packages = BTreeSet::new();
        for choice in libraries {
            choice.validate()?;
            if !packages.insert(&choice.package) {
                return Err(invalid(
                    "each library task requires distinct package identifiers",
                ));
            }
        }
    }
    Ok(())
}

fn validate_size(value: &impl Serialize) -> Result<(), Error> {
    if serde_json::to_vec(value)?.len() > MAX_POLICY_BYTES {
        return Err(invalid("library policy exceeds 16 KiB"));
    }
    Ok(())
}

impl LibraryPolicy {
    pub fn validate(&self) -> Result<(), Error> {
        validate_tasks(self.tasks.iter())?;
        validate_size(self)
    }

    pub fn resolve(&self, language: &str) -> Result<ResolvedLibraryPolicy, Error> {
        self.validate()?;
        super::contracts::validate_language(language)?;
        let mut tasks = BTreeMap::new();
        if self.use_defaults {
            let defaults: BTreeMap<String, BTreeMap<String, Vec<LibraryChoice>>> =
                serde_json::from_str(include_str!("library_defaults.json"))?;
            if let Some(selected) = defaults.get(language) {
                validate_tasks(selected.iter())?;
                tasks.extend(selected.iter().map(|(task, choices)| {
                    (
                        task.clone(),
                        ResolvedLibraryTask {
                            source: LibrarySource::Default,
                            choices: choices.clone(),
                        },
                    )
                }));
            }
        }
        tasks.extend(self.tasks.iter().map(|(task, choices)| {
            (
                task.clone(),
                ResolvedLibraryTask {
                    source: LibrarySource::Operator,
                    choices: choices.clone(),
                },
            )
        }));
        let resolved = ResolvedLibraryPolicy {
            schema: RESOLVED_LIBRARY_POLICY_SCHEMA.into(),
            language: language.into(),
            fallback: self.fallback,
            tasks,
        };
        resolved.validate()?;
        Ok(resolved)
    }
}

impl ResolvedLibraryPolicy {
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != RESOLVED_LIBRARY_POLICY_SCHEMA {
            return Err(invalid("unsupported resolved library policy schema"));
        }
        super::contracts::validate_language(&self.language)?;
        validate_tasks(self.tasks.iter().map(|(task, rule)| (task, &rule.choices)))?;
        validate_size(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn choice(package: &str) -> LibraryChoice {
        LibraryChoice {
            package: package.into(),
            version: None,
            reference: None,
            guidance: String::new(),
        }
    }

    #[test]
    fn operator_rules_replace_defaults_and_empty_rules_disable_a_default() {
        let mut policy = LibraryPolicy::default();
        policy
            .tasks
            .insert("causal_testing".into(), vec![choice("operator-causal")]);
        policy
            .tasks
            .insert("statistical_inference".into(), Vec::new());
        let resolved = policy.resolve("python").unwrap();
        let selected = &resolved.tasks["causal_testing"];
        assert_eq!(selected.source, LibrarySource::Operator);
        assert_eq!(selected.choices, vec![choice("operator-causal")]);
        assert_eq!(
            resolved.tasks["statistical_inference"].source,
            LibrarySource::Operator
        );
        assert!(resolved.tasks["statistical_inference"].choices.is_empty());
    }

    #[test]
    fn omitted_configuration_uses_defaults_and_unmatched_tasks_keep_fallback() {
        let omitted: LibraryPolicy = serde_json::from_value(json!({})).unwrap();
        assert_eq!(omitted, LibraryPolicy::default());
        for language in ["python", "rust", "go", "javascript", "typescript"] {
            let resolved = omitted.resolve(language).unwrap();
            assert!(!resolved.tasks.is_empty(), "{language}");
            assert!(
                resolved
                    .tasks
                    .values()
                    .all(|rule| rule.source == LibrarySource::Default)
            );
        }
        let custom = LibraryPolicy {
            use_defaults: false,
            fallback: LibraryFallback::StandardLibrary,
            tasks: BTreeMap::from([("operator_check".into(), vec![choice("stdlib")])]),
        };
        let resolved = custom.resolve("other-language").unwrap();
        assert_eq!(resolved.tasks.len(), 1);
        assert_eq!(resolved.fallback, LibraryFallback::StandardLibrary);
        assert!(
            LibraryPolicy::default()
                .resolve("other-language")
                .unwrap()
                .tasks
                .is_empty()
        );
    }

    #[test]
    fn causal_source_defaults_and_go_solver_name_usable_library_entries() {
        for language in ["javascript", "typescript"] {
            let resolved = LibraryPolicy::default().resolve(language).unwrap();
            let choices = &resolved.tasks["causal_identification"].choices;
            assert_eq!(choices.len(), 1);
            let source = &choices[0];
            assert_eq!(source.package, "jtextor/libdagitty");
            let revision = source.version.as_ref().expect("source default needs a pin");
            assert_eq!(revision.len(), 40);
            assert!(revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(
                source.reference.as_deref(),
                Some(format!("https://github.com/jtextor/libdagitty/tree/{revision}").as_str())
            );
            assert!(source.guidance.contains("libdagitty/src/index.js"));
        }
        let go = LibraryPolicy::default().resolve("go").unwrap();
        assert_eq!(
            go.tasks["formal_verification"].choices[0].package,
            "github.com/crillab/gophersat/solver"
        );
    }

    #[test]
    fn evidence_families_have_bounded_defaults_for_every_supported_language() {
        for (language, abductive, inductive, counterfactual) in [
            ("python", "pgmpy", vec!["scipy", "scikit-learn"], "dowhy"),
            ("rust", "statrs", vec!["statrs"], "nalgebra"),
            (
                "go",
                "gonum.org/v1/gonum/stat/distuv",
                vec!["gonum.org/v1/gonum/stat"],
                "gonum.org/v1/gonum/mat",
            ),
            ("javascript", "mathjs", vec!["simple-statistics"], "mathjs"),
            ("typescript", "mathjs", vec!["simple-statistics"], "mathjs"),
        ] {
            let policy = LibraryPolicy::default().resolve(language).unwrap();
            for (task, packages) in [
                ("abductive", vec![abductive]),
                ("inductive", inductive),
                ("counterfactual", vec![counterfactual]),
            ] {
                let rule = &policy.tasks[task];
                assert_eq!(rule.source, LibrarySource::Default, "{language}/{task}");
                assert_eq!(
                    rule.choices
                        .iter()
                        .map(|choice| choice.package.as_str())
                        .collect::<Vec<_>>(),
                    packages,
                    "{language}/{task}"
                );
                assert!(
                    rule.choices.iter().all(|choice| {
                        !choice.guidance.is_empty() && choice.reference.is_some()
                    })
                );
            }
        }
    }

    #[test]
    fn evidence_family_overrides_are_exact_and_can_disable_the_new_defaults() {
        for language in ["python", "rust", "go", "javascript", "typescript"] {
            let defaults = LibraryPolicy::default().resolve(language).unwrap();
            let mut policy = LibraryPolicy {
                fallback: LibraryFallback::StandardLibrary,
                tasks: BTreeMap::from([
                    ("abductive".into(), vec![choice("operator_likelihoods")]),
                    ("inductive".into(), vec![]),
                    ("counterfactual".into(), vec![choice("operator_scm")]),
                ]),
                ..LibraryPolicy::default()
            };
            let resolved = policy.resolve(language).unwrap();
            assert_eq!(resolved.fallback, LibraryFallback::StandardLibrary);
            assert_eq!(resolved.tasks["abductive"].source, LibrarySource::Operator);
            assert_eq!(
                resolved.tasks["abductive"].choices,
                vec![choice("operator_likelihoods")]
            );
            assert!(resolved.tasks["inductive"].choices.is_empty());
            assert_eq!(resolved.tasks["inductive"].source, LibrarySource::Operator);
            assert_eq!(
                resolved.tasks["probabilistic_inference"],
                defaults.tasks["probabilistic_inference"],
                "family labels must not silently alias existing task rules"
            );
            policy.use_defaults = false;
            let without_defaults = policy.resolve(language).unwrap();
            assert_eq!(without_defaults.tasks.len(), 3);
            assert!(
                without_defaults
                    .tasks
                    .values()
                    .all(|rule| rule.source == LibrarySource::Operator)
            );
        }
    }

    #[test]
    fn malformed_and_excessive_policies_fail_admission() {
        for value in [
            json!({"unknown":true}),
            json!({"fallback":"install_anything"}),
            json!({"tasks":{"causal_testing":[{"package":"dowhy","install":"pip install dowhy"}]}}),
        ] {
            assert!(serde_json::from_value::<LibraryPolicy>(value).is_err());
        }
        for invalid_choice in [
            choice(""),
            choice("pip install dowhy"),
            choice("dowhy;echo"),
            choice("-r"),
            LibraryChoice {
                guidance: "hidden\ncommand".into(),
                ..choice("dowhy")
            },
            LibraryChoice {
                version: Some("".into()),
                ..choice("dowhy")
            },
        ] {
            assert!(invalid_choice.validate().is_err());
        }
        let mut policy = LibraryPolicy {
            use_defaults: false,
            ..LibraryPolicy::default()
        };
        policy.tasks.insert("causal testing".into(), vec![]);
        assert!(policy.resolve("python").is_err());
        policy.tasks = BTreeMap::from([("valid".into(), vec![choice("dowhy"), choice("dowhy")])]);
        assert!(policy.resolve("python").is_err());
        policy.tasks = (0..=MAX_TASKS)
            .map(|n| (format!("task_{n}"), vec![]))
            .collect();
        assert!(policy.resolve("python").is_err());
        policy.tasks = BTreeMap::from([(
            "valid".into(),
            (0..=MAX_CHOICES)
                .map(|n| choice(&format!("package_{n}")))
                .collect(),
        )]);
        assert!(policy.resolve("python").is_err());
        policy.tasks = BTreeMap::from([(
            "valid".into(),
            (0..10)
                .map(|n| LibraryChoice {
                    guidance: "x".repeat(2048),
                    ..choice(&format!("package_{n}"))
                })
                .collect(),
        )]);
        assert!(policy.resolve("python").is_err());
    }

    #[test]
    fn resolved_policy_rejects_unknown_schema_and_preserves_configuration_identity() {
        let mut policy = LibraryPolicy {
            use_defaults: false,
            ..LibraryPolicy::default()
        };
        policy.tasks.insert("custom".into(), vec![choice("one")]);
        let first = policy.resolve("python").unwrap();
        policy.tasks.get_mut("custom").unwrap()[0].version = Some("1.2.3".into());
        let second = policy.resolve("python").unwrap();
        assert_ne!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&second).unwrap()
        );
        let mut invalid = second;
        invalid.schema = "engarg.resolved-library-policy/0".into();
        assert!(invalid.validate().is_err());
    }
}
