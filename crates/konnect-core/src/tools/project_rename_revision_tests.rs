//! Deterministic served-call regressions. No native KiCad is spawned here.
//! Unchanged native fixture provenance: annotate_duplicates.README.md and
//! specctra_two_resistors.README.md. PRL and injected variants are synthetic.

use super::*;
use crate::mcp::handler::McpHandler;
use crate::mcp::protocol::ToolContent;
use crate::tools::pcb_board::board_mock;
use crate::tools::ServerConfig;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const OLD: &str = "multichannel_mixer";
const NEW: &str = "guarded_rename";
const ROOT: &str = include_str!("../../tests/fixtures/multichannel_mixer.kicad_sch");
const CHILD: &str = include_str!("../../tests/fixtures/multichannel_channel_strip.kicad_sch");
const PRO: &str = include_str!("../../tests/fixtures/multichannel_mixer.kicad_pro");
const BOARD: &str = include_str!("../../tests/fixtures/specctra_two_resistors.kicad_pcb");
const PRL: &str = r#"{"name":"multichannel_mixer","file":"multichannel_mixer.kicad_prl","unrelated":"keep-A"}"#;

type Action = Box<dyn FnOnce() -> Option<SexpError>>;

struct Hook {
    stage: &'static str,
    path: PathBuf,
    action: Action,
    fired: Rc<Cell<bool>>,
}

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

struct ArmedHook(Rc<Cell<bool>>);

impl ArmedHook {
    fn assert_fired(&self) {
        assert!(self.0.get(), "the designated interleaving hook was not consumed");
    }
}

impl Drop for ArmedHook {
    fn drop(&mut self) {
        HOOK.with(|slot| *slot.borrow_mut() = None);
    }
}

fn arm(stage: &'static str, path: &Path, action: impl FnOnce() -> Option<SexpError> + 'static) -> ArmedHook {
    let fired = Rc::new(Cell::new(false));
    HOOK.with(|slot| {
        assert!(slot.borrow().is_none(), "previous hook was not cleared");
        *slot.borrow_mut() = Some(Hook {
            stage, path: path.to_owned(), action: Box::new(action), fired: fired.clone(),
        });
    });
    ArmedHook(fired)
}

fn take(stage: &str, path: &Path) -> Option<Action> {
    HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|hook| hook.stage == stage && hook.path == path) {
            let hook = slot.take().unwrap();
            hook.fired.set(true);
            Some(hook.action)
        } else {
            None
        }
    })
}

pub(super) fn fire(stage: &str, path: &Path) {
    if let Some(action) = take(stage, path) {
        assert!(action().is_none(), "prewrite hook cannot return a writer error");
    }
}

pub(super) fn after_write(
    path: &Path,
    result: Result<bool, SexpError>,
    attempted: bool,
) -> Result<bool, SexpError> {
    if matches!(result, Ok(true)) && attempted {
        if let Some(action) = take("after_write", path) {
            if let Some(error) = action() {
                return Err(error);
            }
        }
    }
    result
}

struct Fixture {
    _temp: tempfile::TempDir,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join(OLD);
        std::fs::create_dir(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        for (extension, bytes) in [
            ("kicad_pro", PRO), ("kicad_sch", ROOT),
            ("kicad_pcb", BOARD), ("kicad_prl", PRL),
        ] {
            std::fs::write(dir.join(format!("{OLD}.{extension}")), bytes).unwrap();
        }
        std::fs::write(dir.join("multichannel_channel_strip.kicad_sch"), CHILD).unwrap();
        Self { _temp: temp, dir }
    }

    fn old(&self, extension: &str) -> PathBuf {
        self.dir.join(format!("{OLD}.{extension}"))
    }

    fn new_path(&self, extension: &str) -> PathBuf {
        self.dir.join(format!("{NEW}.{extension}"))
    }

    fn child(&self) -> PathBuf {
        self.dir.join("multichannel_channel_strip.kicad_sch")
    }
}

async fn api(address: &str) -> McpHandler {
    McpHandler::new(ServerConfig {
        kicad_cli: String::new(), kicad_binary: String::new(),
        ipc_address: address.to_owned(), project_dir: None, jlcpcb_db_path: None,
        auto_load_toolsets: false, eager_toolsets: true,
    }).await.unwrap()
}

async fn call(api: &McpHandler, fixture: &Fixture, options: Value) -> CallToolResult {
    let mut args = json!({"project": fixture.old("kicad_pro"), "new_name": NEW});
    for (key, value) in options.as_object().unwrap() {
        args[key] = value.clone();
    }
    let response = api.handle_message(json!({
        "jsonrpc": "2.0", "id": 851, "method": "tools/call",
        "params": {"name": "rename_project", "arguments": args},
    })).await.unwrap();
    assert!(response.error.is_none(), "{response:?}");
    serde_json::from_value(response.result.unwrap()).unwrap()
}

fn body(result: &CallToolResult) -> Value {
    let ToolContent::Text { text } = &result.content[0] else {
        panic!("expected JSON text");
    };
    serde_json::from_str(text).unwrap()
}

fn assert_status(result: &CallToolResult, status: &str) -> Value {
    let body = body(result);
    assert_eq!(body["outcome"]["status"], status, "designated outcome assertion: {body}");
    assert_eq!(result.is_error, status != "complete", "{body}");
    body
}

fn assert_late_native_move_failure(result: &CallToolResult) -> Value {
    let body = body(result);
    assert!(result.is_error, "{body}");
    // Native directory collision error codes differ. A known EEXIST refusal
    // is partial; an unexpected native error is conservatively uncertain.
    match body["outcome"]["status"].as_str() {
        Some("partial") => assert_eq!(body["error"]["kind"], "conflict"),
        Some("uncertain") => assert_eq!(body["error"]["kind"], "mutation_outcome_uncertain"),
        _ => panic!("late native move failure hid applied work: {body}"),
    }
    assert_eq!(body["outcome"]["retry"]["safe"], false);
    body
}

fn unchanged(f: &Fixture) {
    for (extension, expected) in [
        ("kicad_pro", PRO), ("kicad_sch", ROOT), ("kicad_pcb", BOARD), ("kicad_prl", PRL),
    ] {
        assert_eq!(std::fs::read(f.old(extension)).unwrap(), expected.as_bytes());
        assert!(!f.new_path(extension).exists());
    }
    assert_eq!(std::fs::read(f.child()).unwrap(), CHILD.as_bytes());
}

#[tokio::test]
async fn clean_rename_changes_only_independently_enumerated_reference_bytes() {
    let f = Fixture::new();
    let api = api("").await;
    let result = call(&api, &f, json!({})).await;
    let body = assert_status(&result, "complete");
    let root_keys = ROOT.matches("(project \"multichannel_mixer\"").count();
    let child_keys = CHILD.matches("(project \"multichannel_mixer\"").count();
    assert_eq!((root_keys, child_keys), (29, 36));
    // These byte oracles never call the production transformer.
    assert_eq!(std::fs::read(f.new_path("kicad_sch")).unwrap(),
        ROOT.replace("(project \"multichannel_mixer\"", "(project \"guarded_rename\"").as_bytes());
    assert_eq!(std::fs::read(f.child()).unwrap(),
        CHILD.replace("(project \"multichannel_mixer\"", "(project \"guarded_rename\"").as_bytes());
    assert_eq!(std::fs::read(f.new_path("kicad_pro")).unwrap(),
        PRO.replace("\"multichannel_mixer.kicad_pro\"", "\"guarded_rename.kicad_pro\"")
            .replace("\"multichannel_mixer\"", "\"guarded_rename\"").as_bytes());
    assert_eq!(std::fs::read(f.new_path("kicad_prl")).unwrap(),
        br#"{"name":"guarded_rename","file":"guarded_rename.kicad_prl","unrelated":"keep-A"}"#.as_slice());
    assert_eq!(std::fs::read(f.new_path("kicad_pcb")).unwrap(), BOARD.as_bytes());
    assert_eq!(body["applied"]["moved_files"].as_array().unwrap().len(), 4);
    assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), 4);
    for extension in PROJECT_EXTS {
        assert!(!f.old(extension).exists());
    }
}

#[tokio::test]
async fn original_full_snapshots_reject_root_child_project_and_prl_interposed_saves() {
    let api = api("").await;
    for (extension, before, newer, completed) in [
        ("kicad_sch", ROOT, ROOT.replacen("(paper \"A4\")", "(paper \"A3\")", 1), 1),
        ("child", CHILD, CHILD.replacen("(paper \"A4\")", "(paper \"A3\")", 1), 3),
        ("kicad_pro", PRO, PRO.replacen("\"version\": 1", "\"version\": 2", 1), 0),
        ("kicad_prl", PRL, PRL.replace("keep-A", "keep-B"), 2),
    ] {
        assert_ne!(newer, before, "synthetic same-length edit anchor is missing");
        assert_eq!(newer.len(), before.len());
        let f = Fixture::new();
        let path = if extension == "child" { f.child() } else { f.new_path(extension) };
        let destination = path.clone();
        let saved = newer.clone();
        let hook = arm("before_rewrite", &path, move || {
            std::fs::write(destination, saved).unwrap();
            None
        });
        let result = call(&api, &f, json!({})).await;
        hook.assert_fired();
        let body = assert_status(&result, "partial");
        assert_eq!(body["error"]["kind"], "conflict", "designated guard assertion: {body}");
        assert_eq!(body["error"]["paths"], json!([path]));
        assert_eq!(std::fs::read(&path).unwrap(), newer.as_bytes(), "newer bytes must survive");
        assert_eq!(body["applied"]["moved_files"].as_array().unwrap().len(), 4);
        assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), completed);
        assert_eq!(body["outcome"]["retry"]["safe"], false);
    }
}

#[tokio::test]
async fn pre_move_save_is_a_clean_refusal_without_filename_changes() {
    let f = Fixture::new();
    let path = f.old("kicad_pro");
    let newer = PRO.replacen("\"version\": 1", "\"version\": 2", 1);
    assert_ne!(newer, PRO);
    let saved = newer.clone();
    let destination = path.clone();
    let hook = arm("before_moves", &path, move || {
        std::fs::write(destination, saved).unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({})).await;
    hook.assert_fired();
    let body = assert_status(&result, "failed");
    assert_eq!(body["error"]["kind"], "conflict");
    assert!(body["applied"]["moved_files"].as_array().unwrap().is_empty());
    assert_eq!(std::fs::read(path).unwrap(), newer.as_bytes());
    assert!(!f.new_path("kicad_pro").exists());
    assert_eq!(std::fs::read(f.old("kicad_sch")).unwrap(), ROOT.as_bytes());
}

#[tokio::test]
async fn old_project_and_design_owners_refuse_before_any_move() {
    let api = api("").await;
    for extension in ["kicad_pro", "kicad_sch", "kicad_pcb", "child"] {
        let f = Fixture::new();
        let path = if extension == "child" { f.child() } else { f.old(extension) };
        let lock = if extension == "kicad_pro" {
            konnect_sexp::writer::kicad_project_lock_path(&path).unwrap()
        } else {
            konnect_sexp::writer::kicad_editor_lock_path(&path).unwrap()
        };
        std::fs::write(&lock, "synthetic owner, not proof of freshness").unwrap();
        let result = call(&api, &f, json!({})).await;
        assert_status(&result, "failed");
        unchanged(&f);
        assert!(lock.exists());
    }
}

#[tokio::test]
async fn exact_live_board_refuses_and_unrelated_live_board_is_untouched() {
    let f = Fixture::new();
    let mock = board_mock::spawn_kicad_holding_board(&f.old("kicad_pcb"), |_| None);
    let result = call(&api(mock.address()).await, &f, json!({})).await;
    assert_status(&result, "failed");
    unchanged(&f);
    drop(mock);

    let other = Fixture::new();
    let mock = board_mock::spawn_kicad_holding_board(&other.old("kicad_pcb"), |_| None);
    let result = call(&api(mock.address()).await, &f, json!({})).await;
    assert_status(&result, "complete");
    unchanged(&other);
}

#[tokio::test]
async fn pending_journals_are_refused_without_recovery_or_moves() {
    let api = api("").await;
    for content in ["not valid JSON", r#"{"version":1,"id":"pending","entries":[]}"#] {
        let f = Fixture::new();
        let path = f.dir.join(".konnect-transaction-pending.json");
        std::fs::write(&path, content).unwrap();
        let result = call(&api, &f, json!({})).await;
        let body = assert_status(&result, "failed");
        assert_eq!(body["error"]["paths"], json!([path]));
        assert_eq!(std::fs::read_to_string(path).unwrap(), content);
        unchanged(&f);
    }
}

#[tokio::test]
async fn late_new_or_old_design_lock_reports_partial_without_rewriting_the_locked_sheet() {
    let api = api("").await;
    for old_lock in [false, true] {
        let f = Fixture::new();
        let path = f.new_path("kicad_sch");
        let locked_path = if old_lock { f.old("kicad_sch") } else { path.clone() };
        let lock = konnect_sexp::writer::kicad_editor_lock_path(&locked_path).unwrap();
        let hook = arm("before_rewrite", &path, move || {
            std::fs::write(lock, "synthetic late lock").unwrap();
            None
        });
        let result = call(&api, &f, json!({})).await;
        hook.assert_fired();
        let body = assert_status(&result, "partial");
        assert_eq!(body["error"]["kind"], "conflict");
        assert_eq!(std::fs::read(path).unwrap(), ROOT.as_bytes());
        assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn late_project_lock_reports_partial_with_the_verified_project_rewrite() {
    let f = Fixture::new();
    let path = f.new_path("kicad_sch");
    let project = f.new_path("kicad_pro");
    let lock = konnect_sexp::writer::kicad_project_lock_path(&project).unwrap();
    let hook = arm("before_rewrite", &path, move || {
        std::fs::write(lock, "late project owner").unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({})).await;
    hook.assert_fired();
    let body = assert_status(&result, "partial");
    assert_eq!(body["error"]["kind"], "unsafe_file_fallback");
    assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(path).unwrap(), ROOT.as_bytes());
}

#[tokio::test]
async fn nonregular_child_source_refuses_before_any_project_move() {
    let f = Fixture::new();
    std::fs::create_dir(f.dir.join("unreadable-child.kicad_sch")).unwrap();
    let result = call(&api("").await, &f, json!({})).await;
    assert_status(&result, "failed");
    unchanged(&f);
}

#[tokio::test]
async fn concurrent_new_target_and_recreated_old_source_survive_without_rollback() {
    let f = Fixture::new();
    let to = f.new_path("kicad_sch");
    let old_pro = f.old("kicad_pro");
    let foreign_target = to.clone();
    let hook = arm("before_move", &to, move || {
        std::fs::write(old_pro, "recreated old project by another writer").unwrap();
        std::fs::write(foreign_target, "concurrent target must survive").unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({})).await;
    hook.assert_fired();
    let body = assert_status(&result, "partial");
    assert_eq!(body["error"]["kind"], "conflict");
    assert_eq!(std::fs::read_to_string(f.old("kicad_pro")).unwrap(),
        "recreated old project by another writer");
    assert_eq!(std::fs::read_to_string(to).unwrap(), "concurrent target must survive");
    assert_eq!(std::fs::read(f.new_path("kicad_pro")).unwrap(), PRO.as_bytes());
    assert_eq!(std::fs::read(f.old("kicad_sch")).unwrap(), ROOT.as_bytes());
    assert_eq!(body["applied"]["moved_files"].as_array().unwrap().len(), 1);
    assert!(body["applied"]["verified_rewrites"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn first_move_collision_is_clean_and_preserves_the_new_target() {
    let f = Fixture::new();
    let to = f.new_path("kicad_pro");
    let destination = to.clone();
    let hook = arm("before_move", &to, move || {
        std::fs::write(destination, "concurrent first target").unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({})).await;
    hook.assert_fired();
    let body = assert_status(&result, "failed");
    assert!(body["applied"]["moved_files"].as_array().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(to).unwrap(), "concurrent first target");
    assert_eq!(std::fs::read(f.old("kicad_pro")).unwrap(), PRO.as_bytes());
}

#[tokio::test]
async fn postwrite_mismatch_and_io_are_uncertain_and_never_clean_refusals() {
    let api = api("").await;
    for mode in ["readback_mismatch", "readback_io", "writer_conflict", "writer_io"] {
        let f = Fixture::new();
        let path = f.new_path("kicad_sch");
        let destination = path.clone();
        let newer = ROOT.replace("(project \"multichannel_mixer\"", "(project \"guarded_rename\"")
            .replacen("(paper \"A4\")", "(paper \"A3\")", 1);
        let saved = newer.clone();
        let hook = arm("after_write", &path, move || {
            match mode {
                "readback_io" | "writer_io" => {
                    std::fs::rename(&destination, destination.with_extension("unobserved")).unwrap();
                }
                _ => std::fs::write(&destination, saved).unwrap(),
            }
            match mode {
                "writer_conflict" => Some(SexpError::Conflict { path: destination }),
                "writer_io" => Some(std::fs::read(&destination).unwrap_err().into()),
                _ => None,
            }
        });
        let result = call(&api, &f, json!({})).await;
        hook.assert_fired();
        let body = assert_status(&result, "uncertain");
        assert_eq!(body["error"]["kind"], "mutation_outcome_uncertain",
            "designated uncertain assertion: {body}");
        assert_eq!(body["outcome"]["retry"]["safe"], false);
        assert_eq!(body["applied"]["moved_files"].as_array().unwrap().len(), 4);
        assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), 1);
        if mode.ends_with("io") {
            assert!(!path.exists());
            assert!(path.with_extension("unobserved").exists());
        } else {
            assert_eq!(std::fs::read(path).unwrap(), newer.as_bytes());
        }
    }
}

#[tokio::test]
async fn backup_collision_reports_partial_and_never_claims_a_backup_move() {
    let f = Fixture::new();
    let source = f.dir.join(format!("{OLD}-backups"));
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("keep.txt"), "original backup").unwrap();
    let target = f.dir.join(format!("{NEW}-backups"));
    let destination = target.clone();
    let hook = arm("before_move", &target, move || {
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("keep.txt"), "other backup").unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({})).await;
    hook.assert_fired();
    let body = assert_late_native_move_failure(&result);
    assert!(body["applied"]["backup_move"].is_null());
    assert_eq!(std::fs::read_to_string(source.join("keep.txt")).unwrap(), "original backup");
    assert_eq!(std::fs::read_to_string(target.join("keep.txt")).unwrap(), "other backup");
    assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn late_directory_collision_keeps_completed_backup_and_project_moves() {
    let f = Fixture::new();
    std::fs::create_dir(f.dir.join(format!("{OLD}-backups"))).unwrap();
    let target = f.dir.parent().unwrap().join(NEW);
    let destination = target.clone();
    let hook = arm("before_move", &target, move || {
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("foreign"), "preserve").unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({"rename_directory": true})).await;
    hook.assert_fired();
    let body = assert_late_native_move_failure(&result);
    assert!(body["applied"]["backup_move"].is_object());
    assert!(body["applied"]["directory_move"].is_null());
    assert!(f.dir.join(format!("{NEW}-backups")).is_dir());
    assert_eq!(std::fs::read_to_string(target.join("foreign")).unwrap(), "preserve");
    assert!(f.new_path("kicad_sch").is_file());
}

#[tokio::test]
async fn late_move_io_failure_reports_uncertain_with_known_applied_work() {
    let api = api("").await;
    for backup_phase in [false, true] {
        let f = Fixture::new();
        let from = if backup_phase {
            let path = f.dir.join(format!("{OLD}-backups"));
            std::fs::create_dir(&path).unwrap();
            path
        } else {
            f.old("kicad_sch")
        };
        let to = if backup_phase {
            f.dir.join(format!("{NEW}-backups"))
        } else {
            f.new_path("kicad_sch")
        };
        let hook = arm("before_move", &to, move || {
            if backup_phase {
                std::fs::remove_dir(from).unwrap();
            } else {
                std::fs::remove_file(from).unwrap();
            }
            None
        });
        let result = call(&api, &f, json!({})).await;
        hook.assert_fired();
        let body = assert_status(&result, "uncertain");
        assert_eq!(body["error"]["kind"], "mutation_outcome_uncertain");
        assert!(body["applied"]["unverified_move_attempt"].is_object());
        let moved = body["applied"]["moved_files"].as_array().unwrap().len();
        assert_eq!(moved, if backup_phase { 4 } else { 1 });
        assert_eq!(std::fs::read(f.new_path("kicad_pro")).unwrap(),
            if backup_phase {
                PRO.replace("\"multichannel_mixer.kicad_pro\"", "\"guarded_rename.kicad_pro\"").into_bytes()
            } else {
                PRO.as_bytes().to_vec()
            });
    }
}

#[tokio::test]
async fn synthetic_board_project_token_uses_the_guard_but_is_not_native_evidence() {
    let f = Fixture::new();
    // This intentionally synthetic input exercises the otherwise uncovered
    // branch. Native BOARD has no (project ...) clause.
    let before = "(kicad_pcb (project \"multichannel_mixer\") (synthetic \"keep-A\"))";
    let newer = before.replace("keep-A", "keep-B");
    std::fs::write(f.old("kicad_pcb"), before).unwrap();
    let path = f.new_path("kicad_pcb");
    let destination = path.clone();
    let saved = newer.clone();
    let hook = arm("before_rewrite", &path, move || {
        std::fs::write(destination, saved).unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({})).await;
    hook.assert_fired();
    let body = assert_status(&result, "partial");
    assert_eq!(body["error"]["kind"], "conflict");
    assert_eq!(std::fs::read(path).unwrap(), newer.as_bytes());
}

#[tokio::test]
async fn successful_directory_move_reports_final_current_paths() {
    let f = Fixture::new();
    std::fs::create_dir(f.dir.join(format!("{OLD}-backups"))).unwrap();
    let result = call(&api("").await, &f, json!({"rename_directory": true})).await;
    let body = assert_status(&result, "complete");
    let target = f.dir.parent().unwrap().join(NEW);
    assert!(!f.dir.exists());
    assert!(target.join(format!("{NEW}.kicad_sch")).is_file());
    assert!(target.join(format!("{NEW}-backups")).is_dir());
    for entry in body["applied"]["current_paths"].as_array().unwrap() {
        let path = PathBuf::from(entry["current_path"].as_str().unwrap());
        assert!(path.starts_with(&target), "{entry}");
        assert!(path.is_file(), "{entry}");
    }
}

#[tokio::test]
async fn final_directory_readback_failure_is_uncertain_at_the_relocated_path() {
    let f = Fixture::new();
    let target = f.dir.parent().unwrap().join(NEW);
    let path = target.join(format!("{NEW}.kicad_sch"));
    let destination = path.clone();
    let newer = ROOT.replace("(project \"multichannel_mixer\"", "(project \"guarded_rename\"")
        .replacen("(paper \"A4\")", "(paper \"A3\")", 1);
    let saved = newer.clone();
    let hook = arm("after_move", &target, move || {
        std::fs::write(destination, saved).unwrap();
        None
    });
    let result = call(&api("").await, &f, json!({"rename_directory": true})).await;
    hook.assert_fired();
    let body = assert_status(&result, "uncertain");
    assert_eq!(body["error"]["path"], json!(path));
    assert!(body["applied"]["directory_move"].is_object());
    assert_eq!(body["applied"]["verified_rewrites"].as_array().unwrap().len(), 4);
    assert_eq!(std::fs::read(path).unwrap(), newer.as_bytes());
}

#[tokio::test]
async fn dry_run_is_read_only_even_with_locks_and_pending_journals() {
    let f = Fixture::new();
    let lock = konnect_sexp::writer::kicad_project_lock_path(&f.old("kicad_pro")).unwrap();
    std::fs::write(lock, "owner").unwrap();
    std::fs::write(f.dir.join(".konnect-transaction-pending.json"), "pending").unwrap();
    let mut before: Vec<_> = std::fs::read_dir(&f.dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    before.sort();
    let result = call(&api("").await, &f, json!({"dry_run": true, "rename_directory": true})).await;
    assert!(!result.is_error, "{result:?}");
    assert_eq!(body(&result)["dry_run"], true);
    assert!(body(&result)["content_rewrites"].as_array().unwrap().iter()
        .any(|entry| entry["file"] == "multichannel_channel_strip.kicad_sch"));
    unchanged(&f);
    let mut after: Vec<_> = std::fs::read_dir(&f.dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    after.sort();
    assert_eq!(before, after);
}

#[test]
fn native_exclusive_moves_preserve_existing_file_and_directory_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let from = temp.path().join("源-project");
    let to = temp.path().join("目标-project");
    std::fs::write(&from, "source").unwrap();
    std::fs::write(&to, "destination").unwrap();
    assert!(move_no_replace(&from, &to).is_err());
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "source");
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "destination");
    std::fs::remove_file(&to).unwrap();
    move_no_replace(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "source");
    let from = temp.path().join("from-dir");
    let to = temp.path().join("to-dir");
    std::fs::create_dir(&from).unwrap();
    std::fs::create_dir(&to).unwrap();
    assert!(move_no_replace(&from, &to).is_err());
    assert!(from.is_dir());
    assert!(to.is_dir());
    let missing = temp.path().join("missing");
    assert!(move_no_replace(&missing, &to).is_err());
    assert!(to.is_dir());
}

#[cfg(unix)]
#[test]
fn exclusive_move_preserves_even_a_dangling_destination_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let from = temp.path().join("source");
    let to = temp.path().join("destination");
    std::fs::write(&from, "source").unwrap();
    std::os::unix::fs::symlink("absent", &to).unwrap();
    assert!(move_no_replace(&from, &to).is_err());
    assert_eq!(std::fs::read_to_string(from).unwrap(), "source");
    assert_eq!(std::fs::read_link(to).unwrap(), PathBuf::from("absent"));
}

#[cfg(windows)]
#[test]
fn windows_exclusive_moves_reject_nul_and_accept_verbatim_unicode_paths() {
    use std::os::windows::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let from = dir.join("源-source");
    let to = dir.join("目标-destination");
    std::fs::write(&from, "source").unwrap();
    let nul = PathBuf::from(std::ffi::OsString::from_wide(&[b'x' as u16, 0, b'y' as u16]));
    assert_eq!(move_no_replace(&from, &nul).unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "source");
    move_no_replace(&from, &to).unwrap();
    assert_eq!(std::fs::read_to_string(to).unwrap(), "source");
    let mut long_dir = dir;
    for _ in 0..3 {
        long_dir.push("long-path-segment-".repeat(7));
        std::fs::create_dir(&long_dir).unwrap();
    }
    let from = long_dir.join("源");
    let to = long_dir.join("目标");
    std::fs::write(&from, "long source").unwrap();
    move_no_replace(&from, &to).unwrap();
    assert_eq!(std::fs::read_to_string(to).unwrap(), "long source");
}
