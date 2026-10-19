//! MCP server that drives an ADS target through adsim-client.
//!
//! The simulator is the default target. A real target requires
//! `--real <AmsNetId>` and only symbols named with `--write` may be written.

use adsim_client::AdsClient;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    pub ads: String,
    pub real: Option<String>,
    pub write_allowlist: Vec<String>,
}

impl Config {
    pub fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            ads: "127.0.0.1:48898".to_string(),
            real: None,
            write_allowlist: Vec::new(),
        };
        let mut args = args.peekable();

        while let Some(arg) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "--ads" => config.ads = value()?,
                "--real" => config.real = Some(value()?),
                "--write" => config.write_allowlist.push(value()?),
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        if let Some(real) = &config.real {
            parse_net_id(real)?;
        }
        Ok(config)
    }

    pub fn target_net_id(&self) -> [u8; 6] {
        parse_net_id(self.real.as_deref().unwrap_or("127.0.0.1.1.1")).expect("validated NetId")
    }
}

fn parse_net_id(text: &str) -> Result<[u8; 6], String> {
    let mut net_id = [0u8; 6];
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 6 {
        return Err(format!("AMS NetId needs 6 octets: {text}"));
    }
    for (slot, part) in net_id.iter_mut().zip(parts) {
        *slot = part
            .parse()
            .map_err(|_| format!("bad AMS NetId octet: {part}"))?;
    }
    Ok(net_id)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParams {
    /// Symbol name, for example MAIN.stAxisX.fActPos.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Symbol name, for example MAIN.stAxisX.fSetPos.
    pub name: String,
    /// Value as JSON: true/false, number or byte array.
    pub value: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WaitParams {
    /// Symbol name, for example MAIN.stAxisX.fActPos.
    pub name: String,
    /// Value to wait for, as JSON.
    pub value: Value,
    /// Maximum wait in milliseconds (default 5000).
    pub timeout_ms: Option<u64>,
}

pub struct AdsimMcp {
    client: Mutex<AdsClient>,
    real: bool,
    write_allowlist: Vec<String>,
}

impl AdsimMcp {
    pub fn new(client: AdsClient, config: Config) -> Self {
        Self {
            client: Mutex::new(client),
            real: config.real.is_some(),
            write_allowlist: config.write_allowlist,
        }
    }

    fn writes_allowed(&self, name: &str) -> bool {
        !self.real || self.write_allowlist.iter().any(|allowed| allowed == name)
    }
}

#[tool_router]
impl AdsimMcp {
    #[tool(
        name = "list_symbols",
        description = "List PLC symbols with their ADS type and size.",
        annotations(read_only_hint = true)
    )]
    async fn list_symbols(&self) -> CallToolResult {
        let client = self.client.lock().await;
        let symbols: Vec<Value> = client
            .symbols()
            .iter()
            .map(|symbol| {
                json!({
                    "name": symbol.name,
                    "type": symbol.type_name,
                    "size": symbol.size,
                })
            })
            .collect();
        success(json!(symbols))
    }

    #[tool(
        name = "read_symbol",
        description = "Read one PLC symbol as JSON.",
        annotations(read_only_hint = true)
    )]
    async fn read_symbol(&self, Parameters(params): Parameters<SymbolParams>) -> CallToolResult {
        let mut client = self.client.lock().await;
        match client.read_symbol(&params.name).await {
            Ok(value) => success(json!({"name": params.name, "value": value})),
            Err(error) => failure(error),
        }
    }

    #[tool(
        name = "write_symbol",
        description = "Write one PLC symbol. A real target only accepts symbols on the --write allowlist."
    )]
    async fn write_symbol(&self, Parameters(params): Parameters<WriteParams>) -> CallToolResult {
        if !self.writes_allowed(&params.name) {
            return failure(format!("write denied by allowlist: {}", params.name));
        }

        let mut client = self.client.lock().await;
        match client.write_symbol(&params.name, &params.value).await {
            Ok(()) => success(json!({"name": params.name, "value": params.value, "written": true})),
            Err(error) => failure(error),
        }
    }

    #[tool(
        name = "wait_for_symbol",
        description = "Poll a PLC symbol until it equals the expected value or the timeout expires.",
        annotations(read_only_hint = true)
    )]
    async fn wait_for_symbol(&self, Parameters(params): Parameters<WaitParams>) -> CallToolResult {
        let mut client = self.client.lock().await;
        match client
            .wait_for_symbol(
                &params.name,
                &params.value,
                params.timeout_ms.unwrap_or(5000),
            )
            .await
        {
            Ok((value, reached)) => {
                success(json!({"name": params.name, "value": value, "reached": reached}))
            }
            Err(error) => failure(error),
        }
    }
}

#[tool_handler(
    name = "adsim-mcp",
    version = "0.1.0",
    instructions = "Drives a PLC over ADS. Defaults to the adsim simulator; a real target needs --real and an explicit write allowlist."
)]
impl ServerHandler for AdsimMcp {}

fn success(value: Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}

fn failure(error: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(error.to_string())])
}
