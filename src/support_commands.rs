//! Operator-owned discovery admission for the support-graph CLI.
use eal_collector_contracts::capability::CapabilityCatalogue;
use engarg::{
    Error,
    domain::ArgumentCase,
    engine::RouteResolver,
    route_planning::{DiscoverySnapshot, Planner, PlanningConfig},
    support_requirements::SupportElaboration,
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryReport {
    limits: Value,
    budget_usage: Value,
    schema: String,
    catalogue: CapabilityCatalogue,
    admitted_capabilities: Vec<String>,
    interfaces: Vec<Value>,
    gaps: Vec<Value>,
    incomplete: bool,
    metrics: Value,
}

struct DiscoveryBudget {
    requests: u64,
    bytes: u64,
    timeout_seconds: u64,
    pages: u64,
}
fn remaining(discovery: &DiscoverySnapshot) -> Option<DiscoveryBudget> {
    let limits = &discovery.limits;
    let used = &discovery.metrics;
    let allocation = &discovery.budget_usage;
    let requests = limits["requests"]
        .as_u64()?
        .saturating_sub(allocation["requests_upper_bound"].as_u64()?);
    let bytes = limits["bytes"]
        .as_u64()?
        .saturating_sub(allocation["bytes_upper_bound"].as_u64()?);
    let timeout_seconds = limits["timeout_seconds"]
        .as_u64()?
        .saturating_sub(used["elapsed_ms"].as_u64()?.div_ceil(1000));
    let pages = limits["pages_per_connection"].as_u64()?.saturating_sub(
        allocation["pages_by_connection"]
            .as_object()?
            .values()
            .filter_map(Value::as_u64)
            .max()
            .unwrap_or(0),
    );
    (requests >= 2 && bytes > 0 && timeout_seconds > 0 && pages > 0).then_some(DiscoveryBudget {
        requests,
        bytes,
        timeout_seconds,
        pages,
    })
}

pub struct CliPlanner {
    initial: Planner,
    expanded: tokio::sync::OnceCell<Planner>,
    approved_catalogues: bool,
}
impl RouteResolver for CliPlanner {
    type Plan = engarg::route_planning::RoutePlan;
    fn preview(&self, plan: &Self::Plan) -> Result<Value, Error> {
        self.initial.preview(plan)
    }
    async fn resolve(
        &self,
        case: &ArgumentCase,
        elaboration: &SupportElaboration,
    ) -> Result<Self::Plan, Error> {
        if let Some(expanded) = self.expanded.get() {
            return expanded.resolve(case, elaboration).await;
        }
        let initial = self.initial.resolve(case, elaboration).await?;
        if !self.approved_catalogues
            || initial
                .routes
                .iter()
                .all(|route| route.selected.is_some() && route.configuration_gaps.is_empty())
        {
            return Ok(initial);
        }
        let Some(budget) = remaining(&self.initial.discovery) else {
            let mut pending = initial;
            pending.discovery.incomplete = true;
            pending.gaps.retain(|gap| {
                !matches!(
                    gap.kind,
                    engarg::route_planning::GapKind::OperationImplementation
                        | engarg::route_planning::GapKind::TransformationImplementation
                )
            });
            pending.gaps.push(engarg::route_planning::PlanningGap{acquisition_id:None,requirement_id:None,kind:engarg::route_planning::GapKind::DiscoveryIncomplete,description:"Configured discovery exhausted the aggregate boundary; approved catalogue discovery was deferred without additional IO.".into(),implementation:None});
            return Ok(pending);
        };
        let expanded = self
            .expanded
            .get_or_try_init(|| async {
                let mut expanded =
                    discover(self.initial.config.clone(), false, Some(budget)).await?;
                for key in [
                    "requests",
                    "request_bytes",
                    "response_bytes",
                    "elapsed_ms",
                    "server_starts",
                ] {
                    let count = expanded.discovery.metrics[key]
                        .as_u64()
                        .unwrap_or(0)
                        .saturating_add(self.initial.discovery.metrics[key].as_u64().unwrap_or(0));
                    expanded.discovery.metrics[key] = Value::from(count);
                }
                expanded.discovery.metrics["phases"] =
                    serde_json::json!(["configured", "expanded_for_unresolved_operations"]);
                expanded.discovery.limits = self.initial.discovery.limits.clone();
                for key in ["requests_upper_bound", "bytes_upper_bound"] {
                    expanded.discovery.budget_usage[key] = Value::from(
                        expanded.discovery.budget_usage[key]
                            .as_u64()
                            .unwrap_or(0)
                            .saturating_add(
                                self.initial.discovery.budget_usage[key]
                                    .as_u64()
                                    .unwrap_or(0),
                            ),
                    );
                }
                if let Some(pages) =
                    self.initial.discovery.budget_usage["pages_by_connection"].as_object()
                {
                    for (connection, count) in pages {
                        let total = count.as_u64().unwrap_or(0).saturating_add(
                            expanded.discovery.budget_usage["pages_by_connection"][connection]
                                .as_u64()
                                .unwrap_or(0),
                        );
                        expanded.discovery.budget_usage["pages_by_connection"][connection] =
                            Value::from(total);
                    }
                }
                Ok::<_, Error>(expanded)
            })
            .await?;
        expanded.resolve(case, elaboration).await
    }
}

pub async fn admit(path: &Path) -> Result<CliPlanner, Error> {
    let mut config: PlanningConfig = super::read_json(path, 4 * 1_048_576)?;
    config.validate()?;
    let base = path
        .canonicalize()?
        .parent()
        .ok_or_else(|| Error::InvalidInput("Planning configuration has no directory".into()))?
        .to_owned();
    for path in [
        &mut config.collector,
        &mut config.connections,
        &mut config.store,
    ] {
        if !path.is_absolute() {
            *path = base.join(&*path);
        }
    }
    config.collector = config
        .collector
        .canonicalize()
        .map_err(|_| Error::InvalidInput("evidence collector executable is unavailable".into()))?;
    config.connections = config.connections.canonicalize()?;
    let connections: Value = super::read_json(&config.connections, 16 * 1_048_576)?;
    let approved_catalogues = connections
        .get("approved_catalogues")
        .and_then(Value::as_array)
        .is_some_and(|catalogues| !catalogues.is_empty());
    Ok(CliPlanner {
        initial: discover(config, true, None).await?,
        expanded: tokio::sync::OnceCell::new(),
        approved_catalogues,
    })
}

struct CancelDiscovery(eal_process::CancellationToken);
impl Drop for CancelDiscovery {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

async fn discover(
    config: PlanningConfig,
    configured_only: bool,
    budget: Option<DiscoveryBudget>,
) -> Result<Planner, Error> {
    let mut environment = BTreeMap::new();
    for name in &config.environment {
        if name.is_empty()
            || !name.bytes().enumerate().all(|(i, ch)| {
                ch.is_ascii_alphabetic() || ch == b'_' || (i > 0 && ch.is_ascii_digit())
            })
        {
            return Err(Error::InvalidInput(
                "Planning environment entries must be variable names".into(),
            ));
        }
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.clone(), value);
        }
    }
    let supervisor = super::tooling_commands::supervisor()?;
    let mut argv = vec![
        config.collector.to_string_lossy().into_owned(),
        "evidence-discover".into(),
        "--config".into(),
        config.connections.to_string_lossy().into_owned(),
    ];
    if configured_only {
        argv.push("--configured-only".into());
    }
    if let Some(budget) = budget {
        for (flag, value) in [
            ("--request-budget", budget.requests),
            ("--byte-budget", budget.bytes),
            ("--timeout-secs", budget.timeout_seconds),
            ("--pages-per-connection", budget.pages),
        ] {
            argv.push(flag.into());
            argv.push(value.to_string());
        }
    }
    let request = eal_process::CommandRequest {
        argv,
        environment,
        cwd: config
            .connections
            .parent()
            .ok_or_else(|| {
                Error::InvalidInput("Connections configuration has no directory".into())
            })?
            .to_owned(),
        input: vec![],
        timeout: Duration::from_secs(120),
        max_output_bytes: 4 * 1_048_576 + 65536,
    };
    let cancellation = eal_process::CancellationToken::default();
    let _guard = CancelDiscovery(cancellation.clone());
    let outcome = tokio::task::spawn_blocking(move || {
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            request,
            None,
            &cancellation,
            eal_process::StreamLimits {
                max_stdout_bytes: 4 * 1_048_576,
                max_stderr_bytes: 65536,
            },
        )
    })
    .await
    .map_err(|_| Error::Provider("Evidence discovery worker failed".into()))?
    .map_err(|_| Error::Provider("Evidence discovery process failed".into()))?;
    if outcome.returncode != 0 || outcome.execution_error.is_some() {
        return Err(Error::Provider("Evidence discovery failed; check the operator-selected collector/configuration. Completed argument calls remain in the audit.".into()));
    }
    let value = eal_collector_contracts::strict_json_exact(
        &outcome.stdout,
        eal_collector_contracts::JsonLimits {
            max_bytes: 4 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    let report: DiscoveryReport = serde_json::from_value(value)?;
    if report.schema != "eal.evidence-discovery/1" || report.interfaces.len() > 4096 {
        return Err(Error::InvalidInput(
            "Unsupported evidence discovery snapshot".into(),
        ));
    }
    report.catalogue.validate().map_err(Error::InvalidInput)?;
    Ok(Planner {
        config,
        catalogue: report.catalogue,
        discovery: DiscoverySnapshot {
            budget_usage: report.budget_usage,
            limits: report.limits,
            admitted_capabilities: report.admitted_capabilities,
            incomplete: report.incomplete,
            gaps: report.gaps,
            metrics: report.metrics,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn failed_io_reservations_prevent_a_second_discovery_budget() {
        let snapshot = DiscoverySnapshot {
            limits: json!({"requests":64,"bytes":4194304,"timeout_seconds":60,"pages_per_connection":16}),
            budget_usage: json!({"requests_upper_bound":64,"bytes_upper_bound":4194304,"pages_by_connection":{"unavailable":16}}),
            metrics: json!({"requests":0,"request_bytes":0,"response_bytes":0,"elapsed_ms":1}),
            ..Default::default()
        };
        assert!(remaining(&snapshot).is_none());
    }
    #[test]
    fn many_connections_do_not_consume_each_others_page_allowance() {
        let pages = (0..12)
            .map(|i| (format!("connection-{i}"), json!(1)))
            .collect::<BTreeMap<_, _>>();
        let snapshot = DiscoverySnapshot {
            limits: json!({"requests":64,"bytes":4194304,"timeout_seconds":60,"pages_per_connection":16}),
            budget_usage: json!({"requests_upper_bound":24,"bytes_upper_bound":1000,"pages_by_connection":pages}),
            metrics: json!({"elapsed_ms":2}),
            ..Default::default()
        };
        let budget = remaining(&snapshot).unwrap();
        assert_eq!(budget.requests, 40);
        assert_eq!(budget.pages, 15);
        assert_eq!(budget.bytes, 4193304);
    }
}
