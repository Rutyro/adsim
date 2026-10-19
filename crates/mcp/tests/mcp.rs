use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

struct Mcp {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Mcp {
    async fn start(args: &[&str]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(adsim_server::serve(listener));

        let mut child = Command::new(env!("CARGO_BIN_EXE_adsim-mcp"))
            .arg("--ads")
            .arg(addr.to_string())
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn adsim-mcp");

        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");

        let mut mcp = Self {
            _child: child,
            stdin,
            lines: BufReader::new(stdout).lines(),
        };
        let response = mcp
            .request(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "adsim-test", "version": "0.1.0"}
                }),
            )
            .await;
        assert_eq!(response["result"]["serverInfo"]["name"], "adsim-mcp");
        mcp.notify("notifications/initialized").await;
        mcp
    }

    async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .expect("write request");

        loop {
            let line = self
                .lines
                .next_line()
                .await
                .expect("read response")
                .expect("mcp closed");
            let message: Value = serde_json::from_str(&line).expect("json response");
            if message.get("id") == Some(&json!(id)) {
                return message;
            }
        }
    }

    async fn notify(&mut self, method: &str) {
        let line = json!({"jsonrpc": "2.0", "method": method});
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .expect("write notification");
    }

    async fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        let response = self
            .request(
                id,
                "tools/call",
                json!({"name": name, "arguments": arguments}),
            )
            .await;
        assert!(
            response.get("error").is_none(),
            "protocol error: {response}"
        );
        let result = &response["result"];
        if result["isError"] == json!(true) {
            panic!("tool error: {result}");
        }
        payload(result)
    }
}

fn payload(result: &Value) -> Value {
    let structured = &result["structuredContent"];
    if !structured.is_null() && structured != &json!({}) {
        return structured.clone();
    }
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no tool payload: {result}"));
    serde_json::from_str(text).unwrap_or_else(|_| json!(text))
}

#[tokio::test]
async fn lists_and_reads_symbols() {
    let mut mcp = Mcp::start(&[]).await;

    let response = mcp.request(2, "tools/list", json!({})).await;
    let names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    for expected in [
        "list_symbols",
        "read_symbol",
        "write_symbol",
        "wait_for_symbol",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }

    let symbols = mcp.call(3, "list_symbols", json!({})).await;
    assert_eq!(symbols.as_array().expect("symbol array").len(), 18);

    let value = mcp
        .call(4, "read_symbol", json!({"name": "MAIN.stAxisX.fActPos"}))
        .await;
    assert_eq!(value["value"], json!(0.0));
}

#[tokio::test]
async fn drives_the_axis_and_reports_the_alarm() {
    let mut mcp = Mcp::start(&[]).await;

    for (id, name, value) in [
        (10, "MAIN.stAxisX.bEnabled", json!(true)),
        (11, "MAIN.stAxisX.bHomed", json!(true)),
        (12, "MAIN.stAxisX.fSetPos", json!(100.0)),
    ] {
        mcp.call(id, "write_symbol", json!({"name": name, "value": value}))
            .await;
    }

    let reached = mcp
        .call(
            13,
            "wait_for_symbol",
            json!({"name": "MAIN.stAxisX.fActPos", "value": 100.0, "timeout_ms": 5000}),
        )
        .await;
    assert_eq!(reached["reached"], json!(true));

    mcp.call(
        14,
        "write_symbol",
        json!({"name": "MAIN.stAxisX.fSetPos", "value": 400.0}),
    )
    .await;

    let alarm = mcp
        .call(
            15,
            "wait_for_symbol",
            json!({"name": "MAIN.nAlarmCode", "value": 1001, "timeout_ms": 5000}),
        )
        .await;
    assert_eq!(alarm["reached"], json!(true));

    mcp.call(
        16,
        "write_symbol",
        json!({"name": "MAIN.bReset", "value": true}),
    )
    .await;
    mcp.call(
        17,
        "write_symbol",
        json!({"name": "MAIN.stAxisX.fSetPos", "value": 10.0}),
    )
    .await;

    let cleared = mcp
        .call(
            18,
            "wait_for_symbol",
            json!({"name": "MAIN.bAlarm", "value": false, "timeout_ms": 5000}),
        )
        .await;
    assert_eq!(cleared["reached"], json!(true));
}

#[tokio::test]
async fn a_real_target_only_writes_allowlisted_symbols() {
    let mut mcp = Mcp::start(&["--real", "127.0.0.1.1.1", "--write", "MAIN.stAxisX.fSetPos"]).await;

    let response = mcp
        .request(
            20,
            "tools/call",
            json!({
                "name": "write_symbol",
                "arguments": {"name": "MAIN.stAxisX.bEnabled", "value": true}
            }),
        )
        .await;
    assert_eq!(response["result"]["isError"], json!(true));

    let allowed = mcp
        .call(
            21,
            "write_symbol",
            json!({"name": "MAIN.stAxisX.fSetPos", "value": 42.0}),
        )
        .await;
    assert_eq!(allowed["name"], "MAIN.stAxisX.fSetPos");
}
