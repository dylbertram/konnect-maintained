//! Served non-noop edits and an explicitly admitted, ignored native CLI gate.
//! No test opens or communicates with a live CAD editor.

use konnect_core::mcp::handler::McpHandler;
use konnect_core::tools::ServerConfig;
use konnect_sexp::parser::parse_sexp;
use konnect_sexp::schematic::extract_wires;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// Some assertions are used only by the model tests sharing this source oracle.
#[allow(dead_code)]
#[path = "../../konnect-schematic-editor/tests/support/symbol_serialization.rs"]
mod fixture;

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
    let handler = McpHandler::new(ServerConfig {
        kicad_cli: String::new(),
        kicad_binary: String::new(),
        ipc_address: String::new(),
        project_dir: None,
        jlcpcb_db_path: None,
        auto_load_toolsets: true,
        eager_toolsets: false,
    })
    .await
    .unwrap();
    let response = handler
        .handle_message(json!({
            "jsonrpc":"2.0", "id":692, "method":"tools/call",
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
    let committed = extract_wires(&parse_sexp(&written).unwrap());
    assert_eq!(committed.len(), wires.len() + 1);
    let added: Vec<_> = committed
        .iter()
        .filter(|wire| matches_requested(wire))
        .collect();
    assert_eq!(added.len(), 1, "requested wire committed exactly once");
    let uuid = added[0].uuid.as_deref().expect("new wire UUID");
    assert!(!wires.iter().any(|wire| wire.uuid.as_deref() == Some(uuid)));
    written
}

#[tokio::test]
async fn served_nonnoop_add_wire_preserves_all_25_native_symbol_blocks() {
    fixture::assert_native(fixture::NATIVE);
    let (_directory, path) = project(fixture::NATIVE);
    let written = add_wire(&path).await;
    fixture::assert_exact(fixture::NATIVE, &written);
    fixture::assert_attributes(fixture::NATIVE, &written);
    fixture::assert_native(&written);
}

#[tokio::test]
async fn served_nonnoop_add_wire_preserves_flag_syntax() {
    for (_, clause, _) in fixture::FLAG_CASES {
        let source = fixture::with_flag(*clause);
        let (_directory, path) = project(&source);
        let written = add_wire(&path).await;
        fixture::assert_attributes(&source, &written);
        fixture::assert_others_exact(&source, &written);
    }
}

#[tokio::test]
async fn served_nonnoop_add_wire_preserves_raw_attributes_and_unknown_children() {
    for (body, positions) in fixture::RAW_CASES {
        let source = fixture::with_raw_attributes(fixture::NATIVE, body, positions);
        let source = fixture::with_unknown(&source);
        let (_directory, path) = project(&source);
        let written = add_wire(&path).await;
        fixture::assert_attributes(&source, &written);
        fixture::assert_unknown(&source, &written);
        fixture::assert_others_exact(&source, &written);
    }
}

/// Reuse the existing tested Linux native supervisor unchanged. The supervisor
/// and its config/audits live outside TempDir so cancellation cannot remove its
/// cleanup instructions. It owns deadlines, group termination and child reaping.
async fn native(
    artifacts: &Path,
    phase: &str,
    cli: &str,
    seconds: u32,
    arguments: &[&str],
) -> (std::process::Output, Value) {
    if !cfg!(target_os = "linux") {
        panic!("native gate requires Linux");
    }
    assert!(Path::new(cli).is_absolute(), "explicit absolute native CLI");
    let wrapper = artifacts.join(format!("symbol-native-{phase}.py"));
    std::fs::write(&wrapper, include_str!("support/pcb_export_supervisor.py")).unwrap();
    std::fs::write(
        wrapper.with_extension("json"),
        serde_json::to_vec(&json!({
            "program":cli, "seconds":seconds, "audit":wrapper.with_extension("audit")
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        artifacts.join(format!("{phase}.command.json")),
        serde_json::to_vec_pretty(&json!({
            "program":cli, "arguments":arguments, "deadline_seconds":seconds,
            "supervisor_sha256":format!(
                "{:x}", Sha256::digest(include_bytes!("support/pcb_export_supervisor.py"))
            )
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
    std::fs::write(
        artifacts.join(format!("{phase}.stdout.log")),
        &output.stdout,
    )
    .unwrap();
    std::fs::write(
        artifacts.join(format!("{phase}.stderr.log")),
        &output.stderr,
    )
    .unwrap();
    let audits: Vec<_> = std::fs::read_dir(wrapper.with_extension("audit"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("finished.json"))
        .collect();
    assert_eq!(audits.len(), 1, "one invocation for {phase}");
    let audit: Value = serde_json::from_slice(&std::fs::read(&audits[0]).unwrap()).unwrap();
    assert_eq!(audit["group_reaped"], true, "{phase}: {audit}");
    assert_eq!(audit["timed_out"], false, "{phase}: {audit}");
    assert_eq!(audit["interrupted"], 0, "{phase}: {audit}");
    assert_eq!(audit["status"], 0, "{phase}: {audit}");
    assert!(output.status.success(), "{phase}: {output:?}");
    (output, audit)
}

/// Connectivity expectations come only from native exported netlists, never
/// the internal wire graph. Include singleton partitions and require nonempty
/// results so missing exports cannot produce a vacuous equality.
fn pin_partitions(path: &Path) -> BTreeSet<BTreeSet<(String, String)>> {
    let source = std::fs::read_to_string(path).unwrap();
    let root = parse_sexp(&source).unwrap();
    let mut partitions = BTreeSet::new();
    for net in root.find("nets").expect("native nets").find_all("net") {
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
    assert!(!partitions.is_empty(), "native pin oracle cannot be empty");
    partitions
}

#[tokio::test]
#[ignore = "coordinator-admitted Linux native CLI only; never the live editor"]
async fn native_resave_and_served_edit_preserve_symbols_and_native_pin_partitions() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").expect("explicit native CLI");
    let artifacts = PathBuf::from(
        std::env::var_os("KONNECT_TEST_NATIVE_ARTIFACT_DIR").expect("retained artifacts"),
    );
    assert!(artifacts.is_absolute(), "absolute artifact directory");
    std::fs::create_dir_all(&artifacts).unwrap();
    assert!(
        std::fs::read_dir(&artifacts).unwrap().next().is_none(),
        "native artifact directory must be empty"
    );
    fixture::assert_native(fixture::NATIVE);
    let (_directory, path) = project(fixture::NATIVE);
    let path_string = path.to_str().unwrap();
    let (version, version_audit) = native(&artifacts, "version", &cli, 10, &["--version"]).await;
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(!version.trim().is_empty());
    let (_, before_upgrade) = native(
        &artifacts,
        "upgrade-before",
        &cli,
        45,
        &["sch", "upgrade", "--force", path_string],
    )
    .await;
    let captured = std::fs::read_to_string(&path).unwrap();
    fixture::assert_native(&captured);
    std::fs::copy(&path, artifacts.join("symbol_native_before.kicad_sch")).unwrap();
    let before_netlist = artifacts.join("symbol_before.net");
    let (_, before_export) = native(
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
    let before_partitions = pin_partitions(&before_netlist);
    let before_provenance = json!({
        "native_version":version.trim(),
        "base_sha256":format!("{:x}", Sha256::digest(fixture::NATIVE.as_bytes())),
        "captured_sha256":format!("{:x}", Sha256::digest(captured.as_bytes())),
        "symbol_uuids":fixture::UUIDS,
        "scope":"native valid fixture only, 25 direct placed symbols; no synthetic syntax",
        "deadlines_seconds":{"version":10, "upgrade":45, "netlist":45},
        "supervision":{
            "version":version_audit,
            "upgrade_before":before_upgrade,
            "netlist_before":before_export
        }
    });
    std::fs::write(
        artifacts.join("symbol_native_before.provenance.json"),
        serde_json::to_vec_pretty(&before_provenance).unwrap(),
    )
    .unwrap();
    let written = add_wire(&path).await;
    fixture::assert_exact(&captured, &written);
    fixture::assert_attributes(&captured, &written);
    std::fs::copy(&path, artifacts.join("symbol_served_after.kicad_sch")).unwrap();
    let after_netlist = artifacts.join("symbol_after.net");
    let (_, after_export) = native(
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
    assert_eq!(pin_partitions(&after_netlist), before_partitions);
    let (_, after_upgrade) = native(
        &artifacts,
        "upgrade-after",
        &cli,
        45,
        &["sch", "upgrade", "--force", path_string],
    )
    .await;
    let resaved = std::fs::read_to_string(&path).unwrap();
    fixture::assert_exact(&captured, &resaved);
    std::fs::copy(&path, artifacts.join("symbol_native_after.kicad_sch")).unwrap();
    std::fs::write(
        artifacts.join("symbol_native_after.provenance.json"),
        serde_json::to_vec_pretty(&json!({
            "served_sha256":format!("{:x}", Sha256::digest(written.as_bytes())),
            "resaved_sha256":format!("{:x}", Sha256::digest(resaved.as_bytes())),
            "supervision":{"netlist_after":after_export, "upgrade_after":after_upgrade},
            "pin_partition_count":before_partitions.len(),
            "exact_symbol_count":25
        }))
        .unwrap(),
    )
    .unwrap();
}
