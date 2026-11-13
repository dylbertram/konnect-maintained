//! Served whole-file regression and coordinator-only native CLI acceptance.
//! Never opens or talks to a live KiCad editor.

use konnect_core::mcp::handler::McpHandler;
use konnect_core::tools::ServerConfig;
use konnect_sexp::parser::parse_sexp;
use konnect_sexp::schematic::extract_wires;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[path = "../../konnect-schematic-editor/tests/support/text_preservation.rs"]
mod fixture;

async fn handler() -> McpHandler {
    McpHandler::new(ServerConfig {
        kicad_cli: String::new(),
        kicad_binary: String::new(),
        ipc_address: String::new(),
        project_dir: None,
        jlcpcb_db_path: None,
        auto_load_toolsets: true,
        eager_toolsets: false,
    })
    .await
    .unwrap()
}

fn project(source: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixer.kicad_sch");
    std::fs::write(&path, source).unwrap();
    (directory, path)
}

async fn add_wire(path: &Path) -> String {
    let before = std::fs::read_to_string(path).unwrap();
    let wires = extract_wires(&parse_sexp(&before).unwrap());
    let matches_requested = |wire: &konnect_sexp::schematic::Wire| {
        (wire.x1 - 10.16).abs() < 1e-9
            && (wire.y1 - 10.16).abs() < 1e-9
            && (wire.x2 - 15.24).abs() < 1e-9
            && (wire.y2 - 10.16).abs() < 1e-9
    };
    assert!(!wires.iter().any(matches_requested), "wire already exists");
    let response = handler()
        .await
        .handle_message(json!({
            "jsonrpc":"2.0", "id":691, "method":"tools/call",
            "params":{
                "name":"add_wire",
                "arguments":{
                    "schematic":path,
                    "x1":10.16, "y1":10.16, "x2":15.24, "y2":10.16
                }
            }
        }))
        .await
        .unwrap();
    assert!(response.error.is_none(), "{response:?}");
    let result = response.result.unwrap();
    assert_ne!(result["isError"], true, "{result}");
    let payload: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        payload["added_wire"],
        json!({"x1":10.16, "y1":10.16, "x2":15.24, "y2":10.16})
    );
    let written = std::fs::read_to_string(path).unwrap();
    assert_ne!(written, before, "served edit must not be a no-op");
    let committed_wires = extract_wires(&parse_sexp(&written).unwrap());
    assert_eq!(committed_wires.len(), wires.len() + 1);
    let added: Vec<_> = committed_wires
        .iter()
        .filter(|wire| matches_requested(wire))
        .collect();
    assert_eq!(added.len(), 1, "committed requested wire exactly once");
    let uuid = added[0].uuid.as_deref().expect("new wire has a UUID");
    assert!(!wires.iter().any(|wire| wire.uuid.as_deref() == Some(uuid)));
    written
}

#[tokio::test]
async fn served_add_wire_preserves_native_text_blocks_exactly() {
    fixture::assert_state(fixture::NATIVE, Some(false));
    let (_directory, path) = project(fixture::NATIVE);
    let written = add_wire(&path).await;
    fixture::assert_exact_blocks(fixture::NATIVE, &written);
    fixture::assert_state(&written, Some(false));
}

#[tokio::test]
async fn served_add_wire_preserves_yes_no_absence_and_unknown_children() {
    for state in [Some(true), Some(false), None] {
        let source = fixture::with_state(state);
        fixture::assert_state(&source, state);
        let (_directory, path) = project(&source);
        let written = add_wire(&path).await;
        fixture::assert_state(&written, state);
        fixture::assert_exact_blocks(&source, &written);

        let source = fixture::with_unknown(state);
        fixture::assert_state(&source, state);
        let (_directory, path) = project(&source);
        let written = add_wire(&path).await;
        fixture::assert_state(&written, state);
        fixture::assert_unknown(&source, &written);
    }
}

/// The existing export supervisor owns the native process group and adopts
/// descendants. Its deadline survives a dropped Rust future. No shell or
/// blocking Command::output is hidden inside an async timeout.
async fn native(
    directory: &Path,
    phase: &str,
    cli: &str,
    seconds: u32,
    arguments: &[&str],
) -> (std::process::Output, Value) {
    if !cfg!(target_os = "linux") {
        panic!("native gate requires Linux");
    }
    assert!(Path::new(cli).is_absolute(), "select an explicit native CLI");
    let wrapper = directory.join(format!("text-native-{phase}.py"));
    std::fs::write(&wrapper, include_str!("support/pcb_export_supervisor.py")).unwrap();
    std::fs::write(
        wrapper.with_extension("json"),
        serde_json::to_vec(&json!({
            "program":cli, "seconds":seconds, "audit":wrapper.with_extension("audit")
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let output = tokio::process::Command::new(&wrapper)
        .args(arguments)
        .kill_on_drop(false)
        .output()
        .await
        .unwrap();
    std::fs::write(directory.join(format!("{phase}.stdout.log")), &output.stdout).unwrap();
    std::fs::write(directory.join(format!("{phase}.stderr.log")), &output.stderr).unwrap();
    let audits: Vec<_> = std::fs::read_dir(wrapper.with_extension("audit"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("finished.json"))
        .collect();
    assert_eq!(audits.len(), 1, "one invocation for phase {phase}");
    let audit: Value = serde_json::from_slice(&std::fs::read(&audits[0]).unwrap()).unwrap();
    assert_eq!(audit["group_reaped"], true, "{phase}: {audit}");
    assert_eq!(audit["timed_out"], false, "{phase}: {audit}");
    assert_eq!(audit["interrupted"], 0, "{phase}: {audit}");
    assert_eq!(audit["status"], 0, "{phase}: {audit}");
    assert!(output.status.success(), "{phase}: {output:?}");
    (output, audit)
}

fn pin_partitions(path: &Path) -> BTreeSet<BTreeSet<(String, String)>> {
    let source = std::fs::read_to_string(path).unwrap();
    let root = parse_sexp(&source).unwrap();
    let mut partitions = BTreeSet::new();
    for net in root
        .find("nets")
        .expect("native netlist has nets")
        .find_all("net")
    {
        let pins: BTreeSet<_> = net
            .find_all("node")
            .iter()
            .map(|node| {
                (
                    node.find_str("ref").unwrap().to_owned(),
                    node.find_str("pin").unwrap().to_owned(),
                )
            })
            .collect();
        if !pins.is_empty() {
            assert!(partitions.insert(pins), "duplicate native pin partition");
        }
    }
    assert!(
        !partitions.is_empty(),
        "native netlist oracle cannot be empty"
    );
    partitions
}

#[tokio::test]
#[ignore = "requires coordinator-admitted Linux native CLI, never the live editor"]
async fn native_resave_then_served_add_wire_keeps_text_and_connectivity() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").expect("explicit native CLI required");
    let artifacts = PathBuf::from(
        std::env::var_os("KONNECT_TEST_NATIVE_ARTIFACT_DIR").expect("native gate must retain artifacts"),
    );
    assert!(artifacts.is_absolute(), "use an absolute artifact directory");
    std::fs::create_dir_all(&artifacts).unwrap();
    assert!(
        std::fs::read_dir(&artifacts).unwrap().next().is_none(),
        "native artifact directory must be empty"
    );
    let (directory, path) = project(fixture::NATIVE);
    let path_string = path.to_str().unwrap();
    // Keep the supervisor and audits outside TempDir, so dropping this future
    // cannot delete the cleanup owner's config or its eventual finished audit.
    let (version, version_audit) =
        native(&artifacts, "version", &cli, 10, &["--version"]).await;
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(!version.trim().is_empty());
    let (_, upgrade_audit) = native(
        &artifacts,
        "upgrade",
        &cli,
        45,
        &["sch", "upgrade", "--force", path_string],
    )
    .await;
    let captured = std::fs::read_to_string(&path).unwrap();
    assert!(!captured.contains('\r'), "actual native LF output required");
    fixture::assert_state(&captured, Some(false));
    // Capture actual KiCad bytes before the served edit, not a derivative or
    // Konnect's output. The native gate does not feed unknown syntax to KiCad.
    std::fs::copy(&path, artifacts.join("text_native_before.kicad_sch")).unwrap();
    let before_netlist = directory.path().join("before.net");
    let (_, before_audit) = native(
        &artifacts,
        "netlist-before",
        &cli,
        45,
        &[
            "sch",
            "export",
            "netlist",
            "--format",
            "kicadsexpr",
            "--output",
            before_netlist.to_str().unwrap(),
            path_string,
        ],
    )
    .await;
    let provenance = json!({
        "native_version":version.trim(),
        "base_sha256":format!("{:x}", Sha256::digest(fixture::NATIVE.as_bytes())),
        "native_sha256":format!("{:x}", Sha256::digest(captured.as_bytes())),
        "command":["sch", "upgrade", "--force", "mixer.kicad_sch"],
        "scope":"native exclude_from_sim no only; no synthetic yes/absence/unknown input",
        "supervision":{
            "version_seconds":10, "upgrade_seconds":45, "netlist_seconds":45,
            "version":version_audit, "upgrade":upgrade_audit, "netlist_before":before_audit
        }
    });
    std::fs::write(
        artifacts.join("text_native_before.provenance.json"),
        serde_json::to_vec_pretty(&provenance).unwrap(),
    )
    .unwrap();
    let written = add_wire(&path).await;
    fixture::assert_exact_blocks(&captured, &written);
    fixture::assert_state(&written, Some(false));
    let after_netlist = directory.path().join("after.net");
    let (_, after_audit) = native(
        &artifacts,
        "netlist-after",
        &cli,
        45,
        &[
            "sch",
            "export",
            "netlist",
            "--format",
            "kicadsexpr",
            "--output",
            after_netlist.to_str().unwrap(),
            path_string,
        ],
    )
    .await;
    std::fs::write(
        artifacts.join("text_native_after.audit.json"),
        serde_json::to_vec_pretty(&after_audit).unwrap(),
    )
    .unwrap();
    std::fs::copy(&path, artifacts.join("text_native_after.kicad_sch")).unwrap();
    assert_eq!(
        pin_partitions(&after_netlist),
        pin_partitions(&before_netlist)
    );
    println!(
        "native KiCad {}; exact text blocks and pin partitions retained",
        version.trim()
    );
}
