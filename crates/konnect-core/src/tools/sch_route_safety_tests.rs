//! Generated test designs use the supported schematic editor API and the
//! Device:R definition from an existing KiCad-written fixture. They are not
//! claimed to be native-resaved designs. No live editor or library is needed.

use crate::mcp::handler::McpHandler;
use crate::tools::ServerConfig;
use konnect_schematic_editor as cse;
use konnect_sexp::schematic::{extract_junctions, extract_wires};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const NATIVE_SOURCE: &str = include_str!("../../tests/fixtures/junction_reconcile.kicad_sch");

fn blank(path: &Path) -> cse::Schematic {
    let mut sch = cse::Schematic::from_source(path, NATIVE_SOURCE).unwrap();
    sch.symbols = cse::SymbolCollection::new(vec![]);
    sch.wires = cse::WireCollection::new(vec![]);
    sch.labels = cse::LabelCollection::new(vec![]);
    sch.global_labels = cse::GlobalLabelCollection::new(vec![]);
    sch.hierarchical_labels = cse::HierarchicalLabelCollection::new(vec![]);
    sch.junctions.clear();
    sch.no_connects.clear();
    sch.texts.clear();
    sch.sheets = cse::SheetCollection::new(vec![]);
    sch.raw_other
        .retain(|node| matches!(node.tag(), Some("lib_symbols" | "sheet_instances")));
    sch.generator = Some("konnect-route-tests".into());
    sch.generator_version = None;
    sch
}

fn save_generated(sch: &cse::Schematic, path: &Path) {
    let staging = path.with_extension("generated");
    sch.save(&staging).unwrap();
    std::fs::rename(staging, path).unwrap();
}

fn resistor(sch: &mut cse::Schematic, reference: &str, x: f64, y: f64) {
    let template = cse::Schematic::from_source("unused.kicad_sch", NATIVE_SOURCE).unwrap();
    let native = template.symbols.iter().next().unwrap();
    let mut part = cse::Symbol::new("Device:R", x, y);
    // Carry native property formatting, but not another symbol's pin UUIDs.
    part.properties = native.properties.clone();
    part.at = native.at.clone();
    part.move_to(x, y);
    part.set_rotation(0.0);
    part.set_reference(reference);
    part.set_value_str("R");
    part.set_instance_path(
        "route",
        &format!("/{}", sch.uuid.as_ref().unwrap()),
        reference,
        1,
    );
    sch.add_symbol(part);
}

fn design(case: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("route.kicad_sch");
    let mut sch = blank(&path);
    resistor(&mut sch, "R1", 101.6, 101.6);
    resistor(&mut sch, "R2", 127.0, 111.76);
    match case {
        "around" => resistor(&mut sch, "R3", 114.3, 109.22),
        "blocked" => {
            resistor(&mut sch, "R3", 114.3, 109.22);
            resistor(&mut sch, "R4", 120.65, 111.76);
        }
        "label" => {
            // A label alone on each bend, with no pin terminal to mask a
            // missing label guard.
            sch.add_label("OTHER", 114.3, 105.41);
            sch.add_label("OTHER", 120.65, 107.95);
        }
        "sheet" => {
            let child_path = dir.path().join("child.kicad_sch");
            let mut child = blank(&child_path);
            child.uuid = Some(konnect_sexp::writer::new_uuid());
            resistor(&mut child, "R61", 50.8, 50.8);
            child.add_hierarchical_label("IN", "input", 50.8, 46.99);
            let mut sheet = cse::Sheet::new("SUB", "child.kicad_sch", 127.0, 96.52, 19.05, 10.16);
            sheet
                .pins
                .push(cse::SheetPin::new("IN", "input", 127.0, 105.41));
            sheet.pins[0].at.rotation = Some(180.0);
            let root = format!("/{}/{}", sch.uuid.as_ref().unwrap(), sheet.uuid);
            child
                .symbols
                .by_reference_mut("R61")
                .unwrap()
                .raw_sub_nodes
                .clear();
            child
                .symbols
                .by_reference_mut("R61")
                .unwrap()
                .set_instance_path("route", &root, "R61", 1);
            sheet.instances.push(cse::SheetInstance {
                project_name: "route".into(),
                path: format!("/{}", sch.uuid.as_ref().unwrap()),
                page: "2".into(),
            });
            save_generated(&child, &child_path);
            sch.add_sheet(sheet);
        }
        "tee" | "tap" => {
            resistor(&mut sch, "R51", 177.8, 152.4);
            resistor(&mut sch, "R52", 203.2, 162.56);
            resistor(&mut sch, "R53", 190.5, 139.7);
            sch.add_wire(177.8, 156.21, 203.2, 156.21);
            sch.add_wire(203.2, 156.21, 203.2, 158.75);
            // Deliberate undotted T, created through the model API.
            sch.add_wire(190.5, 143.51, 190.5, 156.21);
            if case == "tap" {
                resistor(&mut sch, "R54", 184.15, 139.7);
            }
        }
        "clear" => {}
        _ => panic!("unknown case {case}"),
    }
    // from_source's path is the output identity. save elsewhere uses atomic
    // creation rather than trying to revision-check a file that doesn't exist.
    save_generated(&sch, &path);
    (dir, path)
}

async fn call(name: &str, path: &Path, mut args: Value) -> (bool, Value) {
    args["schematic"] = json!(path);
    let handler = McpHandler::new(ServerConfig {
        auto_load_toolsets: true,
        ..ServerConfig::default()
    })
    .await
    .unwrap();
    let response = handler
        .handle_message(json!({
            "jsonrpc": "2.0", "id": 887, "method": "tools/call",
            "params": {"name": name, "arguments": args}
        }))
        .await
        .unwrap()
        .result
        .unwrap();
    let body = serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
    (response["isError"] == true, body)
}

fn pair() -> Value {
    json!({"ref1": "R1", "pin1": "2", "ref2": "R2", "pin2": "1"})
}

fn points() -> Value {
    json!({"x1": 101.6, "y1": 105.41, "x2": 127.0, "y2": 107.95})
}

fn tree(path: &Path) -> konnect_sexp::SexpNode {
    konnect_sexp::parse_sexp(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn same_net(path: &Path, a: (f64, f64), b: (f64, f64)) -> bool {
    let tree = tree(path);
    let labels = konnect_sexp::schematic::extract_all_net_labels(&tree);
    let mut graph = super::sch_connectivity::net_graph_for(&tree, &extract_wires(&tree), &labels);
    graph.root_at(a.0, a.1) == graph.root_at(b.0, b.1)
}

#[tokio::test]
async fn route_safety_served_alternate_bend_avoids_third_pin_and_sheet_pin() {
    for case in ["around", "sheet"] {
        for name in [
            "connect_pins",
            "add_schematic_connection",
            "batch_connect_pins",
        ] {
            let (_dir, path) = design(case);
            let args = match name {
                "connect_pins" => pair(),
                "batch_connect_pins" => json!({"connections": [pair()]}),
                _ => points(),
            };
            let (error, body) = call(name, &path, args).await;
            assert!(!error, "{name}/{case}: {body}");
            let path_value = if name == "batch_connect_pins" {
                &body["connected"][0]["path"]
            } else {
                &body["path"]
            };
            assert_eq!(
                path_value,
                &json!([[101.6, 105.41], [101.6, 107.95], [127.0, 107.95]])
            );
            assert!(same_net(&path, (101.6, 105.41), (127.0, 107.95)));
            let foreign = if case == "around" {
                (114.3, 105.41)
            } else {
                (127.0, 105.41)
            };
            assert!(!same_net(&path, (101.6, 105.41), foreign));
            assert!(extract_junctions(&tree(&path)).is_empty());
        }
    }
}

#[tokio::test]
async fn route_safety_served_blocked_pin_and_standalone_label_write_nothing() {
    for case in ["blocked", "label"] {
        for name in [
            "connect_pins",
            "add_schematic_connection",
            "batch_connect_pins",
        ] {
            let (_dir, path) = design(case);
            let before = std::fs::read(&path).unwrap();
            let args = match name {
                "connect_pins" => pair(),
                "batch_connect_pins" => json!({"connections": [pair()]}),
                _ => points(),
            };
            let (error, body) = call(name, &path, args).await;
            assert!(error, "{name}/{case}: {body}");
            if name == "batch_connect_pins" {
                assert_eq!(body["connected_count"], 0);
                assert_eq!(body["skipped"][0]["kind"], "plan_blocked");
                assert_eq!(body["skipped"][0]["reasons"].as_array().unwrap().len(), 2);
            } else {
                assert_eq!(body["error"]["kind"], "plan_blocked");
                assert_eq!(body["error"]["operation"], name);
                assert_eq!(body["error"]["reasons"].as_array().unwrap().len(), 2);
            }
            assert_eq!(std::fs::read(&path).unwrap(), before);
            if case == "label" {
                assert!(body.to_string().contains("label OTHER"));
            }
        }
    }
}

#[tokio::test]
async fn route_safety_served_clear_route_and_remote_tee_are_preserved() {
    for name in [
        "connect_pins",
        "add_schematic_connection",
        "batch_connect_pins",
    ] {
        let (_dir, path) = design("tee");
        let original = std::fs::read_to_string(&path).unwrap();
        let before = extract_wires(&tree(&path));
        let args = match name {
            "connect_pins" => pair(),
            "batch_connect_pins" => json!({"connections": [pair()]}),
            _ => points(),
        };
        let (error, body) = call(name, &path, args).await;
        assert!(!error, "{name}: {body}");
        assert!(extract_junctions(&tree(&path)).is_empty());
        assert!(!same_net(&path, (177.8, 156.21), (190.5, 143.51)));
        let after = extract_wires(&tree(&path));
        assert_eq!(after.len(), before.len() + 2);
        for wire in before {
            assert!(after.iter().any(|candidate| candidate.uuid == wire.uuid));
        }
        let saved = std::fs::read_to_string(&path).unwrap();
        for (start, end) in konnect_sexp::writer::find_direct_child_blocks(&original, "kicad_sch") {
            assert!(
                saved.contains(&original[start..end]),
                "an unrelated source block changed"
            );
        }
        let chosen = if name == "batch_connect_pins" {
            &body["connected"][0]["path"]
        } else {
            &body["path"]
        };
        assert_eq!(
            chosen,
            &json!([[101.6, 105.41], [127.0, 105.41], [127.0, 107.95]])
        );
    }
}

#[tokio::test]
async fn route_safety_served_midspan_tap_keeps_unrelated_tee_undotted() {
    let (_dir, path) = design("tap");
    let (error, body) = call(
        "add_schematic_connection",
        &path,
        json!({"x1": 184.15, "y1": 143.51, "x2": 184.15, "y2": 156.21}),
    )
    .await;
    assert!(!error, "{body}");
    assert_eq!(extract_junctions(&tree(&path)), vec![(184.15, 156.21)]);
    assert!(same_net(&path, (184.15, 143.51), (177.8, 156.21)));
    assert!(!same_net(&path, (190.5, 143.51), (177.8, 156.21)));
}

#[tokio::test]
async fn route_safety_served_straight_route_refuses_a_third_pin() {
    let (_dir, path) = design("around");
    let before = std::fs::read(&path).unwrap();
    let (error, body) = call(
        "add_schematic_connection",
        &path,
        json!({"x1": 101.6, "y1": 105.41, "x2": 127.0, "y2": 105.41}),
    )
    .await;
    assert!(error, "{body}");
    assert_eq!(body["error"]["kind"], "plan_blocked");
    assert_eq!(body["error"]["reasons"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn route_safety_served_preexisting_same_net_pin_is_allowed() {
    let (_dir, path) = design("around");
    let mut sch = cse::Schematic::load(&path).unwrap();
    sch.add_label("ALREADY", 101.6, 105.41);
    sch.add_label("ALREADY", 114.3, 105.41);
    sch.overwrite().unwrap();
    let (error, body) = call("connect_pins", &path, pair()).await;
    assert!(!error, "{body}");
    assert_eq!(
        body["path"],
        json!([[101.6, 105.41], [127.0, 105.41], [127.0, 107.95]])
    );
    assert_eq!(extract_junctions(&tree(&path)), vec![(114.3, 105.41)]);
}

#[tokio::test]
async fn route_safety_served_crossing_without_junction_remains_separate() {
    let (_dir, path) = design("clear");
    let mut sch = cse::Schematic::load(&path).unwrap();
    sch.add_wire(114.3, 99.06, 114.3, 111.76);
    sch.add_label("CROSSING", 114.3, 99.06);
    sch.overwrite().unwrap();
    let (error, body) = call("connect_pins", &path, pair()).await;
    assert!(!error, "{body}");
    assert_eq!(
        body["path"],
        json!([[101.6, 105.41], [127.0, 105.41], [127.0, 107.95]])
    );
    assert!(extract_junctions(&tree(&path)).is_empty());
    assert!(!same_net(&path, (101.6, 105.41), (114.3, 99.06)));
}

#[tokio::test]
async fn route_safety_served_batch_saves_only_safe_pairs_and_reports_retry_indexes() {
    let (_dir, path) = design("blocked");
    let (error, body) = call(
        "batch_connect_pins",
        &path,
        json!({"connections": [
            pair(),
        {"ref1": "R3", "pin1": "2", "ref2": "R4", "pin2": "2"},
            {"ref1": "MISSING", "pin1": "1", "ref2": "R2", "pin2": "1"}
        ]}),
    )
    .await;
    assert!(!error, "{body}");
    assert_eq!(body["connected_count"], 1);
    assert_eq!(body["connected"][0]["index"], 1);
    assert_eq!(body["errors"].as_array().unwrap().len(), 2);
    let skipped: BTreeSet<_> = body["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["index"].as_u64().unwrap())
        .collect();
    assert_eq!(skipped, BTreeSet::from([0, 2]));
    assert_eq!(extract_wires(&tree(&path)).len(), 2);
    assert!(same_net(&path, (114.3, 113.03), (120.65, 115.57)));
    assert!(!same_net(&path, (101.6, 105.41), (127.0, 107.95)));
}

#[tokio::test]
async fn route_safety_served_batch_checks_each_pair_against_prior_staged_routes() {
    let (_dir, path) = design("clear");
    let mut sch = cse::Schematic::load(&path).unwrap();
    resistor(&mut sch, "R3", 152.4, 109.22);
    resistor(&mut sch, "R4", 114.3, 102.87);
    sch.overwrite().unwrap();
    // R3.1 -> R4.2 is safe horizontal-first on the original sheet. After the
    // first pair is staged, that bend overlaps its net and must be rerouted.
    let (error, body) = call(
        "batch_connect_pins",
        &path,
        json!({"connections": [
            pair(),
            {"ref1": "R3", "pin1": "1", "ref2": "R4", "pin2": "2"}
        ]}),
    )
    .await;
    assert!(!error, "{body}");
    assert_eq!(body["connected_count"], 2);
    assert!(body["errors"].as_array().unwrap().is_empty());
    assert_eq!(
        body["connected"][1]["path"],
        json!([[152.4, 105.41], [152.4, 106.68], [114.3, 106.68]])
    );
    assert!(same_net(&path, (152.4, 105.41), (114.3, 106.68)));
    assert!(!same_net(&path, (101.6, 105.41), (152.4, 105.41)));
    assert!(extract_junctions(&tree(&path)).is_empty());
}

#[tokio::test]
async fn route_safety_served_missing_ambiguous_and_unknown_pin_geometry_fail_closed() {
    for condition in ["missing", "duplicate", "unresolved", "empty"] {
        for name in [
            "connect_pins",
            "batch_connect_pins",
            "add_schematic_connection",
        ] {
            if condition == "missing" && name == "add_schematic_connection" {
                continue;
            }
            if condition == "duplicate" && name == "add_schematic_connection" {
                continue;
            }
            let (_dir, path) = design("clear");
            let mut sch = cse::Schematic::load(&path).unwrap();
            match condition {
                "duplicate" => resistor(&mut sch, "R1", 150.0, 150.0),
                "unresolved" => {
                    sch.symbols.by_reference_mut("R2").unwrap().lib_id = "Missing:X".into()
                }
                "empty" => sch.symbols.by_reference_mut("R2").unwrap().unit = 99,
                _ => {}
            }
            sch.overwrite().unwrap();
            let before = std::fs::read(&path).unwrap();
            let mut request = pair();
            if condition == "missing" {
                request["pin1"] = json!("99");
            }
            let args = match name {
                "connect_pins" => request,
                "batch_connect_pins" => json!({"connections": [request]}),
                _ => points(),
            };
            let (error, body) = call(name, &path, args).await;
            assert!(error, "{condition}/{name}: {body}");
            assert!(body.to_string().contains("plan_blocked"), "{body}");
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
}

// Deliberately ignored: the coordinator must admit a native KiCad environment.
// Never use the internal graph as the oracle for this acceptance.
fn native_nets(cli: &str, path: &Path, phase: &str) -> BTreeMap<String, BTreeSet<String>> {
    let output = path.with_extension(format!("{phase}.net"));
    let result = std::process::Command::new(cli)
        .args([
            "sch",
            "export",
            "netlist",
            "--format",
            "kicadsexpr",
            "--output",
        ])
        .arg(&output)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read_to_string(output).unwrap();
    let tree = konnect_sexp::parse_sexp(&source).unwrap();
    tree.find("nets")
        .expect("native netlist nets")
        .find_all("net")
        .into_iter()
        .map(|net| {
            let name = net.find_str("name").unwrap().to_string();
            let pins = net
                .find_all("node")
                .into_iter()
                .map(|node| {
                    format!(
                        "{}.{}",
                        node.find_str("ref").unwrap(),
                        node.find_str("pin").unwrap()
                    )
                })
                .collect();
            (name, pins)
        })
        .collect()
}

fn native_group(nets: &BTreeMap<String, BTreeSet<String>>, pin: &str) -> BTreeSet<String> {
    nets.values()
        .find(|pins| pins.contains(pin))
        .unwrap_or_else(|| panic!("native pin {pin} missing: {nets:?}"))
        .clone()
}

#[tokio::test]
#[ignore = "coordinator-only native KiCad CLI acceptance, set KONNECT_ROUTE_KICAD_CLI"]
async fn route_safety_native_before_after_netlist_oracle() {
    let cli = std::env::var("KONNECT_ROUTE_KICAD_CLI").expect("explicit native CLI required");
    for name in [
        "connect_pins",
        "batch_connect_pins",
        "add_schematic_connection",
    ] {
        for case in ["around", "blocked", "label", "sheet", "tee", "tap"] {
            if case == "tap" && name != "add_schematic_connection" {
                continue;
            }
            let (_dir, path) = design(case);
            let before = native_nets(&cli, &path, "before");
            assert_eq!(
                native_group(&before, "R1.2"),
                BTreeSet::from(["R1.2".into()])
            );
            let args = if case == "tap" {
                json!({"x1": 184.15, "y1": 143.51, "x2": 184.15, "y2": 156.21})
            } else {
                match name {
                    "connect_pins" => pair(),
                    "batch_connect_pins" => json!({"connections": [pair()]}),
                    _ => points(),
                }
            };
            let (error, body) = call(name, &path, args).await;
            let after = native_nets(&cli, &path, "after");
            if ["blocked", "label"].contains(&case) {
                assert!(error, "{case}: {body}");
                assert_eq!(before, after);
            } else {
                assert!(!error, "{case}: {body}");
                if case != "tap" {
                    assert_eq!(
                        native_group(&after, "R1.2"),
                        BTreeSet::from(["R1.2".into(), "R2.1".into()])
                    );
                }
                if ["tee", "tap"].contains(&case) {
                    assert_eq!(
                        native_group(&before, "R51.2"),
                        BTreeSet::from(["R51.2".into(), "R52.1".into()])
                    );
                    let mut expected = BTreeSet::from(["R51.2".into(), "R52.1".into()]);
                    if case == "tap" {
                        expected.insert("R54.2".into());
                    }
                    assert_eq!(native_group(&after, "R51.2"), expected);
                    assert_eq!(
                        native_group(&before, "R53.2"),
                        native_group(&after, "R53.2")
                    );
                }
                if case == "sheet" {
                    assert_eq!(
                        native_group(&before, "R61.1"),
                        native_group(&after, "R61.1")
                    );
                }
            }
        }
    }
}
