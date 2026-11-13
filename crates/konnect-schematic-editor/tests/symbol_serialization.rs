//! Keep these tests compilable with the complete exact pre-fix symbol.rs.
//! No new public fields or methods are used.

use konnect_schematic_editor::sexp::parser::parse;
use konnect_schematic_editor::types::At;
use konnect_schematic_editor::{Schematic, Symbol};

#[path = "support/symbol_serialization.rs"]
mod fixture;

fn round_trip(source: &str) -> String {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("mixer.kicad_sch");
    let output = directory.path().join("saved.kicad_sch");
    std::fs::write(&input, source).unwrap();
    let schematic = Schematic::load(&input).unwrap();
    assert_eq!(schematic.symbols.len(), 25);
    schematic.save(&output).unwrap();
    assert_eq!(Schematic::load(&output).unwrap().symbols.len(), 25);
    std::fs::read_to_string(output).unwrap()
}

#[test]
fn native_25_symbol_blocks_round_trip_exactly() {
    fixture::assert_native(fixture::NATIVE);
    let written = round_trip(fixture::NATIVE);
    fixture::assert_exact(fixture::NATIVE, &written);
    fixture::assert_attributes(fixture::NATIVE, &written);
    fixture::assert_native(&written);
}

#[test]
fn flag_syntax_survives_unrelated_writes() {
    for (name, clause, effective) in fixture::FLAG_CASES {
        let source = fixture::with_flag(*clause);
        let all = fixture::blocks(&source);
        let symbol = Symbol::from_sexp(&parse(all[fixture::TARGET]).unwrap()).unwrap();
        assert_eq!(symbol.fields_autoplaced, *effective, "{name}: effective bool");
        let written = round_trip(&source);
        fixture::assert_others_exact(&source, &written);
        fixture::assert_attributes(&source, &written);
    }
}

#[test]
fn invalid_raw_attributes_and_duplicate_clauses_survive_once() {
    for (body, positions) in fixture::RAW_CASES {
        let source = fixture::with_raw_attributes(fixture::NATIVE, body, positions);
        let written = round_trip(&source);
        fixture::assert_attributes(&source, &written);
        fixture::assert_others_exact(&source, &written);
    }
}

#[test]
fn unknown_scalar_and_list_children_keep_content_and_mutual_order() {
    for (_, clause, _) in fixture::FLAG_CASES {
        let source = fixture::with_unknown(&fixture::with_flag(*clause));
        let written = round_trip(&source);
        fixture::assert_attributes(&source, &written);
        fixture::assert_unknown(&source, &written);
    }
}

#[test]
fn intentional_bool_edits_override_raw_syntax_with_yes_or_no() {
    for (name, clause, effective) in fixture::FLAG_CASES {
        let source = fixture::with_flag(*clause);
        let mut schematic = Schematic::from_source("synthetic.kicad_sch", source.as_str()).unwrap();
        let symbol = schematic
            .symbols
            .iter_mut()
            .find(|symbol| symbol.uuid == fixture::TARGET)
            .unwrap();
        assert_eq!(symbol.fields_autoplaced, *effective, "{name}");
        symbol.fields_autoplaced = !*effective;
        let written = schematic.to_source();
        fixture::assert_canonical_flag(&written, !*effective);
        let original = fixture::blocks(&source);
        for (uuid, block) in fixture::blocks(&written) {
            if uuid != fixture::TARGET {
                assert_eq!(block, original[&uuid], "{name}: untouched {uuid}");
            }
        }
        let reloaded = Schematic::from_source("synthetic.kicad_sch", written.as_str()).unwrap();
        let target = reloaded
            .symbols
            .iter()
            .find(|s| s.uuid == fixture::TARGET)
            .unwrap();
        assert_eq!(target.fields_autoplaced, !*effective, "{name}: reloaded bool");
        fixture::assert_canonical_flag(&reloaded.to_source(), !*effective);
    }
}

#[test]
fn constructors_and_existing_exhaustive_literals_remain_source_compatible() {
    let mut symbol = Symbol::new("Device:R", 1.27, 2.54);
    assert!(!symbol.fields_autoplaced);
    let node = symbol.to_sexp();
    for tag in ["body_style", "in_pos_files", "fields_autoplaced"] {
        assert!(node.find(tag).is_none(), "constructor must not invent {tag}");
    }
    symbol.fields_autoplaced = true;
    assert_eq!(symbol.to_sexp().get_value("fields_autoplaced"), Some("yes"));
    let literal = Symbol {
        lib_name: None,
        lib_id: "Device:R".to_owned(),
        at: At::new(1.27, 2.54),
        mirror: None,
        unit: 1,
        exclude_from_sim: None,
        in_bom: true,
        on_board: true,
        dnp: false,
        fields_autoplaced: false,
        uuid: "old-client-literal".to_owned(),
        properties: vec![],
        raw_sub_nodes: vec![],
    };
    assert!(literal.to_sexp().find("fields_autoplaced").is_none());
}
