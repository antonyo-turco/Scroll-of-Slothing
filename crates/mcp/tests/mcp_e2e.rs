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

#[tokio::test(flavor = "multi_thread")]
async fn token_workflow_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let svc = Service::open(dir.path().join("data")).unwrap();
    let png = dir.path().join("Hobgoblin.png");
    image::RgbaImage::from_fn(300, 200, |x, _| image::Rgba([(x % 255) as u8, 40, 90, 255]))
        .save(&png)
        .unwrap();
    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        SleuthServer::new(svc).serve(server_io).await.unwrap().waiting().await.unwrap();
    });
    let client = Client.serve(client_io).await.unwrap();
    let call = |name: &'static str, a: Value| {
        let client = &client;
        async move {
            client
                .call_tool(CallToolRequestParams::new(name).with_arguments(args(a)))
                .await
                .unwrap()
        }
    };

    let r = call("token_borders_list", json!({})).await;
    assert_eq!(json_of(&r)[0]["id"], "builtin:roll20");

    let out_png = dir.path().join("hob_token.png");
    let r = call(
        "token_compose",
        json!({"source_path": png.to_string_lossy(), "zoom": 1.4, "background": "#203040", "size": "Large",
               "save_to_library": true, "add_to_sheet": true, "out_png": out_png.to_string_lossy()}),
    )
    .await;
    assert_ne!(r.is_error, Some(true), "{:?}", r.content);
    let summary = json_of(&r);
    assert_eq!(summary["name"], "Hobgoblin");
    assert!(summary["asset_id"].is_i64());
    assert_eq!(summary["sheet_items"], 1);
    assert!(r.content[1].as_image().is_some(), "the model gets to see the token");
    assert!(out_png.exists());

    // Re-edit from the library keeps the saved framing.
    let r =
        call("token_compose", json!({"asset_id": summary["asset_id"], "ring_inset_pct": 20})).await;
    let again = json_of(&r);
    assert_eq!(again["params"]["zoom"], 1.4);
    assert_eq!(again["params"]["bg"], json!([32, 48, 64]));

    let r = call("token_sheet_list", json!({})).await;
    let item_id = json_of(&r)["items"][0]["id"].as_str().unwrap().to_string();
    call("token_sheet_update", json!({"id": item_id, "copies": 12})).await;
    call("token_sheet_set_options", json!({"format": "A4", "cut_border": true})).await;
    let r = call("token_sheet_layout", json!({})).await;
    let layout = json_of(&r);
    assert_eq!(layout["pages"][0].as_array().unwrap().len(), 12);
    assert_eq!(layout["pages"][0][0]["w"], 144.0, "Large = 2 in");

    let pdf = dir.path().join("sheet.pdf");
    let r = call("token_sheet_export_pdf", json!({"path": pdf.to_string_lossy()})).await;
    let job = json_of(&r)["job_id"].as_str().unwrap().to_string();
    let mut state = Value::Null;
    for _ in 0..400 {
        let r = call("jobs_status", json!({"job_id": job})).await;
        state = json_of(&r)["state"].clone();
        if state != "running" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(state, "done");
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));

    let r = call("token_sheet_set_options", json!({"format": "Legal"})).await;
    assert_eq!(r.is_error, Some(true));
    client.cancel().await.unwrap();
}
