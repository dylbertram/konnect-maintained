//! A deliberately version-pinned native close, not a process-killing fallback.
use super::{require_str, ToolDef};
use crate::mcp::{error::ToolErrorKind, protocol::CallToolResult};
use crate::native_specctra_bridge::{
    registration_root, registrations, validate_registration, Registration,
};
use crate::tool;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    registration: String,
    pid: u32,
    process_start_ticks: String,
    board_path: PathBuf,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{status}: {reason}")]
struct NativeRefusal {
    status: String,
    reason: String,
}

pub(super) fn tools() -> Vec<ToolDef> {
    vec![
        tool!(
            "get_editor_shutdown_target",
            "Observe one exact saved PCB Editor target through the opt-in native bridge. \
             Linux KiCad 10.0.8 only. Does not save or close anything. Use the returned \
             target unchanged with shutdown_editor. Refuses ambiguous editor instances.",
            json!({
                "type":"object",
                "properties":{"expected_board":{"type":"string","minLength":1}},
                "required":["expected_board"]
            }),
            |args, _ctx| async move { handle_get_target(args).await }
        ),
        tool!(
            "shutdown_editor",
            "Request a normal, non-forced close of one verified PCB Editor. Never saves \
             or discards work. Refuses unsaved work and ancillary frames/dialogs. Requires \
             an unchanged target from get_editor_shutdown_target and an external MCP server. \
             Linux KiCad 10.0.8 only. timeout_seconds bounds exit verification after the \
             native request; each bridge request also has a 32-second deadline. A timeout \
             or uncertain result is NOT proof of closure. Never retries the close request.",
            json!({
                "type":"object",
                "properties":{
                    "target":{"type":"object","properties":{
                        "registration":{"type":"string","minLength":1},
                        "pid":{"type":"integer","minimum":1,"maximum":4294967295u64},
                        "process_start_ticks":{"type":"string","pattern":"^[0-9]+$"},
                        "board_path":{"type":"string","minLength":1}
                    },"required":["registration","pid","process_start_ticks","board_path"]},
                    "timeout_seconds":{"type":"integer","minimum":1,"maximum":60,"default":10}
                },
                "required":["target"]
            }),
            |args, _ctx| async move { handle_shutdown(args).await }
        ),
    ]
}

async fn handle_get_target(args: &Value) -> Result<CallToolResult> {
    let board = match require_str(args, "expected_board") {
        Ok(board) => board,
        Err(result) => return Ok(result),
    };
    let result = tokio::time::timeout(
        Duration::from_secs(35),
        discover(&bridge_root()?, Path::new(board)),
    )
    .await;
    Ok(match result {
        Ok(Ok(target)) => CallToolResult::json(&json!({"target":target})),
        Ok(Err(error)) => native_error_result(&error, "refused"),
        Err(_) => refusal("timed_out", "editor target observation exceeded 35 seconds"),
    })
}

async fn handle_shutdown(args: &Value) -> Result<CallToolResult> {
    let target: Target = serde_json::from_value(args["target"].clone())?;
    let timeout = args["timeout_seconds"].as_u64().unwrap_or(10);
    Ok(
        match shutdown(&bridge_root()?, &target, Duration::from_secs(timeout)).await {
            Ok(value) => value,
            Err(error) => refusal("uncertain", error.to_string()),
        },
    )
}

fn bridge_root() -> Result<PathBuf> {
    registration_root().context("native bridge registration directory unavailable")
}

fn refusal(status: &str, reason: impl Into<String>) -> CallToolResult {
    CallToolResult::error_kind(
        ToolErrorKind::EditorShutdown {
            status: status.to_string(),
            reason: reason.into(),
        },
        format!("Editor shutdown {status}; no forced close or automatic retry was attempted."),
    )
}

fn native_error_result(error: &anyhow::Error, fallback: &str) -> CallToolResult {
    if let Some(native) = error.downcast_ref::<NativeRefusal>() {
        return refusal(&native.status, &native.reason);
    }
    refusal(fallback, error.to_string())
}

fn native_status(value: &Value) -> &str {
    let status = value["status"].as_str().unwrap_or("uncertain");
    if [
        "unsupported",
        "save_required",
        "wrong_target",
        "refused",
        "timed_out",
        "uncertain",
    ]
    .contains(&status)
    {
        status
    } else {
        "uncertain"
    }
}

fn load_registration(root: &Path, name: &str) -> Result<Registration> {
    if !name.starts_with("bridge-")
        || !name.ends_with(".json")
        || Path::new(name).file_name().and_then(|p| p.to_str()) != Some(name)
    {
        bail!("invalid bridge session identifier");
    }
    let path = root.join(name);
    // Discovery files contain credentials. Do not follow a substituted symlink.
    let metadata = std::fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 {
        bail!("invalid bridge registration file");
    }
    let registration: Registration = serde_json::from_slice(&std::fs::read(path)?)?;
    validate_registration(&registration)?;
    Ok(registration)
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(32))
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

async fn request(
    client: &reqwest::Client,
    registration: &Registration,
    operation: &str,
    board: &Path,
) -> Result<Value> {
    let response = client
        .post(format!("{}/v1/{operation}", registration.address))
        .bearer_auth(&registration.token)
        .json(&json!({"expected_board":board,"requester_pid":std::process::id()}))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(NativeRefusal {
            status: "unsupported".to_string(),
            reason: "this native bridge does not implement editor shutdown".to_string(),
        }
        .into());
    }
    // Bound untrusted local bridge output rather than buffering an arbitrary body.
    let mut response = response.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 16 * 1024 {
            bail!("oversized editor bridge response");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn validate_ready(value: &Value, registration: &Registration, board: &Path) -> Result<()> {
    if value["success"] != true || value["status"] != "ready" {
        return Err(NativeRefusal {
            status: native_status(value).to_string(),
            reason: value["reason"]
                .as_str()
                .unwrap_or("incomplete native observation")
                .to_string(),
        }
        .into());
    }
    if value["pid"].as_u64() != Some(u64::from(registration.pid))
        || value["kicad_version"] != "10.0.8"
        || value["board_path"].as_str().map(Path::new) != Some(board)
    {
        return Err(NativeRefusal {
            status: "wrong_target".to_string(),
            reason: "native observation does not match exact process/document identity".to_string(),
        }
        .into());
    }
    if registration.pid == std::process::id()
        || value["mcp_server_pid"].as_u64() == Some(u64::from(std::process::id()))
    {
        return Err(NativeRefusal {
            status: "refused".to_string(),
            reason: "closing this editor would terminate this embedded or plugin-owned MCP server"
                .to_string(),
        }
        .into());
    }
    Ok(())
}

async fn discover(root: &Path, board: &Path) -> Result<Target> {
    if !cfg!(target_os = "linux") {
        return Err(NativeRefusal {
            status: "unsupported".to_string(),
            reason: "verified process-exit observation requires Linux".to_string(),
        }
        .into());
    }
    let board = board.canonicalize()?;
    let entries = registrations(root)?;
    if entries.is_empty() {
        return Err(NativeRefusal {
            status: "unsupported".to_string(),
            reason: "no enabled native bridge registration was found".to_string(),
        }
        .into());
    }
    if entries.len() > 64 {
        bail!("too many bridge registrations; remove stale sessions explicitly");
    }
    let client = client()?;
    let mut targets = Vec::new();
    let mut diagnostics = Vec::new();
    let mut native_refusal = None;
    for entry in entries {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let result = async {
            let registration = load_registration(root, name)?;
            let ticks = process_start_ticks(registration.pid)?
                .context("registered editor has already exited")?;
            let value = request(&client, &registration, "editor-target", &board).await?;
            validate_ready(&value, &registration, &board)?;
            if process_start_ticks(registration.pid)?.as_ref() != Some(&ticks) {
                bail!("editor process changed during observation");
            }
            Ok::<_, anyhow::Error>(Target {
                registration: name.to_string(),
                pid: registration.pid,
                process_start_ticks: ticks,
                board_path: board.clone(),
            })
        }
        .await;
        match result {
            Ok(target) => targets.push(target),
            Err(error) => {
                if let Some(native) = error.downcast_ref::<NativeRefusal>() {
                    // A matching board's refusal is more useful than a different
                    // editor's wrong_target response.
                    if native_refusal.is_none() || native.status != "wrong_target" {
                        native_refusal = Some(native.clone());
                    }
                }
                diagnostics.push(error.to_string());
            }
        }
    }
    if targets.is_empty() {
        if let Some(native) = native_refusal {
            return Err(native.into());
        }
    }
    if targets.len() != 1 {
        bail!(
            "expected one verified editor, found {}; {}",
            targets.len(),
            diagnostics.join("; ")
        );
    }
    Ok(targets.remove(0))
}

async fn shutdown(root: &Path, target: &Target, timeout: Duration) -> Result<CallToolResult> {
    if !cfg!(target_os = "linux") {
        return Ok(refusal(
            "unsupported",
            "process-exit observation requires Linux",
        ));
    }
    // A missing registration or socket alone NEVER proves an editor has closed.
    if process_start_ticks(target.pid)?.as_ref() != Some(&target.process_start_ticks) {
        return Ok(CallToolResult::json(
            &json!({"status":"already_closed","target":target}),
        ));
    }
    let registration = load_registration(root, &target.registration)?;
    if registration.pid != target.pid {
        return Ok(refusal(
            "wrong_target",
            "bridge session no longer names the requested PID",
        ));
    }
    let client = client()?;
    let value = request(&client, &registration, "editor-target", &target.board_path).await?;
    if let Err(error) = validate_ready(&value, &registration, &target.board_path) {
        return Ok(native_error_result(&error, "uncertain"));
    }
    if process_start_ticks(target.pid)?.as_ref() != Some(&target.process_start_ticks) {
        return Ok(refusal(
            "uncertain",
            "process identity changed before native close",
        ));
    }
    // Exactly one delivery. A disconnect can be normal when KiCad exits, but is
    // never itself success. Always verify the particular process generation.
    let delivery = request(
        &client,
        &registration,
        "shutdown-editor",
        &target.board_path,
    )
    .await;
    if let Ok(value) = &delivery {
        if value["success"] != true || value["status"] != "requested" {
            return Ok(refusal(
                native_status(value),
                value["reason"]
                    .as_str()
                    .unwrap_or("native shutdown refused"),
            ));
        }
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if process_start_ticks(target.pid)?.as_ref() != Some(&target.process_start_ticks) {
            return Ok(CallToolResult::json(&json!({
                "status":"closed", "target":target,
                "evidence":"verified_linux_process_generation_exited",
                "bridge_response_received":delivery.is_ok()
            })));
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(refusal(
                if delivery.is_ok() {
                    "timed_out"
                } else {
                    "uncertain"
                },
                "target process remains alive; inspect its state before any retry",
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn process_start_ticks(pid: u32) -> Result<Option<String>> {
    if !cfg!(target_os = "linux") {
        bail!("unsupported: process-generation observation requires Linux");
    }
    let source = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    parse_process_stat(&source)
}

fn parse_process_stat(source: &str) -> Result<Option<String>> {
    // comm may contain spaces AND parentheses. Fields after its final ')' begin
    // at field 3 (state); starttime is field 22.
    let (_, tail) = source.rsplit_once(')').context("malformed process stat")?;
    let fields = tail.split_whitespace().collect::<Vec<_>>();
    let ticks = fields.get(19).context("missing process starttime")?;
    ticks.parse::<u64>().context("invalid process starttime")?;
    if matches!(fields.first(), Some(&"Z" | &"X")) {
        return Ok(None);
    }
    Ok(Some((*ticks).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_schema_has_no_force_discard_or_implicit_save_policy() {
        let definitions = tools();
        let shutdown = definitions
            .iter()
            .find(|tool| tool.name == "shutdown_editor")
            .unwrap();
        let valid = json!({"target":{
            "registration":"bridge-test.json","pid":42,
            "process_start_ticks":"12345","board_path":"/board.kicad_pcb"
        }});
        assert!(shutdown.input_validator.is_valid(&valid));
        for field in ["force", "discard", "save_policy"] {
            let mut invalid = valid.clone();
            invalid[field] = json!(true);
            assert!(!shutdown.input_validator.is_valid(&invalid), "{field}");
        }
        assert_eq!(native_status(&json!({"status":"invented"})), "uncertain");
    }

    #[test]
    fn embedded_and_plugin_owned_requesting_servers_are_refused() {
        let own_pid = std::process::id();
        for (editor_pid, child_pid) in [(own_pid, None), (own_pid + 1, Some(own_pid))] {
            let registration: Registration = serde_json::from_value(json!({
                "protocol_version":1,"pid":editor_pid,"address":"http://127.0.0.1:12345",
                "token":"t".repeat(32),"started_at_unix":1.0
            }))
            .unwrap();
            let value = json!({
                "success":true,"status":"ready","pid":editor_pid,
                "board_path":"/board.kicad_pcb","kicad_version":"10.0.8",
                "mcp_server_pid":child_pid
            });
            let error =
                validate_ready(&value, &registration, Path::new("/board.kicad_pcb")).unwrap_err();
            assert_eq!(
                error.downcast_ref::<NativeRefusal>().unwrap().status,
                "refused"
            );
        }
    }

    #[test]
    fn process_generation_handles_spaces_parentheses_and_zombies() {
        for state in ["S", "Z", "X"] {
            let mut fields = vec!["0"; 20];
            fields[0] = state;
            fields[19] = "12345";
            let stat = format!("42 (pcb (test)) {}", fields.join(" "));
            assert_eq!(
                parse_process_stat(&stat).unwrap(),
                (state == "S").then(|| "12345".to_string())
            );
        }
        assert!(parse_process_stat("42 invalid").is_err());
    }

    #[test]
    fn target_registration_cannot_escape_discovery_directory() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "../bridge-other.json",
            "/bridge-test.json",
            "bridge-a/../bridge-b.json",
        ] {
            assert!(load_registration(root.path(), name).is_err());
        }
    }

    #[tokio::test]
    async fn absent_bridge_is_unsupported_not_an_invented_editor_target() {
        let root = tempfile::tempdir().unwrap();
        let board = root.path().join("board.kicad_pcb");
        std::fs::write(&board, "(kicad_pcb)\n").unwrap();
        let error = discover(&root.path().join("missing"), &board)
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<NativeRefusal>().unwrap().status,
            "unsupported"
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn already_closed_does_not_contact_any_bridge() {
        let root = tempfile::tempdir().unwrap();
        let result = shutdown(
            root.path(),
            &Target {
                registration: "bridge-missing.json".into(),
                pid: std::process::id(),
                process_start_ticks: "0".into(),
                board_path: PathBuf::from("/no-board"),
            },
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        assert!(!result.is_error);
    }

    #[cfg(target_os = "linux")]
    struct MockProcess(std::process::Child);

    #[cfg(target_os = "linux")]
    impl Drop for MockProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[cfg(target_os = "linux")]
    async fn mock_target(
        responses: Vec<Option<&'static str>>,
    ) -> (
        tempfile::TempDir,
        Target,
        tokio::task::JoinHandle<()>,
        MockProcess,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let directory = tempfile::tempdir().unwrap();
        let board = directory.path().join("board.kicad_pcb");
        std::fs::write(&board, "(kicad_pcb)\n").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let process = MockProcess(
            std::process::Command::new("sleep")
                .arg("30")
                .spawn()
                .unwrap(),
        );
        let pid = process.0.id();
        let target = Target {
            registration: "bridge-test.json".to_string(),
            pid,
            process_start_ticks: process_start_ticks(pid).unwrap().unwrap(),
            board_path: board.canonicalize().unwrap(),
        };
        std::fs::write(
            directory.path().join(&target.registration),
            json!({
                "protocol_version":1,"pid":pid,"address":format!("http://127.0.0.1:{port}"),
                "token":"t".repeat(32),"started_at_unix":1.0
            })
            .to_string(),
        )
        .unwrap();
        let server = tokio::spawn(async move {
            for (index, status) in responses.into_iter().enumerate() {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 16384];
                let count = stream.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..count]);
                assert!(request.contains(if index == 0 {
                    "/v1/editor-target"
                } else {
                    "/v1/shutdown-editor"
                }));
                let Some(status) = status else {
                    continue; // Simulate transport loss, without closing the target.
                };
                let value = json!({
                    "success":matches!(status, "ready" | "requested"),
                    "status":status,"reason":"injected test outcome",
                    "pid":pid,"board_path":board,"kicad_version":"10.0.8",
                    "mcp_server_pid":null
                })
                .to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    value.len(), value
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (directory, target, server, process)
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn native_refusals_never_become_close_success() {
        for status in [
            "save_required",
            "wrong_target",
            "unsupported",
            "refused",
            "timed_out",
        ] {
            let (directory, target, server, _process) = mock_target(vec![Some(status)]).await;
            let result = shutdown(directory.path(), &target, Duration::from_millis(1))
                .await
                .unwrap();
            server.await.unwrap();
            assert!(result.is_error, "{status}");
            let crate::mcp::protocol::ToolContent::Text { text } = &result.content[0] else {
                panic!("expected structured error")
            };
            let value: Value = serde_json::from_str(text).unwrap();
            assert_eq!(value["error"]["status"], status);
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn accepted_close_and_transport_loss_require_process_exit_readback() {
        for (response, expected) in [(Some("requested"), "timed_out"), (None, "uncertain")] {
            let (directory, target, server, _process) =
                mock_target(vec![Some("ready"), response]).await;
            let result = shutdown(directory.path(), &target, Duration::from_millis(1))
                .await
                .unwrap();
            server.await.unwrap();
            assert!(result.is_error);
            let crate::mcp::protocol::ToolContent::Text { text } = &result.content[0] else {
                panic!("expected structured error")
            };
            let value: Value = serde_json::from_str(text).unwrap();
            assert_eq!(value["error"]["status"], expected);
        }
    }
}
