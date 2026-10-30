//! The timer over MCP, from a real rmcp client: the way an agent uses it.
//!
//! What this pins is the timer's half of the wiring:
//!
//! - over HTTP, each external device has a timer of its own — one device's
//!   run is invisible to another;
//! - a client that opens `subscriptions/listen` on the active-timer resource
//!   hears every step change and reads what happened;
//! - every run reaches the workspace's logs through the app server;
//! - the stdio subcommand serves a timer of its own too, presets from the
//!   running server.
//!
//! One test: `TIMER_CTX`'s directories are first-writer-wins per process.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, ProtocolVersion, ReadResourceRequestParams, ResourceContents,
    ServerNotification, SubscriptionFilter,
};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt as _, RoleClient, RunningService};
use rmcp::transport::{
    StreamableHttpClientTransport, TokioChildProcess,
    streamable_http_client::StreamableHttpClientTransportConfig,
};
use sapphire_framework::backend::protocol as proto;
use sapphire_framework::ipc::{self, ClientInfo, Endpoint};
use sapphire_framework::keys::{AuthConfig, Authenticated, Verdict, Verifier};
use sapphire_framework::server::{HttpConfig, HttpMcpConfig, ServerConfig};
use sapphire_timer_core::{AppKind, TIMER_CTX};
use sapphire_timer_mcp::ACTIVE_TIMER_URI;
use serde_json::{Value, json};

/// Two external devices, one token each.
struct TwoDevices;

#[async_trait::async_trait]
impl Verifier for TwoDevices {
    async fn verify(&self, token: &str) -> Verdict {
        let (id, name) = match token {
            "token-a" => ("012atva", "device-a"),
            "token-b" => ("012atvb", "device-b"),
            _ => return Verdict::Refused,
        };
        Verdict::Accepted(Authenticated {
            id: id.parse().unwrap(),
            name: name.into(),
        })
    }
}

type Client = RunningService<RoleClient, ()>;

async fn wait_until_listening(endpoint: &Endpoint) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !ipc::probe(endpoint).await.unwrap() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the server never started listening"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn ipc_client(endpoint: &Endpoint) -> ipc::Client {
    let info = ClientInfo {
        kind: "test".into(),
        version: "0.0.0".into(),
        api: proto::API_VERSION,
        app_api: None,
        pid: std::process::id(),
    };
    ipc::connect_or_absent(endpoint, TIMER_CTX.app_name, info)
        .await
        .unwrap()
        .expect("the server is listening")
        .0
}

/// The 2026-07-28 lifecycle. rmcp's plain `serve` still opens with the
/// legacy `initialize` handshake, and a legacy session has no
/// `subscriptions/listen`.
fn modern() -> ClientLifecycleMode {
    ClientLifecycleMode::Discover {
        preferred_versions: vec![ProtocolVersion::LATEST],
    }
}

async fn http_client(addr: SocketAddr, token: &str) -> Client {
    let config = StreamableHttpClientTransportConfig::with_uri(format!("http://{addr}/mcp"))
        .auth_header(token);
    ().serve_with_lifecycle(StreamableHttpClientTransport::from_config(config), modern())
        .await
        .expect("the MCP client connects")
}

/// Call `tool` and hand back its text, or its error text as `Err`.
async fn call(client: &Client, tool: &str, args: Value) -> Result<Value, String> {
    let params = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(args.as_object().cloned().unwrap_or_default());
    let result = client
        .call_tool(params)
        .await
        .expect("the call reaches the server");
    let content = serde_json::to_value(&result.content[0]).unwrap();
    let text = content["text"].as_str().unwrap().to_owned();
    if result.is_error == Some(true) {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

async fn active(client: &Client) -> Value {
    let result = client
        .read_resource(ReadResourceRequestParams::new(ACTIVE_TIMER_URI))
        .await
        .unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &result.contents[0] else {
        panic!("the active timer is text");
    };
    serde_json::from_str(text).unwrap()
}

/// Follow `client`'s timer through its subscription until the run finishes,
/// collecting each event's kind.
async fn follow_to_the_end(
    client: &Client,
    subscription: &mut rmcp::service::Subscription,
) -> Vec<String> {
    let mut kinds = Vec::new();
    loop {
        let note = tokio::time::timeout(Duration::from_secs(20), subscription.next())
            .await
            .expect("an update arrives in time")
            .expect("the subscription is alive")
            .expect("the subscription has not ended");
        let ServerNotification::ResourceUpdatedNotification(update) = note else {
            continue;
        };
        assert_eq!(update.params.uri, ACTIVE_TIMER_URI);
        let state = active(client).await;
        let kind = state["last_event"]["kind"].as_str().unwrap().to_owned();
        kinds.push(kind.clone());
        if kind == "finished" {
            assert!(state.get("run").is_none(), "{state}");
            return kinds;
        }
    }
}

fn timer_filter() -> SubscriptionFilter {
    SubscriptionFilter::builder()
        .resource_subscription(ACTIVE_TIMER_URI)
        .build()
}

#[tokio::test(flavor = "multi_thread")]
async fn each_client_hears_its_own_timer_and_every_run_is_logged() {
    let tmp = tempfile::tempdir().unwrap();
    for cat in ["CACHE", "DATA", "CONFIG"] {
        // SAFETY: the only test in this binary, before any thread reads them.
        unsafe {
            std::env::set_var(
                format!("SAPPHIRE_TIMER_{cat}_DIR"),
                tmp.path().join(cat.to_lowercase()),
            )
        };
    }
    sapphire_timer_core::init_app_context(AppKind::Server);

    let run_dir = tmp.path().join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let endpoint = Endpoint::in_dir(TIMER_CTX.app_name, run_dir.clone());
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let server = sapphire_timer_server::server()
        .config(ServerConfig {
            http: HttpConfig {
                listen: Some(addr),
                mcp: HttpMcpConfig { enabled: true },
                ..Default::default()
            },
            ..Default::default()
        })
        .http_auth(AuthConfig::new(Arc::new(TwoDevices)))
        .endpoint(endpoint.clone());
    let task = tokio::spawn(async move { server.run().await.unwrap() });
    wait_until_listening(&endpoint).await;

    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let _: proto::WorkspaceInitResult = ipc_client(&endpoint)
        .await
        .call(
            proto::WORKSPACE_INIT,
            proto::WorkspaceInitParams {
                dir: workspace.clone(),
            },
        )
        .await
        .unwrap();

    let a = http_client(addr, "token-a").await;
    let b = http_client(addr, "token-b").await;

    // A preset fast enough to run through: 0.3 s per step, two cycles.
    let preset = call(
        &a,
        "preset_add",
        json!({
            "name": "Quick",
            "cycles": 2,
            "steps": [{"label": "Focus", "minutes": 0.005}, {"label": "Break", "minutes": 0.005}],
        }),
    )
    .await
    .unwrap();
    assert_eq!(preset["name"], "Quick");

    let mut a_updates = a.peer().listen(timer_filter()).await.unwrap();
    assert_eq!(a_updates.acknowledged(), &timer_filter());
    let mut b_updates = b.peer().listen(timer_filter()).await.unwrap();

    let started = call(
        &a,
        "timer_start_preset",
        json!({"preset": "quick", "comment": "framework migration PR"}),
    )
    .await
    .unwrap();
    assert_eq!(started["resource"], ACTIVE_TIMER_URI);
    let run_id = started["run"]["id"].as_str().unwrap().to_owned();

    // B sees nothing of A's run.
    let b_status = call(&b, "timer_status", json!({})).await.unwrap();
    assert!(b_status.get("run").is_none(), "{b_status}");

    let kinds = follow_to_the_end(&a, &mut a_updates).await;
    assert_eq!(
        kinds,
        [
            "started",
            "step_changed",
            "step_changed",
            "step_changed",
            "finished"
        ]
    );
    // Nor did B hear any of it.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), b_updates.next())
            .await
            .is_err(),
        "device B heard device A's timer"
    );

    // The run's log reaches the workspace; the journal writes after the
    // event, so give it a moment.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let log = loop {
        let log = call(&b, "log_get", json!({"id": run_id})).await;
        if let Ok(log) = log.as_ref()
            && log["outcome"] == "completed"
        {
            break log.clone();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the log never completed: {log:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(log["client"], "device-a");
    assert_eq!(log["preset"], "Quick");
    assert_eq!(log["steps"].as_array().unwrap().len(), 4);
    // A Markdown file with front matter, the comment its body.
    let text = std::fs::read_to_string(workspace.join("logs").join(format!("{run_id}.md")))
        .expect("the log is a file in the workspace");
    assert!(text.starts_with("---\n"), "{text}");
    assert!(
        text.trim_end().ends_with("framework migration PR"),
        "{text}"
    );

    // A cancel is heard, and logged as one.
    let single = call(&b, "timer_start", json!({"minutes": 10, "label": "Tea"}))
        .await
        .unwrap();
    let cancelled = call(&b, "timer_cancel", json!({})).await.unwrap();
    assert_eq!(cancelled["id"], single["run"]["id"]);
    // started, then cancelled — or only the latter if the two coalesced.
    let mut last = String::new();
    while last != "cancelled" {
        tokio::time::timeout(Duration::from_secs(10), b_updates.next())
            .await
            .expect("B hears its own timer")
            .unwrap()
            .unwrap();
        last = active(&b).await["last_event"]["kind"]
            .as_str()
            .unwrap()
            .to_owned();
    }

    // The stdio subcommand: a timer of its own, the presets from the server.
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sapphire-timer"));
    command.arg("mcp").env(ipc::RUNTIME_DIR_ENV, &run_dir);
    for cat in ["CACHE", "DATA", "CONFIG"] {
        command.env(
            format!("SAPPHIRE_TIMER_{cat}_DIR"),
            tmp.path().join(cat.to_lowercase()),
        );
    }
    let local: Client =
        ().serve_with_lifecycle(TokioChildProcess::new(command).unwrap(), modern())
            .await
            .expect("the stdio server starts");
    let status = call(&local, "timer_status", json!({})).await.unwrap();
    assert!(status.get("run").is_none(), "{status}");
    let mut local_updates = local.peer().listen(timer_filter()).await.unwrap();
    call(
        &local,
        "timer_start_preset",
        json!({"preset": "Quick", "cycles": 1}),
    )
    .await
    .unwrap();
    let kinds = follow_to_the_end(&local, &mut local_updates).await;
    assert_eq!(kinds, ["started", "step_changed", "finished"]);
    let logs = call(&local, "log_list", json!({})).await.unwrap();
    let clients: Vec<&str> = logs
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["client"].as_str())
        .collect();
    assert!(clients.contains(&"device-a"), "{logs}");

    drop(local_updates);
    let _ = local.cancel().await;
    drop((a_updates, b_updates));
    let _ = a.cancel().await;
    let _ = b.cancel().await;

    let _: Value = ipc_client(&endpoint)
        .await
        .call(ipc::SHUTDOWN_METHOD, json!({}))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the server stopped")
        .unwrap();
}
