//! `project` toolset — create, open, save, and snapshot KiCAD projects.
//!
//! Tools: create_project, open_project, save_project, get_project_info, snapshot_project
//!
//! KiCAD interface:
//!   - create_project   → file system (template)
//!   - open_project     → IPC open-document query
//!   - save_project     → IPC board.save()
//!   - get_project_info → file system read
//!   - snapshot_project → kicad-cli export PDF

use crate::mcp::{error::ToolErrorKind, protocol::CallToolResult};
use crate::tool;
use crate::tools::{get_path, invalid_arg, opt_str, require_str, ToolContext, ToolDef};
use konnect_sexp::{commit_file_transaction, FileTransition, SexpError};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn tools() -> Vec<ToolDef> {
    let mut tools = vec![
        tool!(
            "create_project",
            "Create a new KiCAD project at the given path. Creates the directory, \
             a blank .kicad_pro file, an empty .kicad_sch schematic, and a blank \
             .kicad_pcb board file. Refuses to replace any existing project file.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path where the project will be created"
                    },
                    "name": {
                        "type": "string",
                        "description": "Project name (used as filename stem)"
                    }
                },
                "required": ["path", "name"]
            }),
            |args, ctx| async move { handle_create_project(args, ctx).await }
        ),
        tool!(
            "open_project",
            "List PCB documents open in the running KiCad UI and optionally check whether \
             one specific KiCad project or board is open over IPC.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional path to a .kicad_pro project or .kicad_pcb board to check"
                    }
                },
                "required": []
            }),
            |args, ctx| async move { handle_open_project(args, ctx).await }
        ),
        tool!(
            "save_project",
            "Save the currently open PCB board file via KiCAD IPC. \
             Requires KiCAD to be running with IPC enabled.",
            json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            |args, ctx| async move { handle_save_project(args, ctx).await }
        ),
        tool!(
            "rename_project",
            "Rename a KiCAD project: renames the .kicad_pro/.kicad_sch/.kicad_pcb/.kicad_prl \
             files and rewrites the internal references that carry the old name. Renaming the \
             files alone is NOT enough — every symbol instance stores (project \"name\"), and \
             a mismatch there makes KiCAD treat the design as unannotated, losing every \
             reference designator. Equivalent to eeschema's File > Save As. Use dry_run first.",
            json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Path to the existing .kicad_pro file" },
                    "new_name": { "type": "string", "description": "New project name, without extension" },
                    "rename_directory": { "type": "boolean", "description": "Also rename the containing folder when it matches the old project name. Default false.", "default": false },
                    "dry_run": { "type": "boolean", "description": "Report the planned changes without touching anything. Default false.", "default": false }
                },
                "required": ["project", "new_name"]
            }),
            |args, ctx| async move { handle_rename_project(args, ctx).await }
        ),
        tool!(
            "get_project_info",
            "Read project metadata from a .kicad_pro file. Returns the project name, \
             schematic and PCB paths, last modified time, project-file format version, \
             and the generator versions recorded by the sibling design files. The \
             compatibility kicad_version is null when those siblings disagree.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the .kicad_pro project file"
                    }
                },
                "required": ["path"]
            }),
            |args, ctx| async move { handle_get_project_info(args, ctx).await }
        ),
        tool!(
            "snapshot_project",
            "Export the schematic and PCB to PDF as a timestamped snapshot/checkpoint. \
             Useful for saving progress before major edits.",
            json!({
                "type": "object",
                "properties": {
                    "schematic": {
                        "type": "string",
                        "description": "Path to .kicad_sch file"
                    },
                    "pcb": {
                        "type": "string",
                        "description": "Optional: path to .kicad_pcb file"
                    },
                    "output_dir": {
                        "type": "string",
                        "description": "Directory to write snapshot PDFs"
                    },
                    "label": {
                        "type": "string",
                        "description": "Optional label to include in the filename"
                    }
                },
                "required": ["schematic", "output_dir"]
            }),
            |args, ctx| async move { handle_snapshot_project(args, ctx).await }
        ),
        tool!(
            "open_schematic_viewer",
            "Launch the live schematic viewer. The viewer shows the schematic as SVG and \
             auto-refreshes when the file changes. Use this after placing components so the \
             user can see the schematic in real-time as you edit it. For hierarchical designs, \
             pass the root schematic — the viewer discovers every sheet reachable from it and \
             shows a sheet selector.",
            json!({
                "type": "object",
                "properties": {
                    "schematic": {
                        "type": "string",
                        "description": "Path to the root .kicad_sch file to view"
                    }
                },
                "required": ["schematic"]
            }),
            |args, ctx| async move { handle_open_viewer(args, ctx).await }
        ),
    ];
    tools.extend(super::editor_shutdown::tools());
    tools
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

fn existing_project_paths(paths: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let mut existing = Vec::new();
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(_) => existing.push(path.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(existing)
}

fn project_conflict(paths: Vec<PathBuf>) -> CallToolResult {
    let paths: Vec<String> = paths
        .into_iter()
        .map(|path| path.display().to_string())
        .collect();
    CallToolResult::error_kind(
        ToolErrorKind::Conflict {
            paths: paths.clone(),
        },
        format!(
            "Project creation would replace existing path(s): {}",
            paths.join(", ")
        ),
    )
}

fn create_project_files(
    project_dir: &Path,
    name: &str,
    pro_path: &Path,
    sch_path: &Path,
    pcb_path: &Path,
) -> Result<(), SexpError> {
    commit_file_transaction(
        project_dir,
        vec![
            FileTransition::create(pro_path, blank_kicad_pro(name)),
            FileTransition::create(sch_path, blank_kicad_sch()),
            FileTransition::create(pcb_path, blank_kicad_pcb()),
        ],
    )?;
    Ok(())
}

async fn handle_create_project(
    args: &serde_json::Value,
    _ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let path = get_path(args, "path")?;
    let name = match require_str(args, "name") {
        Ok(n) => n.to_string(),
        Err(e) => return Ok(e),
    };
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".kicad_pro"))
    {
        return Ok(invalid_arg(
            "path",
            "pass the containing directory as 'path' and the project filename stem as 'name'",
        ));
    }

    let pro_path = path.join(format!("{}.kicad_pro", name));
    let sch_path = path.join(format!("{}.kicad_sch", name));
    let pcb_path = path.join(format!("{}.kicad_pcb", name));
    let project_paths = vec![pro_path.clone(), sch_path.clone(), pcb_path.clone()];

    let existing = tokio::task::spawn_blocking({
        let project_paths = project_paths.clone();
        move || existing_project_paths(&project_paths)
    })
    .await??;
    if !existing.is_empty() {
        return Ok(project_conflict(existing));
    }

    tokio::fs::create_dir_all(&path).await?;

    let transaction = tokio::task::spawn_blocking({
        let project_dir = path.clone();
        let name = name.clone();
        let pro_path = pro_path.clone();
        let sch_path = sch_path.clone();
        let pcb_path = pcb_path.clone();
        move || create_project_files(&project_dir, &name, &pro_path, &sch_path, &pcb_path)
    })
    .await?;
    match transaction {
        Ok(()) => {}
        Err(SexpError::TransactionConflict { path, .. }) => {
            return Ok(project_conflict(vec![path]));
        }
        Err(error) => return Err(error.into()),
    }

    Ok(CallToolResult::json(&json!({
        "created": true,
        "project_file": pro_path.display().to_string(),
        "schematic": sch_path.display().to_string(),
        "pcb": pcb_path.display().to_string()
    })))
}

fn requested_board_path(
    args: &serde_json::Value,
) -> Result<Option<(String, PathBuf)>, CallToolResult> {
    let Some(value) = args.get("path").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let Some(raw) = value.as_str().filter(|path| !path.trim().is_empty()) else {
        return Err(CallToolResult::error_kind(
            ToolErrorKind::InvalidArgument {
                field: "path".to_string(),
                reason: "must be a non-empty .kicad_pro or .kicad_pcb path".to_string(),
            },
            "Argument 'path' must be a non-empty .kicad_pro or .kicad_pcb path",
        ));
    };
    let requested = PathBuf::from(raw);
    let extension = requested
        .extension()
        .and_then(|extension| extension.to_str());
    let board = match extension {
        Some(extension) if extension.eq_ignore_ascii_case("kicad_pro") => {
            requested.with_extension("kicad_pcb")
        }
        Some(extension) if extension.eq_ignore_ascii_case("kicad_pcb") => requested.clone(),
        _ => {
            return Err(CallToolResult::error_kind(
                ToolErrorKind::InvalidArgument {
                    field: "path".to_string(),
                    reason: "must end in .kicad_pro or .kicad_pcb".to_string(),
                },
                "Argument 'path' must end in .kicad_pro or .kicad_pcb",
            ));
        }
    };
    Ok(Some((raw.to_string(), board)))
}

async fn handle_open_project(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let requested = match requested_board_path(args) {
        Ok(requested) => requested,
        Err(error) => return Ok(error),
    };
    let ipc = konnect_ipc::KiCadIpcClient::new(&ctx.config.ipc_address);
    let ping = ipc.ping_outcome();
    let connected = ping.is_responsive();
    let (open_boards, open_boards_error) = if connected {
        match ipc.get_open_board_paths() {
            Ok(paths) => (
                paths
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
                None,
            ),
            Err(error) => (Vec::new(), Some(format!("{error:#}"))),
        }
    } else {
        (Vec::new(), None)
    };
    let (requested_path, requested_board, requested_open, requested_check_error) = match requested {
        Some((project_or_board, board)) if connected => match ipc.find_open_board(&board) {
            Ok(_) => {
                ctx.board_session.observe_live(&board);
                (
                    Some(project_or_board),
                    Some(board.display().to_string()),
                    Some(true),
                    None,
                )
            }
            Err(error) => (
                Some(project_or_board),
                Some(board.display().to_string()),
                Some(false),
                Some(format!("{error:#}")),
            ),
        },
        Some((project_or_board, board)) => (
            Some(project_or_board),
            Some(board.display().to_string()),
            None,
            None,
        ),
        None => (None, None, None, None),
    };

    let message = if !connected {
        unanswered_message(&ping)
    } else if requested_open == Some(true) {
        "The requested board is open in KiCad."
    } else if requested_open == Some(false) {
        "KiCad IPC is available, but the requested board is not open."
    } else {
        "KiCad IPC is available; open PCB documents are listed in open_boards."
    };

    Ok(CallToolResult::json(&json!({
        "kicad_ui_running": connected,
        "ipc_available": connected,
        "ipc_address": ctx.config.ipc_address,
        "ipc_failure": crate::tools::ipc_failure_evidence(&ping),
        "open_board_count": open_boards.len(),
        "open_boards": open_boards,
        "open_boards_error": open_boards_error,
        "requested_path": requested_path,
        "requested_board": requested_board,
        "requested_open": requested_open,
        "requested_check_error": requested_check_error,
        "message": message
    })))
}

/// The headline for an `open_project` call KiCad did not answer.
///
/// Chosen from the typed failure, so the recovery step it names is the one
/// that applies: "start KiCad" is wrong advice when KiCad is listening and
/// refused this account.
fn unanswered_message(ping: &konnect_ipc::PingOutcome) -> &'static str {
    use konnect_ipc::{PingOutcome, UnreachableReason};
    match ping {
        PingOutcome::Unreachable {
            reason: UnreachableReason::AccessDenied,
            ..
        } => {
            "The KiCad IPC endpoint refused this account, likely a different account or a \
             restrictive ACL; run Konnect as the same operating-system user as KiCad. See \
             ipc_failure."
        }
        PingOutcome::Unreachable {
            reason: UnreachableReason::HandshakeFailed,
            ..
        } => {
            "A listener at the KiCad IPC address did not complete NNG's handshake, so it is \
             probably not KiCad; see ipc_failure."
        }
        PingOutcome::RequestFailed { .. } => {
            "The KiCad IPC request did not complete and may have reached the endpoint; a KiCad \
             status in ipc_failure proves receipt, and KiCad may still be starting."
        }
        PingOutcome::Responsive | PingOutcome::Unreachable { .. } => {
            "KiCad IPC is not reachable. Start KiCad and enable the IPC API, or work in file-only mode."
        }
    }
}

async fn handle_save_project(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let ipc = konnect_ipc::KiCadIpcClient::new(&ctx.config.ipc_address);
    ipc.save_board()?;
    Ok(CallToolResult::text("Board saved successfully."))
}

async fn handle_get_project_info(
    args: &serde_json::Value,
    _ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let path = get_path(args, "path")?;

    // Demonstration of the structured-error pattern: returning a FileNotFound
    // kind lets clients branch (e.g. show a file picker) without string-parsing
    // the message.
    if !path.exists() {
        return Ok(CallToolResult::error_kind(
            crate::mcp::error::ToolErrorKind::FileNotFound {
                path: path.display().to_string(),
            },
            format!("Project file not found: {}", path.display()),
        ));
    }

    let content = tokio::fs::read_to_string(&path).await?;
    let pro: serde_json::Value = serde_json::from_str(&content)?;

    let dir = path.parent().unwrap_or(&path);
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let sch = dir.join(format!("{}.kicad_sch", stem));
    let pcb = dir.join(format!("{}.kicad_pcb", stem));

    let meta = tokio::fs::metadata(&path).await.ok();
    let modified = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let schematic_generator_version = read_generator_version(&sch).await;
    let pcb_generator_version = read_generator_version(&pcb).await;
    let kicad_version = match (
        schematic_generator_version.as_deref(),
        pcb_generator_version.as_deref(),
    ) {
        (Some(schematic), Some(pcb)) if schematic == pcb => Some(schematic.to_string()),
        (Some(schematic), None) => Some(schematic.to_string()),
        (None, Some(pcb)) => Some(pcb.to_string()),
        _ => None,
    };

    Ok(CallToolResult::json(&json!({
        "name": stem,
        "path": path.display().to_string(),
        "schematic": sch.display().to_string(),
        "schematic_exists": sch.exists(),
        "pcb": pcb.display().to_string(),
        "pcb_exists": pcb.exists(),
        "last_modified_unix": modified,
        "project_file_version": pro.get("meta").and_then(|m| m.get("version")),
        "schematic_generator_version": schematic_generator_version,
        "pcb_generator_version": pcb_generator_version,
        "kicad_version": kicad_version
    })))
}

async fn read_generator_version(path: &Path) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    let root = konnect_sexp::parse_sexp(&content).ok()?;
    root.find_str("generator_version").map(str::to_owned)
}

async fn handle_snapshot_project(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let schematic = get_path(args, "schematic")?;
    let output_dir = get_path(args, "output_dir")?;
    let label = opt_str(args, "label").unwrap_or("snapshot");

    tokio::fs::create_dir_all(&output_dir).await?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let stem = schematic
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let pdf_name = format!("{}_{}_{}.pdf", stem, label, ts);
    let pdf_path = output_dir.join(&pdf_name);

    crate::tools::cli::export_schematic_pdf(
        &ctx.config.kicad_cli,
        &schematic,
        &pdf_path,
        &crate::tools::cli::SchematicPdfOptions::default(),
    )
    .await?;

    let mut result = json!({
        "snapshot": pdf_path.display().to_string(),
        "label": label,
        "timestamp": ts
    });

    // Optionally snapshot PCB too
    if let Some(pcb_str) = opt_str(args, "pcb") {
        let pcb = PathBuf::from(pcb_str);
        let pcb_pdf_name = format!("{}_pcb_{}_{}.pdf", stem, label, ts);
        let pcb_pdf_path = output_dir.join(&pcb_pdf_name);
        let layers = &["F.Cu", "B.Cu", "F.Silkscreen", "B.Silkscreen", "Edge.Cuts"];
        crate::tools::cli::export_pdf(&ctx.config.kicad_cli, &pcb, &pcb_pdf_path, layers, false)
            .await?;
        result["pcb_snapshot"] = json!(pcb_pdf_path.display().to_string());
    }

    Ok(CallToolResult::json(&result))
}

async fn handle_open_viewer(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let sch_path = get_path(args, "schematic")?;

    if !sch_path.exists() {
        return Ok(CallToolResult::error(format!(
            "File not found: {}",
            sch_path.display()
        )));
    }

    // Find the viewer binary — it should be next to the konnect binary
    let viewer_binary = find_viewer_binary();

    match viewer_binary {
        Some(viewer_path) => {
            tracing::info!(
                "[BETA] Launching schematic viewer: {} {}",
                viewer_path.display(),
                sch_path.display()
            );

            // Spawn as detached process, forwarding the configured kicad-cli
            // path so the viewer renders with the same binary we use.
            let mut cmd = std::process::Command::new(&viewer_path);
            if !ctx.config.kicad_cli.is_empty() {
                cmd.arg("--kicad-cli").arg(&ctx.config.kicad_cli);
            }
            cmd.arg(&sch_path);

            match launch_viewer(cmd, VIEWER_STARTUP_WINDOW).await {
                Ok(()) => Ok(CallToolResult::text(
                    serde_json::to_string(&json!({
                        "launched": true,
                        "viewer": viewer_path.to_str().unwrap_or(""),
                        "schematic": sch_path.to_str().unwrap_or(""),
                        "note": "Schematic viewer opened. It will auto-refresh as you make changes to the schematic file."
                    }))
                    .unwrap(),
                )),
                Err(message) => Ok(CallToolResult::error(message)),
            }
        }
        None => Ok(CallToolResult::error(
            "Schematic viewer binary (schematic-viewer.exe) not found. \
             It should be in the same directory as konnect.exe.",
        )),
    }
}

/// How long the viewer must stay up before it counts as launched. A viewer
/// that cannot open a window, such as one started without a display on Linux,
/// exits well inside this (#702).
const VIEWER_STARTUP_WINDOW: Duration = Duration::from_secs(1);

/// Stderr lines quoted when the viewer exits during startup, and the length
/// each is cut to, so a huge panic message cannot flood the tool result.
const VIEWER_STDERR_TAIL_LINES: usize = 20;
const VIEWER_STDERR_LINE_CHARS: usize = 500;

/// How long to wait for the rest of a dead viewer's stderr. A `kicad-cli`
/// render the viewer started can hold the pipe open after the viewer exits.
const VIEWER_STDERR_DRAIN: Duration = Duration::from_millis(500);

/// Spawns the viewer and watches it for `window`. A successful `spawn()` only
/// means `exec` worked, so an exit inside the window is a failed launch,
/// reported with the exit status and the tail of the viewer's stderr.
async fn launch_viewer(mut cmd: std::process::Command, window: Duration) -> Result<(), String> {
    use std::collections::VecDeque;
    use std::process::Stdio;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncBufReadExt;

    // stdin and stdout carry the stdio transport's JSON-RPC stream, so the
    // viewer must not hold them. Its stderr still reaches the server log.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // Not `kill_on_drop`: the viewer outlives this call.
    let mut child = tokio::process::Command::from(cmd)
        .spawn()
        .map_err(|e| format!("Failed to launch viewer: {e}"))?;

    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let forwarder = child.stderr.take().map(|stderr| {
        let tail = Arc::clone(&tail);
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).split(b'\n');
            while let Ok(Some(line)) = lines.next_segment().await {
                let line = String::from_utf8_lossy(&line).trim_end().to_string();
                if line.is_empty() {
                    continue;
                }
                tracing::warn!(target: "schematic_viewer", "{line}");
                // Once the caller has returned, nobody reads the tail.
                if Arc::strong_count(&tail) == 1 {
                    continue;
                }
                if let Ok(mut tail) = tail.lock() {
                    if tail.len() == VIEWER_STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(
                        line.chars()
                            .take(VIEWER_STDERR_LINE_CHARS)
                            .collect::<String>(),
                    );
                }
            }
        })
    });

    let status = match tokio::time::timeout(window, child.wait()).await {
        Err(_still_running) => return Ok(()),
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(format!("Failed to check the viewer process: {e}")),
    };

    if let Some(forwarder) = forwarder {
        let _ = tokio::time::timeout(VIEWER_STDERR_DRAIN, forwarder).await;
    }
    let tail = tail
        .lock()
        .map(|mut tail| tail.make_contiguous().join("\n"))
        .unwrap_or_default();
    Err(if tail.is_empty() {
        format!("Schematic viewer exited during startup ({status}) with no stderr output.")
    } else {
        format!("Schematic viewer exited during startup ({status}). Its stderr ended with:\n{tail}")
    })
}

fn find_viewer_binary() -> Option<std::path::PathBuf> {
    // Check next to the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        let dir = exe_path.parent()?;
        let viewer = dir.join(if cfg!(target_os = "windows") {
            "schematic-viewer.exe"
        } else {
            "schematic-viewer"
        });
        if viewer.exists() {
            return Some(viewer);
        }
    }

    // Check common locations
    let candidates = ["schematic-viewer.exe", "schematic-viewer"];
    for c in &candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ─── Blank file templates ──────────────────────────────────────────────────────

fn blank_kicad_pro(name: &str) -> String {
    format!(
        r#"{{
  "meta": {{
    "filename": "{name}.kicad_pro",
    "version": 1
  }},
  "board": {{
    "design_settings": {{}}
  }},
  "schematic": {{
    "legacy_lib_dir": "",
    "legacy_lib_list": []
  }}
}}
"#
    )
}

fn blank_kicad_sch() -> String {
    crate::tools::blank_schematic_template()
}

fn blank_kicad_pcb() -> &'static str {
    concat!(
        "(kicad_pcb\n",
        "\t(version 20260206)\n",
        "\t(generator \"konnect\")\n",
        "\t(generator_version \"10.0\")\n",
        "\t(general\n",
        "\t\t(thickness 1.6)\n",
        "\t)\n",
        "\t(paper \"A4\")\n",
        "\t(layers\n",
        "\t\t(0 \"F.Cu\" signal)\n",
        "\t\t(31 \"B.Cu\" signal)\n",
        "\t\t(32 \"B.Adhes\" user \"B.Adhesive\")\n",
        "\t\t(33 \"F.Adhes\" user \"F.Adhesive\")\n",
        "\t\t(34 \"B.Paste\" user)\n",
        "\t\t(35 \"F.Paste\" user)\n",
        "\t\t(36 \"B.SilkS\" user \"B.Silkscreen\")\n",
        "\t\t(37 \"F.SilkS\" user \"F.Silkscreen\")\n",
        "\t\t(38 \"B.Mask\" user)\n",
        "\t\t(39 \"F.Mask\" user)\n",
        "\t\t(40 \"Dwgs.User\" user \"User.Drawings\")\n",
        "\t\t(41 \"Cmts.User\" user \"User.Comments\")\n",
        "\t\t(44 \"Edge.Cuts\" user)\n",
        "\t\t(45 \"Margin\" user)\n",
        "\t\t(46 \"B.CrtYd\" user \"B.Courtyard\")\n",
        "\t\t(47 \"F.CrtYd\" user \"F.Courtyard\")\n",
        "\t\t(48 \"B.Fab\" user)\n",
        "\t\t(49 \"F.Fab\" user)\n",
        "\t)\n",
        "\t(setup\n",
        "\t\t(pad_to_mask_clearance 0.05)\n",
        "\t)\n",
        ")\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::error::extract_error_kind;
    use crate::router::ToolRouter;
    use crate::tools::ServerConfig;
    use std::sync::Arc;

    fn test_ctx() -> ToolContext {
        ToolContext::new(
            ServerConfig {
                kicad_cli: String::new(),
                kicad_binary: String::new(),
                ipc_address: String::new(),
                project_dir: None,
                jlcpcb_db_path: None,
                auto_load_toolsets: false,
                eager_toolsets: false,
            },
            Arc::new(ToolRouter::new()),
        )
    }

    // ─── Blank file templates ─────────────────────────────────────────────

    #[test]
    fn blank_kicad_pro_is_valid_json_with_name() {
        let content = blank_kicad_pro("my_board");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
        assert_eq!(parsed["meta"]["filename"], "my_board.kicad_pro");
    }

    #[test]
    fn blank_kicad_sch_has_expected_header() {
        let content = blank_kicad_sch();
        assert!(content.starts_with("(kicad_sch"));
        assert!(content.contains("(lib_symbols"));
    }

    #[test]
    fn blank_kicad_sch_has_root_uuid() {
        // Without a root (uuid ...) KiCAD's netlister silently drops every
        // wire-only net (symbol instance paths can't resolve).
        let content = blank_kicad_sch();
        assert!(content.contains("(uuid \""));
        let sch = {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("t.kicad_sch");
            std::fs::write(&path, &content).unwrap();
            konnect_schematic_editor::Schematic::load(&path).unwrap()
        };
        let uuid = sch.uuid.expect("blank schematic must carry a root uuid");
        assert_eq!(uuid.len(), 36, "expected a v4 uuid, got '{uuid}'");
    }

    #[test]
    fn blank_kicad_pcb_declares_core_layers() {
        let content = blank_kicad_pcb();
        assert!(content.contains("\"F.Cu\""));
        assert!(content.contains("\"B.Cu\""));
        assert!(content.contains("\"Edge.Cuts\""));
    }

    #[test]
    fn blank_kicad_pcb_uses_the_kicad_10_implicit_net_shape() {
        let content = blank_kicad_pcb();
        assert!(content.contains("(version 20260206)"));
        assert!(
            !content.contains("\n\t(net "),
            "KiCad 10 boards must not contain a legacy top-level net table"
        );
    }

    #[test]
    fn open_project_maps_a_project_file_to_its_board() {
        let requested = requested_board_path(&json!({ "path": "/work/voice.kicad_pro" }))
            .unwrap()
            .unwrap();
        assert_eq!(requested.0, "/work/voice.kicad_pro");
        assert_eq!(requested.1, PathBuf::from("/work/voice.kicad_pcb"));

        let board = requested_board_path(&json!({ "path": "/work/voice.kicad_pcb" }))
            .unwrap()
            .unwrap();
        assert_eq!(board.1, PathBuf::from("/work/voice.kicad_pcb"));
    }

    #[test]
    fn open_project_rejects_a_path_it_cannot_check_over_pcb_ipc() {
        let error = requested_board_path(&json!({ "path": "/work/voice.kicad_sch" }))
            .expect_err("schematic documents are not exposed by this IPC query");
        assert_eq!(
            extract_error_kind(&error).as_deref(),
            Some("invalid_argument")
        );
    }

    // ─── handle_create_project ─────────────────────────────────────────────

    #[tokio::test]
    async fn create_project_writes_all_three_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_ctx();
        let args = json!({
            "path": dir.path().to_str().unwrap(),
            "name": "widget"
        });

        let result = handle_create_project(&args, &ctx)
            .await
            .expect("handler should succeed");
        assert!(!result.is_error);

        assert!(dir.path().join("widget.kicad_pro").exists());
        assert!(dir.path().join("widget.kicad_sch").exists());
        assert!(dir.path().join("widget.kicad_pcb").exists());

        let pro_content = tokio::fs::read_to_string(dir.path().join("widget.kicad_pro"))
            .await
            .unwrap();
        let pro: serde_json::Value = serde_json::from_str(&pro_content).unwrap();
        assert_eq!(pro["meta"]["filename"], "widget.kicad_pro");
    }

    #[tokio::test]
    async fn create_project_rejects_kicad_pro_path_without_writing() {
        let ctx = test_ctx();
        for extension in ["kicad_pro", "KICAD_PRO"] {
            let dir = tempfile::tempdir().expect("tempdir");
            let project_dir = dir.path().join("newproj");
            let project_file = project_dir.join(format!("newproj.{extension}"));
            let args = json!({
                "path": project_file.to_str().unwrap(),
                "name": "newproj"
            });

            let result = handle_create_project(&args, &ctx)
                .await
                .expect("handler should return a structured invalid argument");

            assert!(result.is_error);
            assert_eq!(
                extract_error_kind(&result).as_deref(),
                Some("invalid_argument")
            );
            let body = response_json(&result);
            assert_eq!(body["error"]["field"], "path");
            assert!(
                body["error"]["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("containing directory")
                        && reason.contains("'name'")),
                "{body}"
            );
            assert!(!project_dir.exists());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        }
    }

    #[tokio::test]
    async fn create_project_rejects_a_partially_populated_directory_without_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let existing_path = dir.path().join("widget.kicad_sch");
        tokio::fs::write(&existing_path, b"existing schematic")
            .await
            .unwrap();
        let ctx = test_ctx();
        let args = json!({
            "path": dir.path().to_str().unwrap(),
            "name": "widget"
        });

        let result = handle_create_project(&args, &ctx)
            .await
            .expect("handler should return a structured conflict");

        assert!(result.is_error);
        assert_eq!(extract_error_kind(&result).as_deref(), Some("conflict"));
        let body = response_json(&result);
        assert_eq!(
            body["error"]["paths"],
            json!([existing_path.display().to_string()])
        );
        assert_eq!(
            tokio::fs::read(&existing_path).await.unwrap(),
            b"existing schematic"
        );
        assert!(!dir.path().join("widget.kicad_pro").exists());
        assert!(!dir.path().join("widget.kicad_pcb").exists());
    }

    #[tokio::test]
    async fn create_project_rejects_a_completed_project_without_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_ctx();
        let args = json!({
            "path": dir.path().to_str().unwrap(),
            "name": "widget"
        });
        handle_create_project(&args, &ctx)
            .await
            .expect("initial creation should succeed");
        let paths = [
            dir.path().join("widget.kicad_pro"),
            dir.path().join("widget.kicad_sch"),
            dir.path().join("widget.kicad_pcb"),
        ];
        let before = [
            tokio::fs::read(&paths[0]).await.unwrap(),
            tokio::fs::read(&paths[1]).await.unwrap(),
            tokio::fs::read(&paths[2]).await.unwrap(),
        ];

        let result = handle_create_project(&args, &ctx)
            .await
            .expect("repeat creation should return a conflict");

        assert!(result.is_error);
        assert_eq!(extract_error_kind(&result).as_deref(), Some("conflict"));
        let body = response_json(&result);
        assert_eq!(body["error"]["paths"].as_array().unwrap().len(), 3);
        for (path, original) in paths.iter().zip(before) {
            assert_eq!(tokio::fs::read(path).await.unwrap(), original);
        }
    }

    #[tokio::test]
    async fn create_project_missing_name_returns_structured_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_ctx();
        let args = json!({ "path": dir.path().to_str().unwrap() });

        let result = handle_create_project(&args, &ctx)
            .await
            .expect("handler should return Ok even on validation failure");
        assert!(result.is_error);
        assert_eq!(
            extract_error_kind(&result).as_deref(),
            Some("invalid_argument")
        );
    }

    // ─── handle_get_project_info ───────────────────────────────────────────

    #[tokio::test]
    async fn get_project_info_reports_existing_sibling_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_ctx();
        let create_args = json!({
            "path": dir.path().to_str().unwrap(),
            "name": "widget"
        });
        handle_create_project(&create_args, &ctx)
            .await
            .expect("setup: create_project should succeed");

        let pro_path = dir.path().join("widget.kicad_pro");
        let info_args = json!({ "path": pro_path.to_str().unwrap() });
        let result = handle_get_project_info(&info_args, &ctx)
            .await
            .expect("handler should succeed");
        assert!(!result.is_error);

        let body = match &result.content[0] {
            crate::mcp::protocol::ToolContent::Text { text } => text.clone(),
            _ => panic!("expected text content"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["name"], "widget");
        assert_eq!(parsed["schematic_exists"], true);
        assert_eq!(parsed["pcb_exists"], true);
        assert_eq!(parsed["project_file_version"], 1);
        assert_eq!(parsed["schematic_generator_version"], "10.0");
        assert_eq!(parsed["pcb_generator_version"], "10.0");
        assert_eq!(parsed["kicad_version"], "10.0");
    }

    #[tokio::test]
    async fn get_project_info_does_not_claim_a_version_when_siblings_disagree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_ctx();
        let create_args = json!({
            "path": dir.path().to_str().unwrap(),
            "name": "widget"
        });
        handle_create_project(&create_args, &ctx)
            .await
            .expect("setup: create_project should succeed");

        let pcb_path = dir.path().join("widget.kicad_pcb");
        let pcb = std::fs::read_to_string(&pcb_path).expect("read generated board");
        std::fs::write(
            &pcb_path,
            pcb.replace("generator_version \"10.0\"", "generator_version \"10.1\""),
        )
        .expect("write board with a distinct generator version");

        let pro_path = dir.path().join("widget.kicad_pro");
        let info_args = json!({ "path": pro_path.to_str().unwrap() });
        let result = handle_get_project_info(&info_args, &ctx)
            .await
            .expect("handler should succeed");
        let parsed = response_json(&result);

        assert_eq!(parsed["schematic_generator_version"], "10.0");
        assert_eq!(parsed["pcb_generator_version"], "10.1");
        assert!(parsed["kicad_version"].is_null());
    }

    #[tokio::test]
    async fn get_project_info_missing_file_returns_file_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_ctx();
        let missing = dir.path().join("does_not_exist.kicad_pro");
        let args = json!({ "path": missing.to_str().unwrap() });

        let result = handle_get_project_info(&args, &ctx)
            .await
            .expect("handler should return Ok with a structured error body");
        assert!(result.is_error);
        assert_eq!(
            extract_error_kind(&result).as_deref(),
            Some("file_not_found")
        );
    }

    #[tokio::test]
    async fn snapshot_propagates_a_missing_pcb_artifact() {
        let dir = tempfile::tempdir().unwrap();
        // Produce the first (schematic) PDF, then report success without
        // producing the PCB PDF. This is the exact phantom-path failure #252
        // described, independent of whether a real KiCad is installed.
        let cli = crate::tools::cli::test_support::schematic_only_cli(dir.path());

        let schematic = dir.path().join("voice.kicad_sch");
        let board = dir.path().join("voice.kicad_pcb");
        std::fs::write(&schematic, "placeholder").unwrap();
        std::fs::write(&board, "placeholder").unwrap();
        let ctx = ToolContext::new(
            ServerConfig {
                kicad_cli: cli.display().to_string(),
                kicad_binary: String::new(),
                ipc_address: String::new(),
                project_dir: None,
                jlcpcb_db_path: None,
                auto_load_toolsets: false,
                eager_toolsets: false,
            },
            Arc::new(ToolRouter::new()),
        );

        let error = handle_snapshot_project(
            &json!({
                "schematic": schematic.display().to_string(),
                "pcb": board.display().to_string(),
                "output_dir": dir.path().join("snapshots").display().to_string(),
                "label": "regression"
            }),
            &ctx,
        )
        .await
        .expect_err("missing PCB PDF must fail the snapshot call");
        assert!(error.to_string().contains("did not create"), "{error:#}");
        assert!(error.to_string().contains("pcb"), "{error:#}");
    }

    #[tokio::test]
    async fn open_project_reports_why_kicad_did_not_answer() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = test_ctx();
        ctx.config.ipc_address =
            format!("ipc://{}", dir.path().join("no-kicad-here.sock").display());

        let result = handle_open_project(&json!({}), &ctx).await.unwrap();
        let response = response_json(&result);

        assert_eq!(response["ipc_available"], false, "{response}");
        assert_eq!(response["ipc_failure"]["kind"], "no_listener", "{response}");
        assert!(
            response["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("KiCad IPC is not reachable")),
            "{response}"
        );
    }

    /// A rep0 endpoint that answers every request with `AS_NOT_READY`, the
    /// status KiCad returns while an editor is still loading.
    fn spawn_not_ready_kicad() -> String {
        use nng::options::Options;
        use prost::Message;
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let url = format!("tcp://127.0.0.1:{port}");
        let socket = nng::Socket::new(nng::Protocol::Rep0).expect("mock rep socket");
        socket
            .set_opt::<nng::options::RecvTimeout>(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        socket.listen(&url).expect("mock listen");
        std::thread::spawn(move || {
            while socket.recv().is_ok() {
                let response = konnect_ipc::gen::kiapi::common::ApiResponse {
                    status: Some(konnect_ipc::gen::kiapi::common::ApiResponseStatus {
                        status: konnect_ipc::gen::kiapi::common::ApiStatusCode::AsNotReady as i32,
                        error_message: "KiCad is not ready".to_string(),
                    }),
                    header: None,
                    message: None,
                };
                let out = nng::Message::from(response.encode_to_vec().as_slice());
                if socket.send(out).is_err() {
                    break;
                }
            }
        });
        url
    }

    #[tokio::test]
    async fn open_project_says_kicad_answered_when_it_answered_with_an_error() {
        let mut ctx = test_ctx();
        ctx.config.ipc_address = spawn_not_ready_kicad();

        let result = handle_open_project(&json!({}), &ctx).await.unwrap();
        let response = response_json(&result);

        assert_eq!(response["ipc_available"], false, "{response}");
        assert_eq!(
            response["ipc_failure"]["kind"], "request_failed",
            "{response}"
        );
        assert!(
            response["ipc_failure"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("KiCad is not ready")),
            "{response}"
        );
        assert!(
            response["message"].as_str().is_some_and(
                |message| message.starts_with("The KiCad IPC request did not complete")
            ),
            "a KiCad that answered must not be reported as unreachable: {response}"
        );
    }

    #[test]
    fn the_open_project_headline_follows_the_failure_kind() {
        use konnect_ipc::{PingOutcome, UnreachableReason};
        let unreachable = |reason| PingOutcome::Unreachable {
            reason,
            message: String::new(),
        };
        assert!(
            unanswered_message(&unreachable(UnreachableReason::AccessDenied))
                .contains("same operating-system user")
        );
        assert!(
            unanswered_message(&unreachable(UnreachableReason::HandshakeFailed))
                .contains("probably not KiCad")
        );
        assert!(unanswered_message(&PingOutcome::RequestFailed {
            message: String::new()
        })
        .contains("did not complete"));
        for reason in [
            UnreachableReason::NotConfigured,
            UnreachableReason::NoListener,
            UnreachableReason::TransportError,
        ] {
            assert!(
                unanswered_message(&unreachable(reason)).starts_with("KiCad IPC is not reachable")
            );
        }
    }

    fn response_json(result: &CallToolResult) -> serde_json::Value {
        match &result.content[0] {
            crate::mcp::protocol::ToolContent::Text { text } => serde_json::from_str(text).unwrap(),
            _ => panic!("expected text content"),
        }
    }
}

// ─── rename_project ──────────────────────────────────────────────────────────

/// Project files that carry the project name, in the order they are renamed.
const PROJECT_EXTS: [&str; 4] = ["kicad_pro", "kicad_sch", "kicad_pcb", "kicad_prl"];

/// Every other `.kicad_sch` in the project directory — the child sheets.
///
/// A hierarchical design keeps each sheet in its own file, and **every one of
/// them** stores `(project "NAME"` on its symbol instances: KiCad's own
/// `complex_hierarchy` demo has 46 of them in `ampli_ht.kicad_sch` alone,
/// which is not named after the project and so is never renamed. Rewriting
/// only the root sheet leaves those pointing at the old name, and KiCad reads
/// their symbols as unannotated — the exact failure this tool exists to
/// prevent, moved from the root sheet to the children.
///
/// Child sheets are rewritten in place; they are not renamed, since their
/// names are referenced by `(sheet … (property "Sheetfile" …))` entries.
fn sibling_sheets(
    dir: &std::path::Path,
    already: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut sheets: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("kicad_sch"))
        .filter(|p| !already.contains(p))
        .collect();
    // Directory order is filesystem-defined; a stable report beats a shuffled one.
    sheets.sort();
    sheets
}

/// Rewrite the project name only where it is structurally meaningful.
///
/// A blind `text.replace(old, new)` corrupts unrelated content, and the blast
/// radius scales with how ordinary the name is: a project called `led` would
/// rewrite the footprint `LED_THT:LED_D3.0mm`, a net named `LED_LIGHT` and
/// every value containing the substring, silently, across the whole board and
/// schematic.
///
/// Every real occurrence is quoted, and the two file families need different
/// care:
///
/// - `.kicad_sch` / `.kicad_pcb` hold user content, so only `(project "NAME"`
///   is touched — the key symbol instances hang their annotation off. Getting
///   this wrong is what makes KiCad treat the design as unannotated.
/// - `.kicad_pro` / `.kicad_prl` are settings/metadata with no netlist in
///   them, so quoted whole-string matches and `NAME.kicad_*` filenames are
///   safe there.
fn rewrite_project_references(text: &str, old: &str, new: &str, sexp: bool) -> (String, usize) {
    let mut out = text.to_string();
    let mut hits = 0usize;

    let swap = |out: &mut String, hits: &mut usize, from: &str, to: &str| {
        if from != to {
            let n = out.matches(from).count();
            if n > 0 {
                *hits += n;
                *out = out.replace(from, to);
            }
        }
    };

    if sexp {
        // The annotation key, and nothing else.
        swap(
            &mut out,
            &mut hits,
            &format!("(project \"{old}\""),
            &format!("(project \"{new}\""),
        );
    } else {
        for ext in PROJECT_EXTS {
            swap(
                &mut out,
                &mut hits,
                &format!("\"{old}.{ext}\""),
                &format!("\"{new}.{ext}\""),
            );
        }
        // Bare quoted name: the `name` field and the root sheet entry.
        swap(
            &mut out,
            &mut hits,
            &format!("\"{old}\""),
            &format!("\"{new}\""),
        );
    }

    (out, hits)
}

#[cfg(test)]
#[path = "project_rename_revision_tests.rs"]
mod rename_revision_tests;

struct RenameDocument {
    source: PathBuf,
    target: PathBuf,
    before: String,
    after: String,
    hits: usize,
}

#[derive(Default)]
struct RenameProgress {
    files: Vec<(PathBuf, PathBuf)>,
    rewrites: Vec<(PathBuf, usize)>,
    backup: Option<(PathBuf, PathBuf)>,
    directory: Option<(PathBuf, PathBuf)>,
    uncertain_move: Option<(PathBuf, PathBuf)>,
}

impl RenameProgress {
    fn changed(&self) -> bool {
        !self.files.is_empty()
            || !self.rewrites.is_empty()
            || self.backup.is_some()
            || self.directory.is_some()
    }

    fn current_path(&self, path: &Path) -> PathBuf {
        let path = self
            .files
            .iter()
            .find(|(from, _)| from == path)
            .map_or(path, |(_, to)| to.as_path());
        match &self.directory {
            Some((from, to)) if path.starts_with(from) => {
                to.join(path.strip_prefix(from).expect("prefix checked"))
            }
            _ => path.to_path_buf(),
        }
    }

    fn path_report(&self, path: &Path) -> serde_json::Value {
        let current = self.current_path(path);
        if let Some((from, to)) = &self.uncertain_move {
            if let Ok(relative) = current.strip_prefix(from) {
                let alternative = if relative.as_os_str().is_empty() {
                    to.clone()
                } else {
                    to.join(relative)
                };
                return json!({
                    "source": path, "current_path": null,
                    "possible_paths": [current, alternative],
                });
            }
        }
        json!({"source": path, "current_path": current})
    }

    fn move_report(&self, from: &Path, to: &Path) -> serde_json::Value {
        let mut report = self.path_report(to);
        report
            .as_object_mut()
            .expect("path report is an object")
            .remove("source");
        report["from"] = json!(from);
        report["to"] = json!(to);
        report
    }

    fn report(&self, documents: &[RenameDocument]) -> serde_json::Value {
        json!({
            "moved_files": self.files.iter().map(|(from, to)|
                self.move_report(from, to)).collect::<Vec<_>>(),
            "verified_rewrites": self.rewrites.iter().map(|(path, hits)| {
                let mut report = self.path_report(path);
                report.as_object_mut().expect("path report is an object").remove("source");
                report["references_updated"] = json!(hits);
                report
            }).collect::<Vec<_>>(),
            "current_paths": documents.iter().map(|doc|
                self.path_report(&doc.source)).collect::<Vec<_>>(),
            "backup_move": self.backup.as_ref().map(|(from, to)|
                self.move_report(from, to)),
            "directory_move": self.directory.as_ref().map(|(from, to)| json!({
                "from": from, "to": to,
            })),
            "unverified_move_attempt": self.uncertain_move.as_ref().map(|(from, to)| json!({
                "from": from, "to": to,
            })),
            "paths_basis": "completed moves only; inspect both paths of any unverified move attempt",
        })
    }
}

fn rename_outcome(
    mut result: CallToolResult,
    progress: &RenameProgress,
    documents: &[RenameDocument],
    target: &Path,
    status: crate::outcome::OutcomeStatus,
) -> CallToolResult {
    let was_error = result.is_error;
    if let Some(crate::mcp::protocol::ToolContent::Text { text }) = result.content.first_mut() {
        // RENAME_REFUSAL_NORMALIZATION_BEGIN
        let mut body = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| {
                if was_error {
                    json!({
                        "message": text.as_str(),
                        "error": ToolErrorKind::HandlerError { reason: text.clone() },
                    })
                } else {
                    json!({"message": text.as_str()})
                }
            });
        // RENAME_REFUSAL_NORMALIZATION_END
        // Keep existing structured error fields. Only the rename wrapper
        // corrects the shared helper's whole-operation no-change wording.
        // RENAME_PROJECT_LOCK_WORDING_BEGIN
        if matches!(
            body.pointer("/error/reason").and_then(serde_json::Value::as_str),
            Some("kicad_project_lock_present" | "kicad_project_lock_unreadable")
        ) {
            let detail = body["message"].as_str().unwrap_or_default();
            let message = detail.replace(
                "Konnect did not modify the project file.",
                "This ownership check refused further work. Any earlier completed rename steps remain applied as reported.",
            );
            body["message"] = json!(message);
        }
        // RENAME_PROJECT_LOCK_WORDING_END
        body["applied"] = progress.report(documents);
        if status == crate::outcome::OutcomeStatus::Partial {
            body["message"] = json!(format!(
                "Project rename stopped after partial work. No rollback was attempted. \
                     Inspect applied.current_paths before continuing; do not repeat the whole request. {}",
                body["message"].as_str().unwrap_or_default()
            ));
        }
        *text = body.to_string();
    }
    let complete = usize::from(status == crate::outcome::OutcomeStatus::Complete);
    crate::outcome::attach(
        result,
        crate::outcome::summary(
            status,
            progress.current_path(target).display().to_string(),
            "saved_file_readback",
            1,
            complete,
            1 - complete,
            match status {
                crate::outcome::OutcomeStatus::Complete => None,
                crate::outcome::OutcomeStatus::Failed => {
                    Some(crate::outcome::retry_whole_request())
                }
                _ => Some(crate::outcome::inspect_before_retry()),
            },
        ),
    )
}

fn rename_stopped(
    result: CallToolResult,
    progress: &RenameProgress,
    documents: &[RenameDocument],
    target: &Path,
) -> CallToolResult {
    let status = if progress.changed() {
        crate::outcome::OutcomeStatus::Partial
    } else {
        crate::outcome::OutcomeStatus::Failed
    };
    rename_outcome(result, progress, documents, target, status)
}

fn rename_conflict(path: &Path, reason: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error_kind(
        ToolErrorKind::Conflict {
            paths: vec![path.display().to_string()],
        },
        format!("Project rename refused at '{}': {reason}", path.display()),
    )
}

fn rename_uncertain(
    path: &Path,
    reason: impl std::fmt::Display,
    progress: &RenameProgress,
    documents: &[RenameDocument],
) -> CallToolResult {
    rename_outcome(
        CallToolResult::error_kind(
            ToolErrorKind::MutationOutcomeUncertain {
                operation: "rename_project".into(),
                path: path.display().to_string(),
                reason: reason.to_string(),
            },
            format!(
                "Project rename persistence is uncertain at '{}'. Known applied work is listed, \
         but the last attempt may also have changed state. Inspect files before continuing; \
         do not blindly retry. {reason}",
                path.display()
            ),
        ),
        progress,
        documents,
        path,
        crate::outcome::OutcomeStatus::Uncertain,
    )
}

fn rename_lock_error(error: SexpError) -> CallToolResult {
    match error {
        SexpError::KiCadEditorLocked {
            path,
            inspection_error,
            ..
        } => {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("kicad_pcb"))
            {
                CallToolResult::error_kind(ToolErrorKind::UnsafeFileFallback {
                    path: path.display().to_string(),
                    reason: if inspection_error.is_some() {
                        "kicad_lock_unreadable"
                    } else { "kicad_lock_present" }.into(),
                }, format!("KiCad ownership prevents renaming or rewriting '{}'. Close or recover the editor first.", path.display()))
            } else {
                rename_conflict(&path, "KiCad editor lock is present or cannot be inspected")
            }
        }
        error => CallToolResult::error(error.to_string()),
    }
}

async fn rename_authority(
    ctx: &ToolContext,
    project: &Path,
    documents: &[RenameDocument],
) -> anyhow::Result<Option<CallToolResult>> {
    if let Some(refusal) =
        super::pcb_board::refuse_if_project_locked_by_kicad(project, "project rename")
    {
        return Ok(Some(refusal));
    }
    for doc in documents {
        if let Err(error) =
            konnect_sexp::writer::ensure_kicad_design_document_is_closed(&doc.source)
        {
            return Ok(Some(rename_lock_error(error)));
        }
        if doc
            .source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("kicad_pcb"))
        {
            if let Some(refusal) =
                super::pcb_board::refuse_if_board_open_in_kicad(ctx, &doc.source, "project rename")
                    .await?
            {
                return Ok(Some(refusal));
            }
        }
    }
    Ok(None)
}

fn pending_rename_journal(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    // Observe names only. Even a malformed or symlink journal must not be
    // silently recovered or carried to a different project directory.
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".konnect-transaction-")
            && name.ends_with(".json")
            && !name.ends_with(".abandoned.json")
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths.into_iter().next())
}

/// Exclusive destination claim in the native rename operation itself.
/// There is deliberately no replacing, copying or link/unlink fallback.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn move_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use rustix::fs::{renameat_with, RenameFlags, CWD};
    renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(Into::into)
}

#[cfg(windows)]
fn move_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let wide = |path: &Path| -> std::io::Result<Vec<u16>> {
        let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path contains NUL",
            ));
        }
        value.push(0);
        Ok(value)
    };
    let from = wide(from)?;
    let to = wide(to)?;
    // SAFETY: both pointers refer to live, NUL-terminated UTF-16 buffers.
    // Zero flags forbid replacement, cross-volume copy and delayed moves.
    // Capture GetLastError immediately, before any other Windows operation.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn move_no_replace(_from: &Path, _to: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no native exclusive-move implementation for this target",
    ))
}

fn sync_rename_parent(path: &Path) -> std::io::Result<()> {
    // Windows MoveFileExW does not promise directory-fsync crash durability.
    // Do not emulate a directory flush with a copying/replacing move.
    #[cfg(unix)]
    std::fs::File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn prepare_rename_documents(
    dir: &Path,
    planned: &[(PathBuf, PathBuf)],
    old_name: &str,
    new_name: &str,
) -> anyhow::Result<Vec<RenameDocument>> {
    let mut paths = planned.to_vec();
    let mut children = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "kicad_sch")
            && !paths.iter().any(|(source, _)| source == &path)
        {
            children.push(path);
        }
    }
    children.sort();
    paths.extend(children.into_iter().map(|path| (path.clone(), path)));
    paths
        .into_iter()
        .map(|(source, target)| {
            anyhow::ensure!(
                std::fs::symlink_metadata(&source)?.file_type().is_file(),
                "rename source must be a regular file, not a symlink: {}",
                source.display()
            );
            let before = konnect_sexp::read_consistent(&source)?;
            let sexp = source
                .extension()
                .is_some_and(|ext| ext == "kicad_sch" || ext == "kicad_pcb");
            let (after, hits) = rewrite_project_references(&before, old_name, new_name, sexp);
            Ok(RenameDocument {
                source,
                target,
                before,
                after,
                hits,
            })
        })
        .collect()
}

async fn apply_project_rename(
    ctx: &ToolContext,
    dir: &Path,
    planned: &[(PathBuf, PathBuf)],
    old_name: &str,
    new_name: &str,
    rename_dir: bool,
) -> anyhow::Result<CallToolResult> {
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    }
    .canonicalize()?;
    let project = dir.join(format!("{old_name}.kicad_pro"));
    let planned: Vec<_> = planned
        .iter()
        .map(|(from, to)| {
            (
                dir.join(from.file_name().expect("planned filename")),
                dir.join(to.file_name().expect("planned filename")),
            )
        })
        .collect();
    let mut progress = RenameProgress::default();
    let documents = match prepare_rename_documents(&dir, &planned, old_name, new_name) {
        Ok(documents) => documents,
        Err(error) => {
            return Ok(rename_stopped(
                rename_conflict(&project, error),
                &progress,
                &[],
                &project,
            ))
        }
    };

    #[cfg(test)]
    rename_revision_tests::fire("before_moves", &project);
    // Recheck after snapshot preparation, before moving even the first path.
    if let Some(journal) = pending_rename_journal(&dir)? {
        return Ok(rename_stopped(
            rename_conflict(
                &journal,
                "pending transaction; use konnect transaction status/recover before renaming",
            ),
            &progress,
            &documents,
            &project,
        ));
    }
    if let Some(refusal) = rename_authority(ctx, &project, &documents).await? {
        return Ok(rename_stopped(refusal, &progress, &documents, &project));
    }
    for doc in &documents {
        match konnect_sexp::read_consistent(&doc.source) {
            Ok(current) if current == doc.before => {}
            _ => {
                return Ok(rename_stopped(
                    rename_conflict(
                        &doc.source,
                        "source cannot be read or no longer matches the prepared snapshot",
                    ),
                    &progress,
                    &documents,
                    &project,
                ))
            }
        }
    }
    for (from, to) in &planned {
        #[cfg(test)]
        rename_revision_tests::fire("before_move", to);
        // A refusal here may follow earlier successful filename moves.
        if let Err(error) = konnect_sexp::writer::ensure_kicad_design_document_is_closed(from) {
            return Ok(rename_stopped(
                rename_lock_error(error),
                &progress,
                &documents,
                &project,
            ));
        }
        if let Some(refusal) =
            super::pcb_board::refuse_if_project_locked_by_kicad(&project, "project rename")
        {
            return Ok(rename_stopped(refusal, &progress, &documents, &project));
        }
        if let Err(error) = move_no_replace(from, to) {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                return Ok(rename_stopped(
                    rename_conflict(to, error),
                    &progress,
                    &documents,
                    &project,
                ));
            }
            progress.uncertain_move = Some((from.clone(), to.clone()));
            return Ok(rename_uncertain(to, error, &progress, &documents));
        }
        progress.files.push((from.clone(), to.clone()));
        #[cfg(test)]
        rename_revision_tests::fire("after_move", to);
        if let Err(error) = sync_rename_parent(to) {
            return Ok(rename_uncertain(to, error, &progress, &documents));
        }
    }
    for doc in &documents {
        let path = &doc.target;
        #[cfg(test)]
        rename_revision_tests::fire("before_rewrite", path);
        if let Err(error) =
            konnect_sexp::writer::ensure_kicad_design_document_is_closed(&doc.source)
        {
            return Ok(rename_stopped(
                rename_lock_error(error),
                &progress,
                &documents,
                &project,
            ));
        }
        if let Err(error) = konnect_sexp::writer::ensure_kicad_design_document_is_closed(path) {
            return Ok(rename_stopped(
                rename_lock_error(error),
                &progress,
                &documents,
                &project,
            ));
        }
        if let Some(refusal) =
            super::pcb_board::refuse_if_project_locked_by_kicad(&project, "project rename").or_else(
                || {
                    super::pcb_board::refuse_if_project_locked_by_kicad(
                        &project.with_file_name(format!("{new_name}.kicad_pro")),
                        "project rename",
                    )
                },
            )
        {
            return Ok(rename_stopped(refusal, &progress, &documents, &project));
        }
        let mut attempted = false;
        // RENAME_GUARDED_TRANSACTION_BEGIN
        let result = konnect_sexp::transact_atomic(path, |current| {
            // RENAME_ORIGINAL_SNAPSHOT_BEGIN
            if current != doc.before {
                return Ok((current.to_owned(), false));
            }
            // RENAME_ORIGINAL_SNAPSHOT_END
            attempted = doc.hits > 0;
            Ok((doc.after.clone(), true))
        });
        // RENAME_GUARDED_TRANSACTION_END
        #[cfg(test)]
        let result = rename_revision_tests::after_write(path, result, attempted);
        match result {
            Ok(false) => {
                return Ok(rename_stopped(
                    rename_conflict(
                        path,
                        "file changed since the original snapshot; newer bytes were not replaced",
                    ),
                    &progress,
                    &documents,
                    &project,
                ))
            }
            Err(error @ SexpError::KiCadEditorLocked { .. }) => {
                return Ok(rename_stopped(
                    rename_lock_error(error),
                    &progress,
                    &documents,
                    &project,
                ))
            }
            Err(error) if attempted => {
                return Ok(rename_uncertain(path, error, &progress, &documents))
            }
            Err(error) => {
                return Ok(rename_stopped(
                    rename_conflict(path, error),
                    &progress,
                    &documents,
                    &project,
                ))
            }
            Ok(true) => {}
        }
        match konnect_sexp::read_consistent(path) {
            Ok(saved) if saved == doc.after => {}
            _ => {
                return Ok(rename_uncertain(
                    path,
                    "committed-file readback could not verify the intended bytes",
                    &progress,
                    &documents,
                ))
            }
        }
        if doc.hits > 0 {
            progress.rewrites.push((path.clone(), doc.hits));
        }
    }
    // Optional moves use the same exclusive destination claim and accounting.
    let backup_from = dir.join(format!("{old_name}-backups"));
    let mut extra_moves = Vec::new();
    if backup_from.is_dir() {
        extra_moves.push((
            "backup",
            backup_from,
            dir.join(format!("{new_name}-backups")),
        ));
    }
    if rename_dir && dir.file_name().and_then(|name| name.to_str()) == Some(old_name) {
        if let Some(parent) = dir.parent() {
            extra_moves.push(("directory", dir.clone(), parent.join(new_name)));
        }
    }
    for (kind, from, to) in extra_moves {
        #[cfg(test)]
        rename_revision_tests::fire("before_move", &to);
        if let Err(error) = move_no_replace(&from, &to) {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                return Ok(rename_stopped(
                    rename_conflict(&to, error),
                    &progress,
                    &documents,
                    &project,
                ));
            }
            progress.uncertain_move = Some((from, to.clone()));
            return Ok(rename_uncertain(&to, error, &progress, &documents));
        }
        if kind == "backup" {
            progress.backup = Some((from, to.clone()));
        } else {
            progress.directory = Some((from, to.clone()));
        }
        #[cfg(test)]
        rename_revision_tests::fire("after_move", &to);
        if let Err(error) = sync_rename_parent(&to) {
            return Ok(rename_uncertain(&to, error, &progress, &documents));
        }
    }
    // Directory relocation must not turn an earlier successful readback into
    // a claim that files still exist with those bytes at their final paths.
    for doc in &documents {
        let path = progress.current_path(&doc.source);
        if !konnect_sexp::read_consistent(&path).is_ok_and(|saved| saved == doc.after) {
            return Ok(rename_uncertain(
                &path,
                "final-path readback differs or could not be read",
                &progress,
                &documents,
            ));
        }
    }
    let result = CallToolResult::json(&json!({
        "dry_run": false, "old_name": old_name, "new_name": new_name,
        "files": planned.iter().map(|(from, to)| json!({
            "from": from.file_name().and_then(|name| name.to_str()),
            "to": to.file_name().and_then(|name| name.to_str()),
        })).collect::<Vec<_>>(),
        "content_rewrites": progress.rewrites.iter().map(|(path, hits)| json!({
            "file": path.file_name().and_then(|name| name.to_str()),
            "references_updated": hits,
        })).collect::<Vec<_>>(),
        "backups_folder": progress.backup.as_ref()
            .and_then(|(_, to)| to.file_name()).and_then(|name| name.to_str()),
        "directory": progress.directory.as_ref().map(|(_, to)| to),
    }));
    Ok(rename_outcome(
        result,
        &progress,
        &documents,
        &project,
        crate::outcome::OutcomeStatus::Complete,
    ))
}

async fn handle_rename_project(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> anyhow::Result<CallToolResult> {
    let pro_path = get_path(args, "project")?;
    let new_name = match require_str(args, "new_name") {
        Ok(v) => v.to_string(),
        Err(e) => return Ok(e),
    };
    let dry_run = args
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let rename_dir = args
        .get("rename_directory")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if !pro_path.exists() {
        return Ok(CallToolResult::error_kind(
            crate::mcp::error::ToolErrorKind::FileNotFound {
                path: pro_path.display().to_string(),
            },
            format!("Project file not found: {}", pro_path.display()),
        ));
    }
    let Some(dir) = pro_path.parent().map(std::path::Path::to_path_buf) else {
        return Ok(CallToolResult::error(
            "project path has no parent directory",
        ));
    };
    let Some(old_name) = pro_path.file_stem().and_then(|s| s.to_str()) else {
        return Ok(CallToolResult::error("project path has no file stem"));
    };
    let old_name = old_name.to_string();

    if new_name == old_name {
        return Ok(CallToolResult::json(&json!({
            "renamed": false, "note": "new_name matches the current name"
        })));
    }
    // A name with a path separator would move the project, not rename it.
    if new_name.contains('/') || new_name.contains('\\') {
        return Ok(CallToolResult::error_kind(
            crate::mcp::error::ToolErrorKind::InvalidArgument {
                field: "new_name".to_string(),
                reason: "must be a bare name, not a path".to_string(),
            },
            "new_name must not contain a path separator.",
        ));
    }

    let mut planned_files = Vec::new();
    let mut collisions = Vec::new();
    for ext in PROJECT_EXTS {
        let from = dir.join(format!("{old_name}.{ext}"));
        if !from.exists() {
            continue;
        }
        let to = dir.join(format!("{new_name}.{ext}"));
        if to.exists() {
            collisions.push(to.display().to_string());
        }
        planned_files.push((from, to));
    }
    if !collisions.is_empty() {
        return Ok(rename_stopped(
            rename_conflict(
                Path::new(&collisions[0]),
                format!(
                    "Refusing to rename: these target files already exist: {}",
                    collisions.join(", ")
                ),
            ),
            &RenameProgress::default(),
            &[],
            &pro_path,
        ));
    }

    if !dry_run {
        return apply_project_rename(ctx, &dir, &planned_files, &old_name, &new_name, rename_dir)
            .await;
    }

    // Preserve the existing preview contract, including zero-hit entries.
    // No move probes, authority calls or write attempts are made by dry_run.
    let mut rewritten = Vec::new();
    {
        let sources: Vec<std::path::PathBuf> =
            planned_files.iter().map(|(from, _)| from.clone()).collect();
        let mut previewed: Vec<(std::path::PathBuf, String)> = planned_files
            .iter()
            .map(|(from, to)| {
                (
                    from.clone(),
                    to.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect();
        // Child sheets keep their names, so the reported name is their own.
        for sheet in sibling_sheets(&dir, &sources) {
            let name = sheet
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            previewed.push((sheet, name));
        }

        for (source, reported_name) in &previewed {
            let sexp = matches!(
                source.extension().and_then(|s| s.to_str()),
                Some("kicad_sch" | "kicad_pcb")
            );
            let text = std::fs::read_to_string(source).unwrap_or_default();
            let (_, hits) = rewrite_project_references(&text, &old_name, &new_name, sexp);
            rewritten.push(json!({
                "file": reported_name,
                "references_updated": hits,
            }));
        }
    }

    // The auto-backup folder is named after the project too.
    let backups_from = dir.join(format!("{old_name}-backups"));
    let mut backups = serde_json::Value::Null;
    if backups_from.is_dir() {
        let backups_to = dir.join(format!("{new_name}-backups"));
        backups = json!(backups_to.file_name().and_then(|s| s.to_str()));
    }

    let mut directory = serde_json::Value::Null;
    if rename_dir && dir.file_name().and_then(|s| s.to_str()) == Some(old_name.as_str()) {
        if let Some(parent) = dir.parent() {
            let new_dir = parent.join(&new_name);
            if !new_dir.exists() {
                directory = json!(new_dir.display().to_string());
            }
        }
    }

    Ok(CallToolResult::json(&json!({
        "dry_run": dry_run,
        "old_name": old_name,
        "new_name": new_name,
        "files": planned_files.iter()
            .map(|(f, t)| json!({
                "from": f.file_name().and_then(|s| s.to_str()),
                "to": t.file_name().and_then(|s| s.to_str())
            }))
            .collect::<Vec<_>>(),
        "content_rewrites": rewritten,
        "backups_folder": backups,
        "directory": directory,
    })))
}

#[cfg(test)]
mod rename_rewrite_tests {
    use super::rewrite_project_references;

    /// The corruption this guards against. A project named `led` used to have
    /// every occurrence of that substring rewritten across the schematic —
    /// footprints, net names, values — because the rewrite was a bare
    /// `text.replace(old, new)`.
    #[test]
    fn an_ordinary_name_does_not_eat_unrelated_content() {
        let sch = r#"(kicad_sch
	(symbol (lib_id "Device:LED")
		(property "Footprint" "LED_THT:LED_D3.0mm")
		(property "Value" "led")
	)
	(label "LED_LIGHT" (at 10 20 0))
	(instances
		(project "led"
			(path "/abc" (reference "D1") (unit 1))
		)
	)
)
"#;
        let (out, hits) = rewrite_project_references(sch, "led", "dro", true);
        assert_eq!(hits, 1, "only the (project …) key should match");
        assert!(out.contains(r#"(project "dro""#));
        // Everything else survives untouched.
        assert!(out.contains(r#""LED_THT:LED_D3.0mm""#));
        assert!(out.contains(r#"(property "Value" "led")"#));
        assert!(out.contains(r#"(label "LED_LIGHT""#));
        assert!(out.contains(r#"(lib_id "Device:LED")"#));
    }

    #[test]
    fn sexp_files_rewrite_the_annotation_key() {
        let sch = "(instances\n\t(project \"old name\"\n\t\t(path \"/x\")\n\t)\n)\n";
        let (out, hits) = rewrite_project_references(sch, "old name", "new name", true);
        assert_eq!(hits, 1);
        assert!(out.contains("(project \"new name\""));
        assert!(!out.contains("old name"));
    }

    /// Settings files carry the name as a bare quoted string and inside
    /// `NAME.kicad_*` filenames; both must move or KiCad reopens the old paths.
    #[test]
    fn settings_files_rewrite_names_and_filenames() {
        let pro = r#"{
  "meta": { "filename": "old.kicad_pro" },
  "sheets": [ [ "uuid-1", "old" ] ],
  "schematic": { "filename": "old.kicad_sch" },
  "board": { "filename": "old.kicad_pcb" },
  "name": "old"
}
"#;
        let (out, hits) = rewrite_project_references(pro, "old", "new", false);
        assert!(hits >= 5, "expected every quoted form to move, got {hits}");
        for want in [
            "\"new.kicad_pro\"",
            "\"new.kicad_sch\"",
            "\"new.kicad_pcb\"",
            "\"uuid-1\", \"new\"",
            "\"name\": \"new\"",
        ] {
            assert!(out.contains(want), "missing {want} in:\n{out}");
        }
        assert!(!out.contains("\"old"), "an old reference survived:\n{out}");
    }

    /// A settings file must not have unrelated *unquoted* text touched, and a
    /// schematic must not have its quoted values touched at all.
    #[test]
    fn no_partial_word_matches() {
        let sch = "(property \"Value\" \"prototype\")\n(project \"proto\"\n";
        let (out, hits) = rewrite_project_references(sch, "proto", "final", true);
        assert_eq!(hits, 1);
        assert!(out.contains("\"prototype\""), "substring was eaten: {out}");
        assert!(out.contains("(project \"final\""));
    }

    #[test]
    fn a_no_op_rename_changes_nothing() {
        let sch = "(project \"same\"\n";
        let (out, hits) = rewrite_project_references(sch, "same", "same", true);
        assert_eq!(hits, 0);
        assert_eq!(out, sch);
    }
}

/// A hierarchical project keeps each sheet in its own file, and every one of
/// them stores `(project "NAME"` on its symbol instances. Only the root sheet
/// is named after the project, so a rename that rewrites just the renamed set
/// leaves every child sheet pointing at the old name — KiCad then reads those
/// symbols as unannotated, which is the failure this tool exists to prevent.
#[cfg(test)]
mod rename_hierarchy_tests {
    use super::*;
    use crate::router::ToolRouter;
    use crate::tools::ServerConfig;
    use std::sync::Arc;

    fn test_ctx() -> ToolContext {
        ToolContext::new(
            ServerConfig {
                kicad_cli: String::new(),
                kicad_binary: String::new(),
                ipc_address: String::new(),
                project_dir: None,
                jlcpcb_db_path: None,
                auto_load_toolsets: false,
                eager_toolsets: false,
            },
            Arc::new(ToolRouter::new()),
        )
    }

    /// Root sheet + one child, shaped like KiCad's `complex_hierarchy` demo:
    /// the child is not named after the project and carries its own
    /// `(project …)` instances.
    fn hierarchy(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let pro = dir.join(format!("{name}.kicad_pro"));
        std::fs::write(
            &pro,
            format!("{{\n  \"meta\": {{\n    \"filename\": \"{name}.kicad_pro\"\n  }},\n  \"sheets\": []\n}}\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("{name}.kicad_sch")),
            format!("(kicad_sch\n\t(uuid \"root\")\n\t(sheet\n\t\t(property \"Sheetfile\" \"ampli.kicad_sch\")\n\t)\n\t(symbol\n\t\t(lib_id \"Device:R\")\n\t\t(instances\n\t\t\t(project \"{name}\"\n\t\t\t\t(path \"/root\" (reference \"R1\") (unit 1))\n\t\t\t)\n\t\t)\n\t)\n)\n"),
        )
        .unwrap();
        // The child sheet: never renamed, and full of project references.
        std::fs::write(
            dir.join("ampli.kicad_sch"),
            format!("(kicad_sch\n\t(uuid \"child\")\n\t(symbol\n\t\t(lib_id \"Device:C\")\n\t\t(instances\n\t\t\t(project \"{name}\"\n\t\t\t\t(path \"/root/child\" (reference \"C1\") (unit 1))\n\t\t\t)\n\t\t)\n\t)\n\t(symbol\n\t\t(lib_id \"Device:R\")\n\t\t(instances\n\t\t\t(project \"{name}\"\n\t\t\t\t(path \"/root/child\" (reference \"R9\") (unit 1))\n\t\t\t)\n\t\t)\n\t)\n)\n"),
        )
        .unwrap();
        pro
    }

    fn body(result: &CallToolResult) -> serde_json::Value {
        match &result.content[0] {
            crate::mcp::protocol::ToolContent::Text { text } => serde_json::from_str(text).unwrap(),
            _ => panic!("expected text"),
        }
    }

    #[tokio::test]
    async fn renaming_rewrites_child_sheets_too() {
        let dir = tempfile::tempdir().unwrap();
        let pro = hierarchy(dir.path(), "oldproj");

        let result = handle_rename_project(
            &json!({ "project": pro.to_str().unwrap(), "new_name": "newproj" }),
            &test_ctx(),
        )
        .await
        .unwrap();
        assert!(!result.is_error, "{result:?}");

        // The child keeps its filename and gains the new project name.
        let child = std::fs::read_to_string(dir.path().join("ampli.kicad_sch")).unwrap();
        assert_eq!(
            child.matches("(project \"newproj\"").count(),
            2,
            "every child instance must follow the rename:\n{child}"
        );
        assert!(
            !child.contains("oldproj"),
            "no stale project reference may survive:\n{child}"
        );
        let root = std::fs::read_to_string(dir.path().join("newproj.kicad_sch")).unwrap();
        assert!(root.contains("(project \"newproj\""), "{root}");
        // The child sheet is referenced by name, so it must NOT be renamed.
        assert!(dir.path().join("ampli.kicad_sch").exists());
        assert!(root.contains("\"ampli.kicad_sch\""), "{root}");

        let reported: Vec<String> = body(&result)["content_rewrites"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["file"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(
            reported.iter().any(|f| f == "ampli.kicad_sch"),
            "the child sheet must be reported: {reported:?}"
        );
    }

    /// dry_run must preview the child sheets too, and write nothing.
    #[tokio::test]
    async fn dry_run_previews_child_sheets_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let pro = hierarchy(dir.path(), "oldproj");
        let before = std::fs::read_to_string(dir.path().join("ampli.kicad_sch")).unwrap();

        let result = handle_rename_project(
            &json!({ "project": pro.to_str().unwrap(), "new_name": "newproj",
                     "dry_run": true }),
            &test_ctx(),
        )
        .await
        .unwrap();
        assert!(!result.is_error, "{result:?}");

        let entry = body(&result)["content_rewrites"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == "ampli.kicad_sch")
            .cloned()
            .unwrap_or_else(|| panic!("child sheet missing from the preview: {:?}", body(&result)));
        assert_eq!(entry["references_updated"], json!(2), "{entry}");

        assert_eq!(
            std::fs::read_to_string(dir.path().join("ampli.kicad_sch")).unwrap(),
            before,
            "dry_run must not write"
        );
        assert!(pro.exists(), "dry_run must not rename");
    }
}

/// `open_schematic_viewer` used to report `launched: true` for any successful
/// `spawn()`, including a viewer that panicked at once for lack of a display
/// (#702). `sh` stands in for the viewer, so these run on Unix only.
#[cfg(all(test, unix))]
mod viewer_launch_tests {
    use super::launch_viewer;
    use std::time::Duration;

    fn sh(script: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    }

    /// The panic tao prints when GTK cannot open a display, then the exit
    /// code a Rust panic gives, as the issue's reproduction recorded them.
    #[tokio::test]
    async fn a_viewer_that_crashes_on_startup_is_a_failed_launch() {
        let err = launch_viewer(
            sh("echo 'starting' >&2; \
                echo 'Failed to initialize gtk backend!' >&2; exit 101"),
            Duration::from_secs(10),
        )
        .await
        .expect_err("an exit inside the window must not count as launched");
        assert!(err.contains("101"), "exit status missing: {err}");
        assert!(
            err.contains("Failed to initialize gtk backend!"),
            "stderr missing: {err}"
        );
    }

    #[tokio::test]
    async fn a_clean_exit_during_startup_is_still_a_failed_launch() {
        let err = launch_viewer(sh("exit 0"), Duration::from_secs(10))
            .await
            .expect_err("a viewer that exits shows no window");
        assert!(err.contains("no stderr output"), "{err}");
    }

    #[tokio::test]
    async fn only_the_tail_of_a_long_stderr_is_quoted() {
        let err = launch_viewer(
            sh("i=1; while [ $i -le 30 ]; do echo line$i >&2; i=$((i+1)); done; exit 1"),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(err.contains("line11\n") && err.ends_with("line30"), "{err}");
        assert!(!err.contains("line10\n"), "{err}");
    }

    #[tokio::test]
    async fn a_very_long_stderr_line_is_cut() {
        let err = launch_viewer(
            sh("printf '%2000s\\n' '' | tr ' ' a >&2; exit 1"),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(err.ends_with(&"a".repeat(500)), "{err}");
        assert!(!err.contains(&"a".repeat(501)), "{err}");
    }

    /// A `kicad-cli` render the viewer started can still be writing after
    /// the viewer itself has exited.
    #[tokio::test]
    async fn stderr_from_a_child_the_viewer_started_is_still_quoted() {
        let err = launch_viewer(
            sh("(sleep 0.2; echo 'render failed' >&2) & exit 1"),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(err.ends_with("render failed"), "{err}");
    }

    /// stdin and stdout are the stdio transport's JSON-RPC stream.
    #[tokio::test]
    async fn the_viewer_does_not_hold_the_servers_stdin_or_stdout() {
        let err = launch_viewer(
            sh(
                "[ /dev/fd/0 -ef /dev/null ] && [ /dev/fd/1 -ef /dev/null ] \\
                && echo detached >&2; exit 1",
            ),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(err.ends_with("detached"), "{err}");
    }

    #[tokio::test]
    async fn a_viewer_still_running_after_the_window_is_launched() {
        launch_viewer(sh("sleep 3"), Duration::from_millis(200))
            .await
            .expect("a viewer that outlives the window is up");
    }

    #[tokio::test]
    async fn a_missing_binary_is_a_failed_launch() {
        let err = launch_viewer(
            std::process::Command::new("/nonexistent/schematic-viewer"),
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(err.starts_with("Failed to launch viewer:"), "{err}");
    }
}
