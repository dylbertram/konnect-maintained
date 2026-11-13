use super::*;
use crate::mcp::handler::McpHandler;
use crate::tools::pcb_board::board_mock;
use konnect_ipc::{builders::pack_any, gen::kiapi};
use prost::Message;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const NATIVE: &str = include_str!("../../tests/fixtures/footprint_children_kicad10.kicad_pcb");
const R1: &str = "20a44d6b-d6bb-4ec1-ac0c-f1d3d7e20db2";
const U1: &str = "c7901245-d0c4-476f-8215-85f1183d04de";
const LIBRARY: &str = "Capacitor_SMD:C_0603_1608Metric";
const LIBRARY_SOURCE: &str = include_str!("../../tests/fixtures/c_0603_1608metric_kicad10.kicad_mod");

/// These are SYNTHETIC in-memory IPC/native responses, not rewritten CAD files
/// or native KiCad evidence. The original fixture stays byte-for-byte intact.
fn synthetic_native(children: &str) -> String {
    format!(
        "(kicad_pcb (version 20260206)
          (variants (variant (name \"NoTimer\")))
          (footprint \"{LIBRARY}\" (uuid \"{R1}\"))
          (footprint \"{LIBRARY}\" (uuid \"{U1}\") {children})
          (group \"board only\" (uuid \"board-group\") (members \"{R1}\" \"{U1}\")))"
    )
}

fn synthetic_item(reference: &str, id: &str, value: &str) -> prost_types::Any {
    let pads = super::super::pcb_components::extract_pad_definitions(LIBRARY_SOURCE).unwrap();
    let graphics = super::super::pcb_components::extract_graphic_definitions(LIBRARY_SOURCE).unwrap();
    let fields = super::super::pcb_components::extract_field_placement(LIBRARY_SOURCE);
    let item = KiCadIpcClient::build_footprint_item(
        LIBRARY, reference, value, &pads, &graphics, &fields, 30.0, 40.0, 0.0, "F.Cu",
    ).unwrap();
    let mut instance = kiapi::board::types::FootprintInstance::decode(item.value.as_slice()).unwrap();
    instance.id = Some(kiapi::common::types::Kiid { value: id.into() });
    instance.symbol_path = Some(kiapi::common::types::SheetPath {
        path: vec![kiapi::common::types::Kiid { value: format!("{reference}-uuid") }],
        path_human_readable: "/".into(),
    });
    pack_any(&instance, "kiapi.board.types.FootprintInstance")
}

#[derive(Clone)]
struct State {
    native: String,
    items: Vec<prost_types::Any>,
    calls: Vec<String>,
    inventory_reads: usize,
}

struct Served {
    _temp: tempfile::TempDir,
    _mock: crate::test_support::MockIpcServer,
    handler: McpHandler,
    board: PathBuf,
    schematic: PathBuf,
    exported: PathBuf,
    state: Arc<Mutex<State>>,
}

impl Served {
    async fn new() -> Self {
        use crate::tools::cli::test_support::write_script;
        let temp = tempfile::tempdir().unwrap();
        let board = temp.path().join("children.kicad_pcb");
        // Verbatim import only. No synthetic CAD is ever written to this file.
        std::fs::write(&board, NATIVE.as_bytes()).unwrap();
        let schematic = temp.path().join("children.kicad_sch");
        std::fs::write(&schematic, include_bytes!(
            "../../tests/fixtures/structural_scans_kicad10.kicad_sch"
        )).unwrap();
        let library = temp.path().join("Capacitor_SMD.pretty");
        std::fs::create_dir(&library).unwrap();
        std::fs::write(library.join("C_0603_1608Metric.kicad_mod"), LIBRARY_SOURCE).unwrap();
        std::fs::write(temp.path().join("fp-lib-table"),
            "(fp_lib_table (lib (name \"Capacitor_SMD\") (type \"KiCad\") (uri \"${KIPRJMOD}/Capacitor_SMD.pretty\") (options \"\") (descr \"\")))").unwrap();
        let exported = temp.path().join("children.net");
        let unix_source = exported.to_string_lossy().replace('\'', "'\\''");
        let windows_source = exported.to_string_lossy();
        let cli = write_script(
            temp.path(), "fake-kicad-cli-a8",
            &format!("#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\nif [ \"$1\" = \"--output\" ]; then\nshift\ncp '{unix_source}' \"$1\"\nexit $?\nfi\nshift\ndone\nexit 2\n"),
            &format!("@echo off\r\n:loop\r\nif \"%~1\"==\"\" exit /b 2\r\nif \"%~1\"==\"--output\" goto found\r\nshift\r\ngoto loop\r\n:found\r\nshift\r\ncopy /Y \"{windows_source}\" \"%~1\" >nul\r\nexit /b %ERRORLEVEL%\r\n"),
        );
        let state = Arc::new(Mutex::new(State {
            native: NATIVE.into(),
            // Deliberately SYNTHETIC protobuf with no native-owned children.
            // Both synthetic definitions use the checked-in capacitor library.
            items: vec![synthetic_item("R1", R1, "before"), synthetic_item("U1", U1, "before")],
            calls: Vec::new(),
            inventory_reads: 0,
        }));
        let captured = state.clone();
        let document = board_mock::board_document(&board.to_string_lossy());
        let mock = board_mock::spawn_kicad_holding_board(&board, move |command| {
            let mut state = captured.lock().unwrap();
            let name = command.type_url.rsplit('.').next().unwrap();
            state.calls.push(name.into());
            match name {
                "GetItems" => {
                    let request = kiapi::common::commands::GetItems::decode(command.value.as_slice()).unwrap();
                    assert_eq!(request.header.unwrap().document, Some(document.clone()));
                    let items = if request.types.contains(&(kiapi::common::types::KiCadObjectType::KotPcbFootprint as i32)) {
                        state.items.clone()
                    } else { Vec::new() };
                    Some(pack_any(&kiapi::common::commands::GetItemsResponse {
                        header: None, status: kiapi::common::types::ItemRequestStatus::IrsOk as i32, items,
                    }, "kiapi.common.commands.GetItemsResponse"))
                }
                "GetNets" => Some(pack_any(&kiapi::board::commands::NetsResponse { nets: Vec::new() }, "kiapi.board.commands.NetsResponse")),
                "GetBoundingBox" => Some(board_mock::kicad_bounding_boxes(command, |_| (30.0, 40.0, 2.0, 2.0))),
                "SaveDocumentToString" => {
                    let request = kiapi::common::commands::SaveDocumentToString::decode(command.value.as_slice()).unwrap();
                    assert_eq!(request.document, Some(document.clone()), "inventory must name the bound document");
                    state.inventory_reads += 1;
                    // Empty contents exercise live-read failure, not disk fallback.
                    Some(pack_any(&kiapi::common::commands::SavedDocumentResponse {
                        document: request.document, contents: state.native.clone(),
                    }, "kiapi.common.commands.SavedDocumentResponse"))
                }
                "BeginCommit" => Some(pack_any(&kiapi::common::commands::BeginCommitResponse {
                    id: Some(kiapi::common::types::Kiid { value: "a8-commit".into() }),
                }, "kiapi.common.commands.BeginCommitResponse")),
                "UpdateItems" => {
                    let request = kiapi::common::commands::UpdateItems::decode(command.value.as_slice()).unwrap();
                    assert_eq!(request.header.unwrap().document, Some(document.clone()));
                    for item in &request.items {
                        let replacement = kiapi::board::types::FootprintInstance::decode(item.value.as_slice()).unwrap();
                        let current = state.items.iter_mut().find(|item| {
                            kiapi::board::types::FootprintInstance::decode(item.value.as_slice()).unwrap().id == replacement.id
                        }).unwrap();
                        *current = item.clone();
                    }
                    Some(pack_any(&kiapi::common::commands::UpdateItemsResponse {
                        header: None, status: kiapi::common::types::ItemRequestStatus::IrsOk as i32,
                        updated_items: request.items.into_iter().map(|item| kiapi::common::commands::ItemUpdateResult {
                            status: Some(ok_status()), item: Some(item),
                        }).collect(),
                    }, "kiapi.common.commands.UpdateItemsResponse"))
                }
                "CreateItems" => {
                    let request = kiapi::common::commands::CreateItems::decode(command.value.as_slice()).unwrap();
                    state.items.extend(request.items.iter().cloned());
                    Some(pack_any(&kiapi::common::commands::CreateItemsResponse {
                        header: None, status: kiapi::common::types::ItemRequestStatus::IrsOk as i32,
                        created_items: request.items.into_iter().map(|item| kiapi::common::commands::ItemCreationResult {
                            status: Some(ok_status()), item: Some(item),
                        }).collect(),
                    }, "kiapi.common.commands.CreateItemsResponse"))
                }
                "EndCommit" => Some(pack_any(&kiapi::common::commands::EndCommitResponse {}, "kiapi.common.commands.EndCommitResponse")),
                _ => panic!("unexpected a8 RPC: {name}"),
            }
        });
        let handler = McpHandler::new(crate::tools::ServerConfig {
            kicad_cli: cli.to_string_lossy().into(),
            ipc_address: mock.address().into(),
            eager_toolsets: true,
            ..Default::default()
        }).await.unwrap();
        Self { _temp: temp, _mock: mock, handler, board, schematic, exported, state }
    }

    async fn call(&self, refresh: bool, apply_revision: Option<&str>, references: &[&str]) -> Value {
        let mut args = json!({"board": self.board});
        let name = if refresh {
            args["references"] = json!(references);
            "update_footprints_from_library"
        } else {
            args["schematic"] = json!(self.schematic);
            let components = references.iter().map(|reference| {
                format!("(comp (ref \"{reference}\") (value \"part\") (footprint \"{LIBRARY}\") (sheetpath (names \"/\") (tstamps \"/\")) (tstamps \"{reference}-uuid\"))")
            }).collect::<String>();
            std::fs::write(&self.exported, format!("(export (components {components}) (nets))")).unwrap();
            "update_pcb_from_schematic"
        };
        if let Some(revision) = apply_revision {
            args["dry_run"] = json!(false);
            args["expected_plan_revision"] = json!(revision);
        }
        let response = self.handler.handle_message(json!({
            "jsonrpc": "2.0", "id": 836, "method": "tools/call",
            "params": { "name": name, "arguments": args },
        })).await.unwrap();
        assert!(response.error.is_none(), "{response:?}");
        let result = response.result.unwrap();
        assert_ne!(result["isError"], true, "{result:#}");
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    fn snapshot(&self) -> (String, Vec<prost_types::Any>, Vec<u8>) {
        let state = self.state.lock().unwrap();
        (state.native.clone(), state.items.clone(), std::fs::read(&self.board).unwrap())
    }

    fn assert_no_writes(&self, before: &(String, Vec<prost_types::Any>, Vec<u8>)) {
        let state = self.state.lock().unwrap();
        for mutation in ["BeginCommit", "CreateItems", "UpdateItems", "DeleteItems",
                         "EndCommit", "SaveDocument", "RevertDocument"] {
            assert!(!state.calls.iter().any(|name| name == mutation), "a8 zero mutation RPCs: {mutation}");
        }
        drop(state);
        assert_eq!(&self.snapshot(), before, "a8 unchanged native state, raw protobuf and disk");
    }
}

fn ok_status() -> kiapi::common::commands::ItemStatus {
    kiapi::common::commands::ItemStatus {
        code: kiapi::common::commands::ItemStatusCode::IscOk as i32,
        error_message: String::new(),
    }
}

fn assert_refused(result: &Value, kiid: &str, kind: Option<&str>) {
    assert_eq!(result["status"], "conflict", "a8 native children must refuse: {result:#}");
    assert_eq!(result["changes"], json!([]), "a8 whole plan cleared");
    let diagnostic = result["diagnostics"].as_array().unwrap().iter()
        .find(|d| d["code"] == DIAGNOSTIC).expect("a8 named compatibility diagnostic");
    assert_eq!(diagnostic["kiid"], kiid);
    assert_eq!(diagnostic["library_id"], LIBRARY);
    if let Some(kind) = kind {
        assert!(diagnostic["kinds"].as_array().unwrap().contains(&json!(kind)), "{diagnostic:#}");
        assert!(diagnostic["inventory_error"].is_null(), "{diagnostic:#}");
    } else {
        assert!(diagnostic["inventory_error"].is_string(), "{diagnostic:#}");
    }
}

#[test]
fn a8_native_fixture_inventory_counts_only_direct_owned_children() {
    let entries = inventory(NATIVE, &BTreeSet::from([R1, U1])).unwrap();
    assert_eq!(entries[R1], [Children::default()]);
    assert_eq!(entries[U1], [Children {
        groups: 1, points: 1, variants: 1, variant_names: vec!["NoTimer".into()],
    }]);
}

#[tokio::test]
async fn a8_served_each_kind_refuses_dry_run_and_apply_without_writes() {
    for refresh in [false, true] {
        for (kind, child) in [
            ("group", "(group \"owned\" (uuid \"owned-group\") (members \"line\"))"),
            ("point", "(point (at 1 2) (size 1.27) (layer \"F.Fab\") (uuid \"point\"))"),
            ("variant", "(variant (name \"NoTimer\") (dnp yes))"),
        ] {
            let served = Served::new().await;
            served.state.lock().unwrap().native = synthetic_native(child);
            let before = served.snapshot();
            for revision in [None, Some("not-needed-for-unsafe-plan")] {
                let result = served.call(refresh, revision, &["U1"]).await;
                assert_refused(&result, U1, Some(kind));
                let diagnostic = &result["diagnostics"][0];
                assert_eq!(diagnostic["reference"], "U1");
                if kind == "variant" {
                    assert_eq!(diagnostic["native_children"]["variant_names"], json!(["NoTimer"]));
                }
            }
            assert_eq!(served.state.lock().unwrap().inventory_reads, 2, "a8 read every request");
            served.assert_no_writes(&before);
        }
    }
}

#[tokio::test]
async fn a8_served_mixed_plan_refuses_safe_update_and_addition_too() {
    for refresh in [false, true] {
        let served = Served::new().await;
        let before = served.snapshot();
        // Refresh cannot add; sync has a safe update, safe addition, unsafe update.
        let refs: &[&str] = if refresh { &["R1", "U1"] } else { &["R1", "C1", "U1"] };
        for revision in [None, Some("unsafe-plan")] {
            let result = served.call(refresh, revision, refs).await;
            assert_refused(&result, U1, Some("group"));
            if refresh {
                assert_eq!(result["coverage"]["changed"]["planned"], 0);
            } else {
                assert_eq!(result["coverage"]["footprints_added"]["planned"], 0);
                assert_eq!(result["coverage"]["footprints_updated"]["planned"], 0);
            }
        }
        served.assert_no_writes(&before);
    }
}

#[tokio::test]
async fn a8_served_apply_rechecks_native_children_despite_unchanged_protobuf() {
    for refresh in [false, true] {
        let served = Served::new().await;
        served.state.lock().unwrap().native = synthetic_native("");
        let plan = served.call(refresh, None, &["U1"]).await;
        assert_eq!(plan["status"], "ready", "{plan:#}");
        served.state.lock().unwrap().native = NATIVE.into();
        let before = served.snapshot();
        let result = served.call(refresh, plan["plan_revision"].as_str(), &["U1"]).await;
        assert_refused(&result, U1, Some("variant"));
        assert_eq!(served.state.lock().unwrap().inventory_reads, 2, "a8 fresh apply inventory");
        served.assert_no_writes(&before);
    }
}

#[tokio::test]
async fn a8_served_unknown_inventory_fails_closed_in_both_target_orders() {
    let safe = format!("(footprint \"{LIBRARY}\" (uuid \"{R1}\"))");
    let unsafe_target = format!("(footprint \"{LIBRARY}\" (uuid \"{R1}\") (point (at 1 2)))");
    let missing = format!("(kicad_pcb (footprint \"{LIBRARY}\" (uuid \"other\")))");
    for native in [
        String::new(), "(kicad_pcb".into(), "(footprint \"wrong root\")".into(),
        "(kicad_pcb) broken(".into(), missing,
        format!("(kicad_pcb {safe} {unsafe_target})"),
        format!("(kicad_pcb {unsafe_target} {safe})"),
        format!("(kicad_pcb (footprint \"{LIBRARY}\" (uuid \"{R1}\") (uuid \"other\")))"),
        format!("(kicad_pcb (footprint \"{LIBRARY}\" (uuid \"{R1}\") (variant (dnp yes))))"),
    ] {
        for refresh in [false, true] {
            let served = Served::new().await;
            served.state.lock().unwrap().native = native.clone();
            let before = served.snapshot();
            for revision in [None, Some("untrusted-inventory")] {
                assert_refused(&served.call(refresh, revision, &["R1"]).await, R1, None);
            }
            served.assert_no_writes(&before);
        }
    }
}

#[tokio::test]
async fn a8_served_safe_r1_changes_and_reads_back_board_membership_is_allowed() {
    for refresh in [false, true] {
        let served = Served::new().await;
        let before = served.snapshot();
        let plan = served.call(refresh, None, &["R1"]).await;
        assert_eq!(plan["status"], "ready", "{plan:#}");
        assert!(served.state.lock().unwrap().calls.iter().all(|name| name != "BeginCommit"));
        let applied = served.call(refresh, plan["plan_revision"].as_str(), &["R1"]).await;
        assert_eq!(applied["status"], "applied", "{applied:#}");
        let client = KiCadIpcClient::new(served._mock.address());
        let document = client.find_open_board(&served.board).unwrap();
        let readback = client.get_items_in(document, kiapi::common::types::KiCadObjectType::KotPcbFootprint).unwrap();
        let state = served.state.lock().unwrap();
        assert_eq!(state.items, readback, "a8 independent mock state readback");
        assert_ne!(state.items[0], before.1[0], "a8 positive must actually replace R1");
        assert_eq!(state.items[1], before.1[1], "a8 unselected unsafe U1 untouched");
        if !refresh {
            let r1 = kiapi::board::types::FootprintInstance::decode(readback[0].value.as_slice()).unwrap();
            assert_eq!(r1.value_field.unwrap().text.unwrap().text.unwrap().text, "part");
        }
        assert_eq!(state.native, before.0);
        assert_eq!(std::fs::read(&served.board).unwrap(), before.2);
    }
}

#[tokio::test]
async fn a8_served_noop_and_add_only_do_not_inventory_existing_instances() {
    let sync = Served::new().await;
    sync.state.lock().unwrap().items = vec![synthetic_item("R1", R1, "part")];
    let noop = sync.call(false, None, &["R1"]).await;
    assert_eq!(noop["status"], "noop", "{noop:#}");
    assert_eq!(sync.call(false, noop["plan_revision"].as_str(), &["R1"]).await["status"], "noop");
    assert_eq!(sync.state.lock().unwrap().inventory_reads, 0, "a8 noop has no inventory");
    sync.state.lock().unwrap().items.clear();
    let addition = sync.call(false, None, &["C1"]).await;
    assert_eq!(addition["status"], "ready", "{addition:#}");
    assert_eq!(sync.state.lock().unwrap().inventory_reads, 0, "a8 add-only dry run has no inventory");
    let applied = sync.call(false, addition["plan_revision"].as_str(), &["C1"]).await;
    assert_eq!(applied["status"], "applied", "{applied:#}");
    // Sync's existing recovering-commit snapshot is not a child inventory.
    assert_eq!(sync.state.lock().unwrap().inventory_reads, 1, "a8 add-only keeps only existing recovery snapshot");

    let refresh = Served::new().await;
    let plan = refresh.call(true, None, &["R1"]).await;
    assert_eq!(plan["status"], "ready", "{plan:#}");
    assert_eq!(refresh.call(true, plan["plan_revision"].as_str(), &["R1"]).await["status"], "applied");
    refresh.state.lock().unwrap().inventory_reads = 0;
    let noop = refresh.call(true, None, &["R1"]).await;
    assert_eq!(noop["status"], "noop", "{noop:#}");
    assert_eq!(refresh.call(true, noop["plan_revision"].as_str(), &["R1"]).await["status"], "noop");
    assert_eq!(refresh.state.lock().unwrap().inventory_reads, 0, "a8 refresh noop has no inventory");
}
