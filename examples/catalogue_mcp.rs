//! Read-only catalogue demonstration over the official MCP SDK's stdio transport.
//!
//! This server reports a local fixture snapshot. It makes no deployment claims.
use rmcp::{ServerHandler, ServiceExt, model::*};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn input_schema() -> Value {
    json!({"type":"object","properties":{"snapshot":{"type":"string","minLength":1,"maxLength":128}},"required":["snapshot"],"additionalProperties":false})
}

#[derive(Clone)]
struct Catalogue {
    file: PathBuf,
    requests: Option<PathBuf>,
}

impl ServerHandler for Catalogue {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("engarg-catalogue-demonstration", "1"))
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult::with_all_items(vec![Tool::new(
            "read_catalogue_snapshot",
            "Read the complete retained local demonstration catalogue snapshot.",
            Arc::new(input_schema().as_object().expect("schema object").clone()),
        )]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let failure = |message: &str| rmcp::ErrorData::invalid_params(message.to_owned(), None);
        if request.name != "read_catalogue_snapshot" {
            return Err(failure("Unknown catalogue tool"));
        }
        let arguments = request
            .arguments
            .ok_or_else(|| failure("Missing arguments"))?;
        if arguments.len() != 1 || !arguments.get("snapshot").is_some_and(Value::is_string) {
            return Err(failure("Exactly one string snapshot argument is required"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&self.file)
            .map_err(|_| failure("Catalogue snapshot is unavailable"))?
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| failure("Catalogue snapshot could not be read"))?;
        let catalogue = eal_collector_contracts::strict_json_exact(
            &bytes,
            eal_collector_contracts::JsonLimits::default(),
        )
        .map_err(|_| failure("Catalogue snapshot is invalid bounded JSON"))?;
        let items = catalogue["items"]
            .as_array()
            .filter(|items| items.len() <= 32)
            .ok_or_else(|| failure("Catalogue requires at most 32 items"))?;
        if catalogue["snapshot"] != arguments["snapshot"]
            || catalogue["complete"] != true
            || items.iter().any(|item| {
                item.as_object().is_none_or(|object| object.len() != 1)
                    || item["id"]
                        .as_str()
                        .is_none_or(|id| id.is_empty() || id.chars().count() > 64)
            })
        {
            return Err(failure(
                "Snapshot identity, completeness or item shape mismatch",
            ));
        }
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| failure("Observation clock is unavailable"))?
            .as_secs();
        let observed_at = chrono::DateTime::from_timestamp(
            i64::try_from(seconds).map_err(|_| failure("Observation clock overflow"))?,
            0,
        )
        .ok_or_else(|| failure("Observation clock is invalid"))?
        .to_rfc3339();
        let response = json!({"observed_at":observed_at,"measurement":catalogue});
        if let Some(path) = &self.requests {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|_| failure("Catalogue request audit is unavailable"))?;
            writeln!(
                file,
                "{}",
                json!({"tool":"read_catalogue_snapshot","arguments":arguments,"response":response})
            )
            .map_err(|_| failure("Catalogue request audit failed"))?;
        }
        Ok(CallToolResult::structured(response).into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let file = arguments.next().map(PathBuf::from).ok_or(
        "usage: catalogue_mcp CATALOGUE.json [REQUESTS.jsonl] (stdio MCP protocol on stdout)",
    )?;
    let requests = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        return Err("Too many catalogue server arguments".into());
    }
    let server = Catalogue { file, requests }
        .serve(rmcp::transport::stdio())
        .await?;
    server.waiting().await?;
    Ok(())
}
