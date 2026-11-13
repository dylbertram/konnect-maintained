//! Stale-save and persistence regressions through served tools/call.
//! Uses KiCad 10.0.5's serialization, including CRLF and nested pad nets.
//! Fixture provenance: tests/fixtures/specctra_two_resistors.README.md.

use super::*;
use crate::mcp::handler::McpHandler;
use crate::mcp::protocol::ToolContent;
use crate::tools::ServerConfig;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;

const NATIVE: &str = include_str!("../../tests/fixtures/specctra_two_resistors.kicad_pcb");
const TOOLS: [&str; 8] = [
    "set_board_size",
    "add_layer",
    "add_board_outline",
    "delete_graphics",
    "add_board_text",
    "add_zone",
    "add_copper_pour",
    "import_svg_logo",
];

enum Fault {
    Save(String),
    HideCommitted,
    OpenEditor,
    WriterReadbackMismatch(String),
    WriterReadbackIo,
}

struct Interposed {
    board: PathBuf,
    after_commit: bool,
    fault: Fault,
}

thread_local! {
    // These boundaries are synchronous. Each current-thread Tokio test owns
    // its hook; no sleeps, polling, environment variables or competing saves.
    static INTERPOSED: RefCell<Option<Interposed>> = const { RefCell::new(None) };
}

struct ArmedFault;

impl Drop for ArmedFault {
    fn drop(&mut self) {
        INTERPOSED.with(|slot| *slot.borrow_mut() = None);
    }
}

fn arm(board: &Path, after_commit: bool, fault: Fault) -> ArmedFault {
    INTERPOSED.with(|slot| {
        assert!(slot.borrow().is_none(), "an earlier fault was not consumed");
        *slot.borrow_mut() = Some(Interposed {
            board: board.to_path_buf(),
            after_commit,
            fault,
        });
    });
    ArmedFault
}

fn pending() -> bool {
    INTERPOSED.with(|slot| slot.borrow().is_some())
}

fn take_fault(board: &Path, after_commit: bool) -> Option<Fault> {
    INTERPOSED.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|hook| hook.board == board && hook.after_commit == after_commit)
        {
            slot.take().map(|hook| hook.fault)
        } else {
            None
        }
    })
}

fn apply_fault(board: &Path, fault: Fault) -> Option<konnect_sexp::SexpError> {
    match fault {
        Fault::Save(newer) => std::fs::write(board, newer).unwrap(),
        Fault::HideCommitted => std::fs::rename(board, board.with_extension("committed")).unwrap(),
        Fault::OpenEditor => {
            let lock = konnect_sexp::writer::kicad_editor_lock_path(board).unwrap();
            std::fs::write(lock, "editor acquired the board after planning").unwrap();
        }
        Fault::WriterReadbackMismatch(newer) => {
            std::fs::write(board, newer).unwrap();
            return Some(konnect_sexp::SexpError::Conflict {
                path: board.to_path_buf(),
            });
        }
        Fault::WriterReadbackIo => {
            std::fs::rename(board, board.with_extension("committed")).unwrap();
            return Some(std::fs::read_to_string(board).unwrap_err().into());
        }
    }
    None
}

pub(super) fn before_write(board: &Path) {
    if let Some(fault) = take_fault(board, false) {
        assert!(apply_fault(board, fault).is_none());
    }
}

pub(super) fn after_transaction(
    board: &Path,
    result: Result<bool, konnect_sexp::SexpError>,
) -> Result<bool, konnect_sexp::SexpError> {
    if matches!(result, Ok(true)) {
        if let Some(fault) = take_fault(board, true) {
            if let Some(error) = apply_fault(board, fault) {
                // The real transaction has already replaced the file. Replay
                // the primitive's documented readback Conflict/Io result at
                // its return boundary, so its error mapping is covered too.
                return Err(error);
            }
        }
    }
    result
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let board = dir.path().join("guard.kicad_pcb");
    std::fs::write(&board, NATIVE).unwrap();
    std::fs::write(
        dir.path().join("logo.svg"),
        r#"<svg viewBox="0 0 10 10"><path d="M 0,0 L 10,0 L 5,10 Z"/></svg>"#,
    )
    .unwrap();
    (dir, board)
}

fn arguments(tool: &str, board: &Path) -> Value {
    let mut args = match tool {
        "set_board_size" => json!({"width": 60, "height": 40, "origin_x": 10, "origin_y": 20}),
        "add_layer" => json!({"layer_name": "In1.Cu"}),
        "add_board_outline" => json!({"x1": 10, "y1": 20, "x2": 70, "y2": 60}),
        "delete_graphics" => json!({"layer": "Edge.Cuts"}),
        "add_board_text" => json!({"text": "GUARDED REV", "x": 20, "y": 30}),
        "add_zone" | "add_copper_pour" => json!({
            "net_name": "VCC", "layer": "F.Cu", "name": "guarded copper",
            "points": [{"x": 10, "y": 20}, {"x": 70, "y": 20}, {"x": 70, "y": 60}]
        }),
        "import_svg_logo" => json!({
            "svg": board.with_file_name("logo.svg"), "width_mm": 5, "x": 20, "y": 30
        }),
        _ => panic!("no arguments for {tool}"),
    };
    args["board"] = json!(board);
    args
}

async fn handler(address: &str) -> McpHandler {
    McpHandler::new(ServerConfig {
        kicad_cli: String::new(),
        kicad_binary: String::new(),
        ipc_address: address.to_owned(),
        project_dir: None,
        jlcpcb_db_path: None,
        auto_load_toolsets: false,
        eager_toolsets: true,
    })
    .await
    .unwrap()
}

async fn call(handler: &McpHandler, tool: &str, args: Value) -> CallToolResult {
    let response = handler
        .handle_message(json!({
            "jsonrpc": "2.0", "id": 845, "method": "tools/call",
            "params": {"name": tool, "arguments": args}
        }))
        .await
        .unwrap();
    assert!(response.error.is_none(), "{tool}: {:?}", response.error);
    serde_json::from_value(response.result.unwrap()).unwrap()
}

fn body(result: &CallToolResult) -> Value {
    let ToolContent::Text { text } = &result.content[0] else {
        panic!("expected text result");
    };
    serde_json::from_str(text).unwrap()
}

#[tokio::test]
async fn served_writers_refuse_an_interposed_save_and_preserve_every_newer_byte() {
    let api = handler("").await;
    for tool in TOOLS {
        // The second save changes only nested footprint content, with the same
        // byte length. A guard on outline, metadata, size or mtime is not enough.
        for newer in [
            NATIVE.replacen("(paper \"A4\")", "(paper \"A3\")", 1),
            NATIVE.replacen(
                "(property \"Value\" \"10k\"",
                "(property \"Value\" \"22k\"",
                1,
            ),
        ] {
            assert_ne!(newer, NATIVE, "fixture anchor is missing");
            let (_dir, board) = fixture();
            let _armed = arm(&board, false, Fault::Save(newer.clone()));
            let result = call(&api, tool, arguments(tool, &board)).await;
            assert!(!pending(), "{tool} bypassed the guarded writer");
            let body = body(&result);
            assert!(result.is_error, "{tool}: {body}");
            assert_eq!(body["error"]["kind"], "conflict", "{tool}: {body}");
            assert_eq!(body["error"]["paths"], json!([board]), "{tool}: {body}");
            assert!(body["message"]
                .as_str()
                .unwrap()
                .contains("Nothing was written"));
            assert_eq!(std::fs::read(&board).unwrap(), newer.as_bytes(), "{tool}");
        }
    }
}

/// Each original block not targeted by the operation, including its leading
/// whitespace, must remain byte-identical and in order. This checks native
/// tabs, CRLF, setup/plot parameters, UUIDs, pad nets and footprint properties.
fn assert_unrelated_bytes_preserved(tool: &str, saved: &str) {
    if matches!(
        tool,
        "add_board_outline" | "add_board_text" | "add_zone" | "add_copper_pour" | "import_svg_logo"
    ) {
        assert!(saved.starts_with(&NATIVE[..NATIVE.rfind(')').unwrap()]));
    }
    let mut cursor = 0;
    for (start, end) in find_direct_child_blocks(NATIVE, "kicad_pcb") {
        let block = &NATIVE[start..end];
        let affected = (tool == "add_layer" && block_tag(block) == Some("layers"))
            || (matches!(tool, "set_board_size" | "delete_graphics")
                && block.starts_with("(gr_line")
                && block.contains("\"Edge.Cuts\""));
        if affected {
            continue;
        }
        let (start, _) = find_block_with_leading_whitespace(NATIVE, start).unwrap();
        let original = &NATIVE[start..end];
        let found = saved[cursor..]
            .find(original)
            .unwrap_or_else(|| panic!("{tool} changed an unrelated block: {original}"));
        cursor += found + original.len();
    }
    assert!(saved.starts_with(&NATIVE[..NATIVE.find("(version").unwrap()]));
    assert!(saved.ends_with(&NATIVE[NATIVE.rfind(')').unwrap()..]));
}

fn assert_requested_edit(tool: &str, saved: &str, body: &Value) {
    let tree = parse_sexp(saved).unwrap();
    let graphics = read_file_graphics(saved);
    let edge_count = graphics.iter().filter(|g| g.layer == "Edge.Cuts").count();
    match tool {
        "set_board_size" => {
            assert_eq!(edge_count, 4);
            assert_eq!(body["replaced_segments"], 4);
            assert!(graphics.iter().any(|g| g.origin == Some((10.0, 20.0))));
        }
        "add_layer" => {
            assert!(konnect_sexp::layers::layers(&tree)
                .iter()
                .any(|layer| layer.name == "In1.Cu"));
            // Removing only the one added entry must recover every original
            // byte, including the layer block's whitespace and native entries.
            let entry = format!("\n\t\t({} \"In1.Cu\" signal)", body["id"]);
            assert!(saved.contains(&entry), "{saved}");
            assert_eq!(saved.replacen(&entry, "", 1), NATIVE);
        }
        "add_board_outline" => assert_eq!(edge_count, 8),
        "delete_graphics" => {
            assert_eq!(edge_count, 0);
            assert_eq!(body["deleted"], 4);
        }
        "add_board_text" => assert!(tree
            .find_all("gr_text")
            .iter()
            .any(|node| node.get(1).and_then(SexpNode::as_str) == Some("GUARDED REV"))),
        "add_zone" | "add_copper_pour" => {
            let zone = tree.find("zone").unwrap();
            assert_eq!(zone.find_str("name"), Some("guarded copper"));
            assert_eq!(zone.find_str("net"), Some("VCC"));
        }
        "import_svg_logo" => {
            assert!(graphics
                .iter()
                .any(|g| g.kind == "poly" && g.layer == "F.SilkS"));
            assert_eq!(body["polygon_count"], 1);
        }
        _ => unreachable!(),
    }
    if tool != "add_layer" {
        assert_eq!(body["source"], "file", "{tool}: {body}");
    }
}

#[tokio::test]
async fn served_writers_commit_requested_edits_and_preserve_unrelated_native_bytes() {
    let api = handler("").await;
    for tool in TOOLS {
        let (_dir, board) = fixture();
        let result = call(&api, tool, arguments(tool, &board)).await;
        assert!(!result.is_error, "{tool}: {:?}", result.content);
        let saved = std::fs::read_to_string(&board).unwrap();
        assert_ne!(saved, NATIVE, "{tool} did not write");
        assert_unrelated_bytes_preserved(tool, &saved);
        assert_requested_edit(tool, &saved, &body(&result));
    }
}

#[tokio::test]
async fn served_writers_report_postwrite_readback_mismatch_or_io_failure_as_uncertain() {
    let api = handler("").await;
    for tool in TOOLS {
        // Cover both the independent committed read and error results from
        // the transaction primitive. Every case executes a real replacement.
        for (hide, writer_error) in [(false, false), (true, false), (false, true), (true, true)] {
            let (_dir, board) = fixture();
            let newer = NATIVE.replacen("(paper \"A4\")", "(paper \"A3\")", 1);
            let fault = match (hide, writer_error) {
                (false, false) => Fault::Save(newer.clone()),
                (true, false) => Fault::HideCommitted,
                (false, true) => Fault::WriterReadbackMismatch(newer.clone()),
                (true, true) => Fault::WriterReadbackIo,
            };
            let _armed = arm(&board, true, fault);
            let result = call(&api, tool, arguments(tool, &board)).await;
            assert!(!pending(), "{tool} never reached committed readback");
            let body = body(&result);
            assert!(result.is_error, "{tool}: {body}");
            assert_eq!(
                body["error"]["kind"], "mutation_outcome_uncertain",
                "{tool}: {body}"
            );
            assert_eq!(body["error"]["path"], json!(board), "{tool}: {body}");
            let operation = if tool == "add_copper_pour" {
                "add_zone"
            } else {
                tool
            };
            assert_eq!(body["error"]["operation"], operation, "{tool}: {body}");
            assert_eq!(body["outcome"]["status"], "uncertain", "{tool}: {body}");
            assert_eq!(
                body["outcome"]["retry"],
                crate::outcome::inspect_before_retry()
            );
            assert!(!body["message"]
                .as_str()
                .unwrap()
                .contains("Nothing was written"));
            if hide {
                assert!(!board.exists());
                let committed = std::fs::read_to_string(board.with_extension("committed")).unwrap();
                assert_ne!(committed, NATIVE, "{tool} did not actually commit");
                assert_requested_edit(
                    tool,
                    &committed,
                    &json!({
                        "source": "file", "id": 4,
                        "replaced_segments": 4, "deleted": 4, "polygon_count": 1
                    }),
                );
            } else {
                assert_eq!(std::fs::read(&board).unwrap(), newer.as_bytes(), "{tool}");
            }
        }
    }
}

#[tokio::test]
async fn served_writers_never_fall_back_while_the_exact_board_is_live() {
    for tool in TOOLS {
        let (_dir, board) = fixture();
        let kicad = board_mock::spawn_kicad_holding_board_refusing(
            &board,
            vec![konnect_ipc::gen::kiapi::common::types::KiCadObjectType::KotPcbShape],
            |_| None,
        );
        let api = handler(kicad.address()).await;
        let _armed = arm(&board, false, Fault::Save("must not run".into()));
        let result = call(&api, tool, arguments(tool, &board)).await;
        // Explicitly reject GetItems for graphics: a missing body is an empty
        // inventory, which legitimately makes delete_graphics a successful
        // IPC no-op. Other missing replies refuse or fail live readback.
        // Neither failure may authorize a saved-file mutation.
        assert!(result.is_error, "{tool}: {:?}", result.content);
        assert!(
            pending(),
            "{tool} reached the file writer for the exact live board"
        );
        assert_eq!(std::fs::read(&board).unwrap(), NATIVE.as_bytes(), "{tool}");
    }
}

#[tokio::test]
async fn served_writers_allow_a_different_open_board_without_editing_it() {
    let (_other_dir, other) = fixture();
    let kicad = board_mock::spawn_kicad_holding_board(&other, |_| None);
    let api = handler(kicad.address()).await;
    for tool in TOOLS {
        let (_dir, board) = fixture();
        let result = call(&api, tool, arguments(tool, &board)).await;
        assert!(!result.is_error, "{tool}: {:?}", result.content);
        let saved = std::fs::read_to_string(&board).unwrap();
        assert_requested_edit(tool, &saved, &body(&result));
        assert_eq!(std::fs::read(&other).unwrap(), NATIVE.as_bytes(), "{tool}");
    }
}

#[tokio::test]
async fn an_editor_lock_acquired_after_planning_still_refuses_without_writing() {
    let api = handler("").await;
    for tool in TOOLS {
        let (_dir, board) = fixture();
        let _armed = arm(&board, false, Fault::OpenEditor);
        let result = call(&api, tool, arguments(tool, &board)).await;
        assert!(!pending(), "{tool} bypassed the guarded writer");
        assert!(result.is_error, "{tool}: {:?}", result.content);
        assert_ne!(body(&result)["error"]["kind"], "mutation_outcome_uncertain");
        assert_eq!(std::fs::read(&board).unwrap(), NATIVE.as_bytes(), "{tool}");
        assert!(konnect_sexp::writer::kicad_editor_lock_path(&board)
            .unwrap()
            .exists());
    }
}

#[tokio::test]
async fn delete_preview_and_empty_selection_do_not_enter_the_writer() {
    let api = handler("").await;
    for args in [
        json!({"layer": "Edge.Cuts", "dry_run": true}),
        json!({"layer": "B.SilkS"}),
    ] {
        let (_dir, board) = fixture();
        let _armed = arm(&board, false, Fault::Save("must not run".into()));
        let mut args = args;
        args["board"] = json!(board);
        let result = call(&api, "delete_graphics", args).await;
        assert!(!result.is_error, "{:?}", result.content);
        assert!(pending(), "a read-only selection entered the writer");
        assert_eq!(body(&result)["deleted"], 0);
        assert_eq!(std::fs::read(&board).unwrap(), NATIVE.as_bytes());
    }
}
