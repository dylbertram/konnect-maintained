//! Served read responses against native fixtures. No CLI, editor or IPC is needed.
use konnect_core::{mcp::handler::McpHandler, tools::ServerConfig};
use konnect_sexp::{parse_sexp, SexpNode};
use serde_json::{json, Value};
use std::path::PathBuf;

const MIRRORS: &str = include_str!("fixtures/mirror_fields_kicad10.kicad_sch");
const MULTI_UNIT: &str = include_str!("fixtures/ecc83_multiunit.kicad_sch");
const LABELS: &str = include_str!("fixtures/two_name_nets.kicad_sch");

fn fixture(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sheet.kicad_sch");
    std::fs::write(&path, source).unwrap();
    (dir, path)
}

async fn handler() -> McpHandler {
    McpHandler::new(ServerConfig {
        kicad_cli: String::new(),
        kicad_binary: String::new(),
        ipc_address: String::new(),
        eager_toolsets: true,
        ..Default::default()
    })
    .await
    .unwrap()
}

async fn call(handler: &McpHandler, name: &str, args: Value) -> Value {
    let reply = handler
        .handle_message(json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        }))
        .await
        .expect("served response");
    assert!(reply.error.is_none(), "{name}: {:?}", reply.error);
    let result = reply.result.unwrap();
    assert_ne!(result["isError"], true, "{name}: {result}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn near(value: &Value, expected: f64) {
    let actual = value.as_f64().expect("numeric coordinate");
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn placement(value: &Value, x: f64, y: f64, rotation: f64, mirrors: (bool, bool)) {
    near(&value["x"], x);
    near(&value["y"], y);
    near(&value["rotation"], rotation);
    assert_eq!(value["mirror_x"], json!(mirrors.0), "{value}");
    assert_eq!(value["mirror_y"], json!(mirrors.1), "{value}");
}

fn pins(value: &Value, unit: u32, expected: &[(&str, f64, f64)]) {
    let pins = value["pins"].as_array().unwrap();
    assert_eq!(pins.len(), expected.len(), "{value}");
    for &(number, x, y) in expected {
        let matching: Vec<_> = pins.iter().filter(|pin| pin["number"] == number).collect();
        assert_eq!(matching.len(), 1, "{number}: {value}");
        let pin = matching[0];
        assert_eq!(pin["unit"], unit);
        near(&pin["x"], x);
        near(&pin["y"], y);
        assert!(pin["name"].is_string());
        assert!(pin["orientation_degrees"].is_number());
        assert!(pin["length_mm"].is_number());
    }
}

struct MirrorCase {
    reference: &'static str,
    x: f64,
    y: f64,
    rotation: f64,
    mirrors: (bool, bool),
    pins: &'static [(&'static str, f64, f64)],
}

// Raw placement tokens and library pin offsets from the KiCad 10.0.6 resave.
// Coordinates are literal sheet-axis reflections, not computed by the code under test.
#[rustfmt::skip]
const MIRROR_CASES: &[MirrorCase] = &[
    MirrorCase {
        reference: "D1", x: 101.6, y: 50.8, rotation: 0.0, mirrors: (false, false),
        pins: &[("1", 97.79, 50.8), ("2", 105.41, 50.8)],
    },
    MirrorCase {
        reference: "D2", x: 101.6, y: 76.2, rotation: 0.0, mirrors: (true, false),
        pins: &[("1", 97.79, 76.2), ("2", 105.41, 76.2)],
    },
    MirrorCase {
        reference: "U1", x: 139.7, y: 50.8, rotation: 0.0, mirrors: (false, false),
        pins: &[("1", 132.08, 48.26), ("2", 139.7, 58.42), ("3", 132.08, 50.8),
                ("4", 144.78, 50.8), ("5", 147.32, 48.26)],
    },
    MirrorCase {
        reference: "U2", x: 139.7, y: 76.2, rotation: 0.0, mirrors: (false, true),
        pins: &[("1", 147.32, 73.66), ("2", 139.7, 83.82), ("3", 147.32, 76.2),
                ("4", 134.62, 76.2), ("5", 132.08, 73.66)],
    },
    MirrorCase {
        reference: "U4", x: 177.8, y: 76.2, rotation: 90.0, mirrors: (true, false),
        pins: &[("1", 175.26, 68.58), ("2", 185.42, 76.2), ("3", 177.8, 68.58),
                ("4", 177.8, 81.28), ("5", 175.26, 83.82)],
    },
    MirrorCase {
        reference: "U5", x: 215.9, y: 50.8, rotation: 0.0, mirrors: (true, false),
        pins: &[("1", 208.28, 53.34), ("2", 215.9, 43.18), ("3", 208.28, 50.8),
                ("4", 220.98, 50.8), ("5", 223.52, 53.34)],
    },
];

fn assert_mirror_case(value: &Value, case: &MirrorCase) {
    assert_eq!(value["reference"], case.reference);
    placement(value, case.x, case.y, case.rotation, case.mirrors);
    near(&value["component_x"], case.x);
    near(&value["component_y"], case.y);
    assert_eq!(value["unit_count"], 1);
    let units = value["units"].as_array().unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0]["unit"], 1);
    placement(&units[0], case.x, case.y, case.rotation, case.mirrors);
    pins(&units[0], 1, case.pins);
    assert_eq!(value["pins"], units[0]["pins"]);
}

#[tokio::test]
async fn served_single_pin_locations_report_native_mirrors() {
    let api = handler().await;
    let (_target, path) = fixture(MIRRORS);
    let (_other, other) = fixture(MULTI_UNIT);
    // Same basename and reference, different saved document, using one handler.
    let decoy = call(
        &api,
        "get_schematic_pin_locations",
        json!({"schematic": other, "reference": "U1"}),
    )
    .await;
    assert_eq!(decoy["unit_count"], 3);
    for case in MIRROR_CASES {
        let result = call(
            &api,
            "get_schematic_pin_locations",
            json!({"schematic": path, "reference": case.reference}),
        )
        .await;
        assert_mirror_case(&result, case);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), MIRRORS);
    assert_eq!(std::fs::read_to_string(other).unwrap(), MULTI_UNIT);
}

#[tokio::test]
async fn served_batch_pin_locations_report_native_mirrors() {
    let api = handler().await;
    let (_target, path) = fixture(MIRRORS);
    let mut references: Vec<_> = MIRROR_CASES.iter().map(|case| case.reference).collect();
    references.push("U"); // Exact-reference matching, not a prefix.
    let result = call(
        &api,
        "batch_get_schematic_pin_locations",
        json!({"schematic": path, "references": references}),
    )
    .await;
    let components = result["components"].as_array().unwrap();
    assert_eq!(components.len(), MIRROR_CASES.len() + 1);
    for (value, case) in components.iter().zip(MIRROR_CASES) {
        assert_mirror_case(value, case);
    }
    assert_eq!(
        components.last().unwrap(),
        &json!({
            "reference": "U", "error": "Component 'U' not found"
        })
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), MIRRORS);
}

fn assert_multi_unit(value: &Value, mixed: bool) {
    assert_eq!(value["reference"], "U1");
    assert_eq!(value["unit_count"], 3);
    placement(value, 160.02, 64.77, 0.0, (mixed, false));
    near(&value["component_x"], 160.02);
    near(&value["component_y"], 64.77);
    let units = value["units"].as_array().unwrap();
    assert_eq!(units.len(), 3);
    let cases = [
        (
            1,
            160.02,
            64.77,
            (mixed, false),
            if mixed {
                [
                    ("7", 152.4, 64.77),
                    ("8", 157.48, 54.61),
                    ("6", 160.02, 74.93),
                ]
            } else {
                [
                    ("7", 152.4, 64.77),
                    ("8", 157.48, 74.93),
                    ("6", 160.02, 54.61),
                ]
            },
        ),
        (
            2,
            157.48,
            107.95,
            (false, mixed),
            if mixed {
                [
                    ("2", 165.1, 107.95),
                    ("3", 160.02, 118.11),
                    ("1", 157.48, 97.79),
                ]
            } else {
                [
                    ("2", 149.86, 107.95),
                    ("3", 154.94, 118.11),
                    ("1", 157.48, 97.79),
                ]
            },
        ),
        (
            3,
            63.5,
            74.93,
            (false, false),
            [("4", 60.96, 86.36), ("9", 63.5, 86.36), ("5", 66.04, 86.36)],
        ),
    ];
    for (number, x, y, mirrors, expected_pins) in cases {
        let matching: Vec<_> = units.iter().filter(|unit| unit["unit"] == number).collect();
        assert_eq!(matching.len(), 1);
        placement(matching[0], x, y, 0.0, mirrors);
        pins(matching[0], number, &expected_pins);
    }
    let flattened: Vec<_> = units
        .iter()
        .flat_map(|unit| unit["pins"].as_array().unwrap().iter().cloned())
        .collect();
    assert_eq!(value["pins"], json!(flattened));
}

#[tokio::test]
async fn served_pin_locations_keep_each_units_own_mirror() {
    let api = handler().await;
    let (_target, path) = fixture(MULTI_UNIT);
    for mixed in [false, true] {
        if mixed {
            // Controlled variation of the native file through the typed writer.
            // This is NOT claimed to be an additional KiCad-authored fixture.
            let mut sch = konnect_schematic_editor::Schematic::load(&path).unwrap();
            let mut changed = 0;
            for symbol in sch
                .symbols
                .iter_mut()
                .filter(|s| s.reference() == Some("U1"))
            {
                symbol.set_mirror(match symbol.unit {
                    1 => Some("x"),
                    2 => Some("y"),
                    3 => None,
                    _ => panic!("unexpected unit"),
                });
                changed += 1;
            }
            assert_eq!(changed, 3);
            sch.save(&path).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let single = call(
            &api,
            "get_schematic_pin_locations",
            json!({"schematic": path, "reference": "U1"}),
        )
        .await;
        assert_multi_unit(&single, mixed);
        let batch = call(
            &api,
            "batch_get_schematic_pin_locations",
            json!({"schematic": path, "references": ["U1"]}),
        )
        .await;
        assert_eq!(batch["components"], json!([single]));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

// Independent literals transcribed from the top-level label forms in the
// KiCad 10.0.5 resave. Repeated names deliberately have different identities.
#[rustfmt::skip]
const KICAD_LABELS: &[(&str, &str, &str, f64, f64)] = &[
    ("NetLabel", "AAA", "06aae51d-ad92-45a3-ab6b-a2934ec12f2a", 55.88, 119.38),
    ("NetLabel", "ALT", "15de34f4-cedd-40a3-9159-f3eff2ac13bb", 59.69, 88.9),
    ("NetLabel", "SDA", "35064194-a7cb-4fcc-b60a-539f8e03bbbc", 128.27, 77.47),
    ("NetLabel", "SCL", "3893093c-b550-492c-80fd-10739f176fb5", 128.27, 80.01),
    ("NetLabel", "ZZZ", "5990e890-7fcd-4caf-ac60-269b1b5f7d3e", 85.09, 119.38),
    ("NetLabel", "ALT", "849c7341-fc4a-4c95-a59f-68f8994893f5", 35.56, 88.9),
    ("NetLabel", "SDA", "8a222e21-42d9-4588-bafd-c4947977eed0", 160.02, 66.04),
    ("NetLabel", "PULLUP", "9724b793-eefa-47df-a8b9-9c0dc04b968f", 182.88, 52.07),
    ("NetLabel", "MIX", "a102cfc5-873d-4e3d-b1ba-a697aa9366ca", 115.57, 119.38),
    ("NetLabel", "VCC", "ab408dff-d6ba-43fb-abfa-bfff6771323d", 110.49, 69.85),
    ("NetLabel", "ZZZ", "b6d04cca-e63c-4dab-aa7b-48bc7b576c07", 58.42, 119.38),
    ("NetLabel", "SCL", "f7a3ae8e-998e-4051-990b-eda340b2e7de", 175.26, 66.04),
    ("GlobalLabel", "SYS", "821185fe-9fdd-4020-a37c-7b17c7a5d1ef", 167.64, 49.53),
    ("GlobalLabel", "RETURN", "c8e4d402-48cc-4823-9cc2-f7275f5e2973", 110.49, 90.17),
    ("HierarchicalLabel", "MIX_H", "f9df2d51-8230-4a59-b15b-468f176904ac", 118.11, 119.38),
];

fn expected_labels() -> Value {
    json!(KICAD_LABELS
        .iter()
        .map(|&(kind, net, uuid, x, y)| {
            json!({"uuid": uuid, "net": net, "type": kind, "x": x, "y": y, "rotation": 0.0})
        })
        .collect::<Vec<_>>())
}

#[tokio::test]
async fn served_listed_labels_carry_stable_native_uuids() {
    let api = handler().await;
    let (_target, path) = fixture(LABELS);
    let (_other, other) = fixture(MIRRORS);
    let empty = call(&api, "list_schematic_labels", json!({"schematic": other})).await;
    assert_eq!(empty, json!({"count": 0, "labels": []}));
    let expected = json!({"count": 15, "labels": expected_labels()});
    for _ in 0..2 {
        let listed = call(&api, "list_schematic_labels", json!({"schematic": path})).await;
        assert_eq!(listed, expected);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), LABELS);
    assert_eq!(std::fs::read_to_string(other).unwrap(), MIRRORS);
}

#[tokio::test]
async fn served_listed_label_uuids_delete_only_the_selected_instances() {
    let api = handler().await;
    let (_target, path) = fixture(LABELS);
    let (_other, other) = fixture(LABELS);
    let listed = call(&api, "list_schematic_labels", json!({"schematic": path})).await;
    assert_eq!(listed, json!({"count": 15, "labels": expected_labels()}));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), LABELS);
    // One of the two SDA labels plus one of each other kind, taken from the
    // served response, not passed to the writer from the oracle.
    let selected: Vec<_> = listed["labels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|label| {
            (label["net"] == "SDA" && label["x"] == 128.27)
                || label["net"] == "SYS"
                || label["net"] == "MIX_H"
        })
        .map(|label| label["uuid"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        selected,
        [
            "35064194-a7cb-4fcc-b60a-539f8e03bbbc",
            "821185fe-9fdd-4020-a37c-7b17c7a5d1ef",
            "f9df2d51-8230-4a59-b15b-468f176904ac",
        ]
    );
    let deleted = call(
        &api,
        "batch_delete",
        json!({"schematic": path, "uuids": selected}),
    )
    .await;
    assert_eq!(deleted["deleted_count"], 3);
    let mut observed = deleted["deleted_item_uuids"].as_array().unwrap().clone();
    observed.sort_by_key(|value| value.as_str().unwrap().to_owned());
    let mut wanted = selected.iter().map(|uuid| json!(uuid)).collect::<Vec<_>>();
    wanted.sort_by_key(|value| value.as_str().unwrap().to_owned());
    assert_eq!(observed, wanted);
    assert_eq!(deleted["deleted_components"], json!([]));
    assert_eq!(deleted["errors"], json!([]));

    let remaining = call(&api, "list_schematic_labels", json!({"schematic": path})).await;
    let expected: Vec<_> = expected_labels()
        .as_array()
        .unwrap()
        .iter()
        .filter(|label| !selected.iter().any(|uuid| label["uuid"] == *uuid))
        .cloned()
        .collect();
    assert_eq!(remaining, json!({"count": 12, "labels": expected}));
    // Compare the entire saved tree, not just counts. Every unselected label,
    // component, wire, junction, no-connect, library and metadata form must stay.
    let SexpNode::List(mut expected_tree) = parse_sexp(LABELS).unwrap() else {
        panic!("schematic root");
    };
    expected_tree.retain(|node| {
        !matches!(
            node.head(),
            Some("label" | "global_label" | "hierarchical_label")
        ) || !node
            .find_str("uuid")
            .is_some_and(|uuid| selected.iter().any(|id| id == uuid))
    });
    let saved = parse_sexp(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved, SexpNode::List(expected_tree));
    assert_eq!(std::fs::read_to_string(other).unwrap(), LABELS);
}
