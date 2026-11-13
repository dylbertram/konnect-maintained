//! Served discovery contracts for the selector and client guidance.
//! No KiCad process, tool execution, or design mutation is needed.

use konnect_core::mcp::handler::McpHandler;
use konnect_core::router::registry;
use konnect_core::tools::ServerConfig;
use serde_json::{json, Value};
use std::collections::BTreeSet;

async fn handler(eager_toolsets: bool, auto_load_toolsets: bool) -> McpHandler {
    McpHandler::new(ServerConfig {
        kicad_cli: String::new(),
        kicad_binary: String::new(),
        ipc_address: String::new(),
        eager_toolsets,
        auto_load_toolsets,
        ..ServerConfig::default()
    })
    .await
    .expect("handler builds without KiCad")
}

async fn first_listing(handler: &McpHandler) -> Vec<Value> {
    let initialized = handler
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "muse-guidance-test", "version": "0"}
            }
        }))
        .await
        .expect("initialize receives a response");
    assert!(initialized.error.is_none());
    let response = handler
        .handle_message(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .await
        .expect("tools/list receives a response");
    assert!(response.error.is_none());
    response.result.expect("listing result")["tools"]
        .as_array()
        .expect("tools array")
        .clone()
}

#[tokio::test]
async fn served_batch_delete_guidance_names_the_separate_selectors() {
    let tools = first_listing(&handler(true, false).await).await;
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "batch_delete")
        .expect("batch_delete is served");
    let description = tool["description"].as_str().unwrap();
    assert!(description.contains("UUID in `uuids`"));
    assert!(description.contains("reference designator in `references`"));

    let schema = &tool["inputSchema"];
    let properties = &schema["properties"];
    let uuids = properties["uuids"]["description"].as_str().unwrap();
    assert!(uuids.contains("references, not uuids"));
    let references = properties["references"]["description"].as_str().unwrap();
    assert!(references.contains("R1"));
    assert!(references.contains("#PWR01"));
    assert!(references.contains("UUIDs in uuids"));

    // Documentation must not change the selector types or make either required.
    assert_eq!(schema["required"], json!(["schematic"]));
    for selector in ["uuids", "references"] {
        assert_eq!(schema["properties"][selector]["type"], "array");
        assert_eq!(schema["properties"][selector]["items"]["type"], "string");
    }
}

#[tokio::test]
async fn eager_setting_serves_every_registered_tool_in_the_first_listing() {
    let tools = first_listing(&handler(true, false).await).await;
    let names: BTreeSet<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(names.len(), tools.len(), "no duplicate served names");
    for toolset in registry::ALL_TOOLSETS {
        for tool in registry::tools_for(toolset.name).expect("registered toolset") {
            assert!(
                names.contains(tool.name),
                "first listing missed {} from {}",
                tool.name,
                toolset.name
            );
        }
    }
}

#[tokio::test]
async fn auto_load_alone_does_not_make_the_first_listing_complete() {
    // Auto-load only acts on calls. It cannot give a caching client the schemas
    // it missed. Unknown client names must not trigger Claude's automatic path.
    for auto_load in [false, true] {
        let tools = first_listing(&handler(false, auto_load).await).await;
        assert!(tools.iter().any(|tool| tool["name"] == "load_toolset"));
        for name in ["batch_delete", "get_footprint_info"] {
            assert!(
                !tools.iter().any(|tool| tool["name"] == name),
                "{name} unexpectedly in the lazy listing with auto_load={auto_load}"
            );
        }
    }
}

#[test]
fn muse_guidance_contains_a_valid_setting_and_config_diagnostics() {
    let docs = include_str!("../../../docs/TROUBLESHOOTING.md");
    let discovery = docs
        .split("## Tools don't appear after `load_toolset`")
        .nth(1)
        .expect("discovery guidance")
        .split("\n## ")
        .next()
        .unwrap();
    let example = discovery
        .split("```json\n")
        .nth(1)
        .expect("JSON server setting")
        .split("```")
        .next()
        .unwrap();
    let setting: Value = serde_json::from_str(example).expect("valid JSON");
    assert_eq!(setting, json!({"eager_toolsets": true}));
    assert!(discovery.contains("`eager_toolsets = true` in `konnect.toml`"));

    let muse = docs
        .split("## Muse loads toolsets but still reports \"unknown tool\"")
        .nth(1)
        .expect("Muse troubleshooting section")
        .split("\n## ")
        .next()
        .unwrap();
    for required in [
        "eager_toolsets",
        "notifications/tools/list_changed",
        "settings.json",
        "--config <path>",
        "configuration.selected_path",
        "configuration.skipped_existing_paths",
        "new Muse session",
        "auto_load_toolsets",
        "#922",
    ] {
        assert!(muse.contains(required), "Muse guidance missing {required}");
    }
}
