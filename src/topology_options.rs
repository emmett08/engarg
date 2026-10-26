//! CLI admission of bounded specialist-debate graph configurations.

use clap::{Args, ValueEnum};
use engarg::{
    Error,
    engine::{
        Mode,
        topology::{Adaptation, DebateConfig, Topology},
    },
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Preset {
    #[value(alias = "fully_connected")]
    FullyConnected,
    Grouped,
    Chain,
    Ring,
    Star,
    Tree,
    Layered,
    Sparse,
    Bipartite,
}

impl Preset {
    fn name(self) -> &'static str {
        match self {
            Self::FullyConnected => "fully_connected",
            Self::Grouped => "grouped",
            Self::Chain => "chain",
            Self::Ring => "ring",
            Self::Star => "star",
            Self::Tree => "tree",
            Self::Layered => "layered",
            Self::Sparse => "sparse",
            Self::Bipartite => "bipartite",
        }
    }
}

#[derive(Args)]
pub(crate) struct TopologyOptions {
    /// Graph used for specialist peer exchange in hybrid mode.
    #[arg(long, value_enum, conflicts_with = "topology_config")]
    topology: Option<Preset>,
    /// Specialist participants for a topology preset. Defaults to 6.
    #[arg(long, requires = "topology", value_parser = clap::value_parser!(u8).range(3..=16))]
    debate_participants: Option<u8>,
    /// Synchronous peer-exchange rounds after independent assessments. Defaults to 1.
    #[arg(long, requires = "topology", value_parser = clap::value_parser!(u8).range(1..=8))]
    debate_rounds: Option<u8>,
    /// Complete JSON debate configuration with explicit edges, partitions or scheduling.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["topology", "debate_participants", "debate_rounds"])]
    topology_config: Option<PathBuf>,
    /// eal-coordinate executable for selected topologies. Defaults to PATH lookup.
    #[arg(long, value_name = "PATH")]
    eal_coordinate: Option<PathBuf>,
    /// New directory retaining the coordinator's frozen plan and execution journal.
    #[arg(long, value_name = "DIRECTORY")]
    coordination_run: Option<PathBuf>,
}

impl TopologyOptions {
    pub(crate) fn coordinator(
        &self,
        selected: bool,
        output: Option<&std::path::Path>,
    ) -> Result<Option<engarg::engine::CoordinatorConfig>, Error> {
        if !selected {
            if self.eal_coordinate.is_some() || self.coordination_run.is_some() {
                return Err(Error::InvalidInput(
                    "--eal-coordinate and --coordination-run require a selected topology".into(),
                ));
            }
            return Ok(None);
        }
        let run_directory = match (&self.coordination_run, output) {
            (Some(path), _) => path.clone(),
            (None, Some(path)) => {
                let mut name = path.as_os_str().to_os_string();
                name.push(".coordination");
                PathBuf::from(name)
            }
            (None, None) => {
                let epoch = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| Error::InvalidInput("system clock precedes UNIX epoch".into()))?;
                std::env::current_dir()?.join(format!(
                    "engarg-coordination-{}-{}",
                    std::process::id(),
                    epoch.as_nanos(),
                ))
            }
        };
        if output.is_some_and(|path| path == run_directory) {
            return Err(Error::InvalidInput(
                "coordination run directory and output file require different paths".into(),
            ));
        }
        Ok(Some(engarg::engine::CoordinatorConfig {
            executable: self
                .eal_coordinate
                .clone()
                .or_else(|| std::env::var_os("ENGARG_EAL_COORDINATE").map(PathBuf::from))
                .unwrap_or_else(|| "eal-coordinate".into()),
            run_directory,
        }))
    }

    /// Validate graph shape and mode before credentials or provider calls are used.
    pub(crate) fn admit(&self, mode: Mode) -> Result<Option<DebateConfig>, Error> {
        let config = if let Some(path) = &self.topology_config {
            let bytes = engarg::bounded_file::read(path, 65_536, "topology configuration")?;
            Some(
                serde_json::from_slice::<DebateConfig>(&bytes).map_err(|error| {
                    Error::InvalidInput(format!("invalid topology configuration: {error}"))
                })?,
            )
        } else if let Some(preset) = self.topology {
            let participants = self.debate_participants.unwrap_or(6);
            Some(DebateConfig {
                participants,
                rounds: self.debate_rounds.unwrap_or(1),
                topology: Topology::preset(preset.name(), participants)?,
                adaptation: Adaptation::Static {},
            })
        } else {
            None
        };
        if let Some(config) = &config {
            if mode != Mode::Hybrid {
                return Err(Error::InvalidInput(
                    "--topology and --topology-config require --mode hybrid".into(),
                ));
            }
            config.graphs()?;
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        options: TopologyOptions,
    }

    #[test]
    fn absent_options_preserve_the_legacy_workflow() {
        let cli = TestCli::try_parse_from(["engarg"]).unwrap();
        assert!(cli.options.admit(Mode::Hybrid).unwrap().is_none());
        assert!(cli.options.admit(Mode::Single).unwrap().is_none());
    }

    #[test]
    fn every_preset_and_count_is_admitted_with_explicit_defaults() {
        for preset in [
            "fully-connected",
            "fully_connected",
            "grouped",
            "chain",
            "ring",
            "star",
            "tree",
            "layered",
            "sparse",
            "bipartite",
        ] {
            let cli = TestCli::try_parse_from(["engarg", "--topology", preset]).unwrap();
            let config = cli.options.admit(Mode::Hybrid).unwrap().unwrap();
            assert_eq!(config.participants, 6);
            assert_eq!(config.rounds, 1);
            assert_eq!(config.specialist_calls().unwrap(), 12);
        }
        let cli = TestCli::try_parse_from([
            "engarg",
            "--topology",
            "ring",
            "--debate-participants",
            "4",
            "--debate-rounds",
            "2",
        ])
        .unwrap();
        let config = cli.options.admit(Mode::Hybrid).unwrap().unwrap();
        assert_eq!(config.participants, 4);
        assert_eq!(config.rounds, 2);
        assert_eq!(config.specialist_calls().unwrap(), 12);
        assert!(cli.options.admit(Mode::Single).is_err());
    }

    #[test]
    fn preset_arguments_require_a_preset_and_valid_ranges() {
        for args in [
            vec!["engarg", "--debate-participants", "4"],
            vec!["engarg", "--debate-rounds", "2"],
            vec!["engarg", "--topology", "ring", "--debate-participants", "2"],
            vec![
                "engarg",
                "--topology",
                "ring",
                "--debate-participants",
                "17",
            ],
            vec!["engarg", "--topology", "ring", "--debate-rounds", "0"],
            vec!["engarg", "--topology", "ring", "--debate-rounds", "9"],
            vec!["engarg", "--topology", "unknown"],
        ] {
            assert!(TestCli::try_parse_from(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn file_configuration_excludes_preset_options_and_validates_shape() {
        for flag in ["--topology", "--debate-participants", "--debate-rounds"] {
            let value = if flag == "--topology" { "ring" } else { "4" };
            assert!(
                TestCli::try_parse_from([
                    "engarg",
                    "--topology-config",
                    "config.json",
                    flag,
                    value,
                ])
                .is_err()
            );
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("topology.json");
        std::fs::write(
            &path,
            r#"{"participants":4,"rounds":1,"topology":{"kind":"ring"}}"#,
        )
        .unwrap();
        let cli = TestCli::try_parse_from(["engarg", "--topology-config", path.to_str().unwrap()])
            .unwrap();
        assert_eq!(
            cli.options
                .admit(Mode::Hybrid)
                .unwrap()
                .unwrap()
                .specialist_calls()
                .unwrap(),
            8
        );
        std::fs::write(
            &path,
            r#"{"participants":4,"rounds":1,"topology":{"kind":"star","hub":9}}"#,
        )
        .unwrap();
        assert!(cli.options.admit(Mode::Hybrid).is_err());
        std::fs::write(
            &path,
            r#"{"participants":4,"rounds":1,"topology":{"kind":"ring"},"unknown":true}"#,
        )
        .unwrap();
        assert!(cli.options.admit(Mode::Hybrid).is_err());
    }

    #[test]
    fn checked_in_configurations_are_accepted() {
        for (text, calls) in [
            (include_str!("../examples/debate/sparse.json"), 15),
            (include_str!("../examples/debate/scheduled.json"), 21),
        ] {
            let config: DebateConfig = serde_json::from_str(text).unwrap();
            assert_eq!(config.specialist_calls().unwrap() + 3, calls);
        }
    }
}
