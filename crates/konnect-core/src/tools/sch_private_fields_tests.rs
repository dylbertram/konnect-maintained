use super::*;
use crate::mcp::handler::McpHandler;
use crate::tools::ServerConfig;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[path = "../../tests/fixtures/private_fields.rs"]
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
    // Keep the stem the native instance paths name.
    let path = directory.path().join("component_fields.kicad_sch");
    std::fs::write(&path, source).unwrap();
    (directory, path)
}

async fn call(handler: &McpHandler, name: &str, arguments: Value) -> Value {
    let response = handler
        .handle_message(json!({
            "jsonrpc":"2.0", "id":924, "method":"tools/call",
            "params":{"name":name, "arguments":arguments}
        }))
        .await
        .unwrap();
    assert!(response.error.is_none(), "{response:?}");
    let result = response.result.unwrap();
    assert_ne!(result["isError"], true, "{result}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn assert_properties(properties: &Value, reference: &str) {
    let (lcsc, mpn, description) = match reference {
        "R1" => ("C679001", "MPN-679-R1", "Resistor"),
        "U1" => (
            "C679002",
            "MPN-679-U1",
            "Low-Power, Dual Operational Amplifiers, DIP-8/SOIC-8/TO-99-8",
        ),
        other => panic!("unexpected reference {other}"),
    };
    assert_eq!(properties["LCSC"], lcsc);
    assert_eq!(properties["MPN"], mpn);
    assert_eq!(
        properties["private"], description,
        "quoted name is not a flag"
    );
}

#[tokio::test]
async fn served_private_properties_read_real_names_on_single_and_multiunit_parts() {
    let source = fixture::source(true);
    let (_directory, path) = project(&source);
    let handler = handler().await;
    for reference in ["R1", "U1"] {
        let read = call(
            &handler,
            "get_schematic_component",
            json!({"schematic":path, "reference":reference}),
        )
        .await;
        assert_properties(&read["properties"], reference);
        assert_eq!(read["unit_count"], if reference == "U1" { 3 } else { 1 });
    }
    let listed = call(
        &handler,
        "list_schematic_components",
        json!({"schematic":path}),
    )
    .await;
    assert_eq!(listed["count"], 4);
    for row in listed["components"].as_array().unwrap() {
        assert_properties(&row["properties"], row["reference"].as_str().unwrap());
    }
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        source,
        "reads wrote data"
    );
}

#[tokio::test]
async fn served_native_properties_without_private_flags_stay_ordinary() {
    let (_directory, path) = project(fixture::NATIVE);
    let handler = handler().await;
    let read = call(
        &handler,
        "get_schematic_component",
        json!({"schematic":path, "reference":"U1"}),
    )
    .await;
    assert_eq!(read["properties"]["MPN"], "MPN-679-U1");
    assert!(read["properties"].get("private").is_none());
    assert_eq!(std::fs::read_to_string(path).unwrap(), fixture::NATIVE);
}

#[tokio::test]
async fn served_harmless_edit_preserves_private_fields_and_every_unrelated_byte() {
    let source = fixture::source(true);
    let (_directory, path) = project(&source);
    let handler = handler().await;
    let blocks = find_all_symbol_instance_blocks(&source, "R1");
    assert_eq!(blocks.len(), 1);
    let (start, end) = blocks[0];
    let block = &source[start..end];
    let changed = block.replacen(
        "(property \"Value\" \"R\"",
        "(property \"Value\" \"harmless\"",
        1,
    );
    assert_ne!(changed, block);
    let expected = format!("{}{}{}", &source[..start], changed, &source[end..]);
    let result = call(
        &handler,
        "edit_schematic_component",
        json!({"schematic":path, "reference":"R1", "value":"harmless"}),
    )
    .await;
    assert_eq!(result["value"], "harmless");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    let read = call(
        &handler,
        "get_schematic_component",
        json!({"schematic":path, "reference":"R1"}),
    )
    .await;
    assert_properties(&read["properties"], "R1");
}

#[tokio::test]
async fn served_private_and_quoted_field_updates_preserve_headers_on_every_unit() {
    let source = fixture::source(true);
    let (_directory, path) = project(&source);
    let handler = handler().await;
    let result = call(
        &handler,
        "edit_schematic_component",
        json!({"schematic":path, "reference":"U1",
            "fields":{"MPN":"updated", "private":"ordinary updated"}}),
    )
    .await;
    assert_eq!(result["fields"]["MPN"], "updated");
    assert_eq!(result["fields"]["private"], "ordinary updated");
    let expected = source
        .replace("(property private \"MPN\" \"MPN-679-U1\"", "(property private \"MPN\" \"updated\"")
        .replace(
            "(property \"private\" \"Low-Power, Dual Operational Amplifiers, DIP-8/SOIC-8/TO-99-8\"",
            "(property \"private\" \"ordinary updated\"",
        );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    let committed = cse::Schematic::load(&path).unwrap();
    for symbol in committed
        .symbols
        .iter()
        .filter(|symbol| symbol.reference() == Some("U1"))
    {
        assert_eq!(symbol.property("MPN"), Some("updated"));
        assert!(
            symbol
                .properties
                .iter()
                .find(|field| field.name == "MPN")
                .unwrap()
                .private
        );
        assert!(
            !symbol
                .properties
                .iter()
                .find(|field| field.name == "private")
                .unwrap()
                .private
        );
    }
}

#[tokio::test]
async fn served_private_field_placement_changes_only_its_at_child() {
    let source = fixture::source(true);
    let (_directory, path) = project(&source);
    let handler = handler().await;
    let (start, end) = find_all_symbol_instance_blocks(&source, "R1")[0];
    let block = &source[start..end];
    let parsed = cse::sexp::parser::parse(block).unwrap();
    let field = parsed
        .find_all("property")
        .into_iter()
        .find(|field| field.args().get(1).and_then(|name| name.text()) == Some("MPN"))
        .unwrap();
    let at = cse::At::from_sexp(field.find("at").unwrap()).unwrap();
    let property_start = block.find("(property private \"MPN\"").unwrap();
    let old_at = format!(
        "(at {} {} {})",
        cse::types::fmt_f64(at.x),
        cse::types::fmt_f64(at.y),
        cse::types::fmt_f64(at.rotation.unwrap())
    );
    let new_at = format!(
        "(at 12.5 {} {})",
        cse::types::fmt_f64(at.y),
        cse::types::fmt_f64(at.rotation.unwrap())
    );
    let changed = block[property_start..].replacen(&old_at, &new_at, 1);
    assert_ne!(changed, block[property_start..]);
    let expected = format!(
        "{}{}{}{}",
        &source[..start],
        &block[..property_start],
        changed,
        &source[end..]
    );
    let result = call(
        &handler,
        "edit_schematic_component",
        json!({"schematic":path, "reference":"R1", "field_placements":{"MPN":{"x":12.5}}}),
    )
    .await;
    assert_eq!(result["units"][0]["field_placements"]["MPN"]["x"], 12.5);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
}

fn assert_untouched_properties(before: &str, after: &str, reference: &str) {
    let before = cse::sexp::parser::parse(before).unwrap();
    let after = cse::sexp::parser::parse(after).unwrap();
    for symbol in before.find_all("symbol") {
        if cse::Symbol::from_sexp(symbol).unwrap().reference() != Some(reference) {
            continue;
        }
        let uuid = symbol.get_value("uuid").unwrap();
        let written = after
            .find_all("symbol")
            .into_iter()
            .find(|symbol| symbol.get_value("uuid") == Some(uuid))
            .unwrap();
        assert_eq!(written.find_all("property"), symbol.find_all("property"));
    }
    assert_eq!(after.find("lib_symbols"), before.find("lib_symbols"));
}

#[tokio::test]
async fn served_whole_file_move_keeps_unselected_private_values_and_accepts_two_flags() {
    for reference in ["R1", "U1"] {
        let source = fixture::source(true);
        let (_directory, path) = project(&source);
        let handler = handler().await;
        let result = call(
            &handler,
            "move_schematic_component",
            json!({"schematic":path, "reference":reference, "x":50.8, "y":50.8}),
        )
        .await;
        assert_eq!(result["moved_units"], if reference == "U1" { 3 } else { 1 });
        let written = std::fs::read_to_string(&path).unwrap();
        assert_untouched_properties(
            &source,
            &written,
            if reference == "R1" { "U1" } else { "R1" },
        );
        let read = call(
            &handler,
            "get_schematic_component",
            json!({"schematic":path, "reference":reference}),
        )
        .await;
        assert_properties(&read["properties"], reference);
        let committed = cse::Schematic::load(&path).unwrap();
        for symbol in committed.symbols.iter() {
            assert_eq!(
                symbol
                    .properties
                    .iter()
                    .filter(|field| field.private)
                    .count(),
                2
            );
        }
    }
}

#[tokio::test]
async fn served_duplicate_real_name_still_refuses_without_writing() {
    let source = fixture::source(true);
    // Two differently named private fields are valid. Two copies of the SAME
    // real field are ambiguous even when only one copy has a private flag.
    let source = source.replacen(
        "(property private \"LCSC\" \"C679001\"",
        "(property \"MPN\" \"duplicate\"",
        1,
    );
    let (_directory, path) = project(&source);
    let handler = handler().await;
    let response = handler
        .handle_message(json!({
            "jsonrpc":"2.0", "id":924, "method":"tools/call",
            "params":{"name":"move_schematic_component", "arguments":{
                "schematic":path, "reference":"R1", "x":50.8, "y":50.8
            }}
        }))
        .await
        .unwrap()
        .result
        .unwrap();
    assert_eq!(response["isError"], true, "{response}");
    assert!(
        response.to_string().contains("ambiguous_target"),
        "{response}"
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

/// Reuse the export gate's tested Linux subreaper, rather than placing a
/// timeout around a blocking CLI call or duplicating process-group supervision.
fn private_native_supervisor(directory: &Path, phase: &str, cli: &str, seconds: u32) -> PathBuf {
    assert!(
        cfg!(target_os = "linux"),
        "native private-field supervision requires Linux and Python 3.9+"
    );
    let wrapper = directory.join(format!("private-native-{phase}.py"));
    std::fs::write(
        &wrapper,
        include_str!("../../tests/support/pcb_export_supervisor.py"),
    )
    .unwrap();
    std::fs::write(
        wrapper.with_extension("json"),
        serde_json::to_vec(&json!({
            "program": cli,
            "seconds": seconds,
            "audit": wrapper.with_extension("audit"),
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    wrapper
}

fn private_native_audit(wrapper: &Path) -> Value {
    let audits = std::fs::read_dir(wrapper.with_extension("audit"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("finished.json"))
        .collect::<Vec<_>>();
    assert_eq!(
        audits.len(),
        1,
        "one supervised invocation per native phase"
    );
    let audit: Value = serde_json::from_slice(&std::fs::read(&audits[0]).unwrap()).unwrap();
    assert_eq!(audit["group_reaped"], true, "{audit}");
    assert_eq!(audit["timed_out"], false, "{audit}");
    assert_eq!(audit["interrupted"], 0, "{audit}");
    audit
}

#[tokio::test]
#[ignore = "requires coordinator-admitted native KiCad CLI, never the live editor"]
async fn served_private_fields_native_resave_and_readback() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").expect("KONNECT_TEST_KICAD_CLI is required");
    let (directory, path) = project(&fixture::source(false));
    let version_wrapper = private_native_supervisor(directory.path(), "version", &cli, 10);
    // The wrapper owns an independent deadline and kills/reaps its native
    // process group. Dropping this future must not kill that cleanup owner.
    let version = tokio::process::Command::new(&version_wrapper)
        .arg("--version")
        .kill_on_drop(false)
        .output()
        .await
        .unwrap();
    let version_audit = private_native_audit(&version_wrapper);
    assert!(version.status.success(), "{version:?}");
    let upgrade_wrapper = private_native_supervisor(directory.path(), "upgrade", &cli, 45);
    let upgraded = tokio::process::Command::new(&upgrade_wrapper)
        .args(["sch", "upgrade", "--force"])
        .arg(&path)
        .kill_on_drop(false)
        .output()
        .await
        .unwrap();
    let upgrade_audit = private_native_audit(&upgrade_wrapper);
    assert!(upgraded.status.success(), "{upgraded:?}");
    let native = std::fs::read_to_string(&path).unwrap();
    assert!(native.contains("(property private \"MPN\""));
    // Optional native-output capture for the coordinator's artifact retrieval.
    // Copy KiCad's bytes, never synthesize or reserialize the captured CAD file.
    if let Some(directory) = std::env::var_os("KONNECT_TEST_NATIVE_ARTIFACT_DIR") {
        use sha2::Digest;
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(&path, directory.join("private_fields_native.kicad_sch")).unwrap();
        let provenance = json!({
            "base_fixture":"component_fields_kicad10.kicad_sch",
            "base_sha256":"80d57b769749c8b9828be52a09ad947e7a0aa55715e404805f80b70d688d9f54",
            "preparation":"synthetic private flags and quoted private field name; no unknown child",
            "native_version":String::from_utf8_lossy(&version.stdout).trim(),
            "command":["sch", "upgrade", "--force", "component_fields.kicad_sch"],
            "native_sha256":format!("{:x}", sha2::Sha256::digest(native.as_bytes())),
            "supervision":{
                "version_seconds":10,
                "upgrade_seconds":45,
                "version":version_audit,
                "upgrade":upgrade_audit
            }
        });
        std::fs::write(
            directory.join("private_fields_native.provenance.json"),
            serde_json::to_vec_pretty(&provenance).unwrap(),
        )
        .unwrap();
    }
    println!("Native private fixture: base component_fields_kicad10.kicad_sch; synthetic flags/quoted name; kicad-cli {} sch upgrade --force {:?}",
        String::from_utf8_lossy(&version.stdout).trim(), path);
    let handler = handler().await;
    call(
        &handler,
        "move_schematic_component",
        json!({"schematic":path, "reference":"U1", "x":50.8, "y":50.8}),
    )
    .await;
    let written = std::fs::read_to_string(&path).unwrap();
    assert_untouched_properties(&native, &written, "R1");
    for reference in ["R1", "U1"] {
        let read = call(
            &handler,
            "get_schematic_component",
            json!({"schematic":path, "reference":reference}),
        )
        .await;
        assert_properties(&read["properties"], reference);
    }
}
