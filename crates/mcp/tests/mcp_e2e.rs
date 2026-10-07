//! End-to-end: a real MCP client talks to the server over an in-memory duplex
//! pipe, exactly like Claude would over stdio.

use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig};
use rmcp::{ClientHandler, ServiceExt};
use serde_json::{Value, json};
use sleuth_mcp::SleuthServer;
use sleuth_service::Service;

#[derive(Clone, Default)]
struct Client;
impl ClientHandler for Client {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::default()
    }
}

fn args(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn json_of(r: &CallToolResult) -> Value {
    let text =
        r.content.first().and_then(|c| c.as_text()).map(|t| t.text.clone()).expect("text content");
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

#[tokio::test(flavor = "multi_thread")]
async fn library_workflow_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let svc = Service::open(dir.path().join("data")).unwrap();
    let png = dir.path().join("Owlbear.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([120, 80, 40, 255])).save(&png).unwrap();

    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        SleuthServer::new(svc).serve(server_io).await.unwrap().waiting().await.unwrap();
    });
    let client = Client.serve(client_io).await.unwrap();

    let tools: Vec<String> =
        client.list_all_tools().await.unwrap().into_iter().map(|t| t.name.to_string()).collect();
    for expected in
        ["library_search", "library_import", "tags_rename", "sessions_set_assets", "legacy_import"]
    {
        assert!(tools.contains(&expected.to_string()), "missing tool {expected}");
    }

    let call = |name: &'static str, a: Value| {
        let client = &client;
        async move {
            client
                .call_tool(CallToolRequestParams::new(name).with_arguments(args(a)))
                .await
                .unwrap()
        }
    };

    let r = call(
        "library_import",
        json!({"files": [{"path": png.to_string_lossy(), "kind": "token", "size": "Large"}]}),
    )
    .await;
    let id = json_of(&r)["imported"][0].as_i64().expect("imported id");

    let r = call("library_set_tags", json!({"id": id, "values": ["beast", "forest"]})).await;
    assert_eq!(json_of(&r)["tags"], json!(["beast", "forest"]));

    let r = call("library_search", json!({"q": "owl", "tags": ["beast"]})).await;
    let found = json_of(&r);
    assert_eq!(found["total"], 1);
    assert_eq!(found["items"][0]["size_label"], "Large");

    // library_get returns metadata + a thumbnail the model can look at.
    let r = call("library_get", json!({"id": id})).await;
    assert_eq!(r.content.len(), 2);
    assert!(r.content[1].as_image().is_some(), "thumbnail image attached");

    let r = call("campaigns_create", json!({"name": "Wild Sheep Chase"})).await;
    let cid = json_of(&r)["id"].as_str().unwrap().to_string();
    let r = call("sessions_create", json!({"campaign_id": cid, "name": "Session 1"})).await;
    let sid = json_of(&r)["id"].as_str().unwrap().to_string();
    call("sessions_set_assets", json!({"id": sid, "asset_ids": [id]})).await;
    let r = call("library_search", json!({"campaign_id": cid})).await;
    assert_eq!(json_of(&r)["total"], 1, "campaign filter includes session-linked assets");

    // Service errors come back as tool errors, readable by the model.
    let r = call("library_get", json!({"id": 9999})).await;
    assert_eq!(r.is_error, Some(true));
    assert!(json_of(&r).as_str().unwrap().starts_with("not_found"));
    let r = call("library_search", json!({"sort": "sideways"})).await;
    assert_eq!(r.is_error, Some(true));

    let r =
        call("settings_set", json!({"values": {"token_page_format": "A4", "map_square_in": 0}}))
            .await;
    let up = json_of(&r);
    assert_eq!(up["settings"]["token_page_format"], "A4");
    assert!(up["errors"]["map_square_in"].is_string());

    client.cancel().await.unwrap();
}

#[test]
fn tools_are_documented() {
    let doc =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/mcp-tools.md"))
            .unwrap();
    let missing: Vec<String> = SleuthServer::tool_names()
        .into_iter()
        .filter(|t| !doc.contains(&format!("`{t}`")))
        .collect();
    assert!(missing.is_empty(), "tools missing from docs/mcp-tools.md: {missing:?}");
}
