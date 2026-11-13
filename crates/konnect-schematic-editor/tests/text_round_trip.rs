//! Exact native text blocks, plus explicitly synthetic boolean/unknown cases.
//! Do not access added Text fields here: the original implementation must
//! compile for the coordinator's genuine failing-before regression.

use konnect_schematic_editor::{sexp::parser::parse, Schematic, Text};

#[path = "support/text_preservation.rs"]
mod fixture;

fn round_trip(source: &str) -> String {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("mixer.kicad_sch");
    let output = directory.path().join("saved.kicad_sch");
    std::fs::write(&input, source).unwrap();
    let schematic = Schematic::load(&input).unwrap();
    schematic.save(&output).unwrap();
    let written = std::fs::read_to_string(&output).unwrap();
    // Reload verifies the committed document, but is not the preservation oracle.
    assert_eq!(Schematic::load(&output).unwrap().texts.len(), 2);
    written
}

#[test]
fn native_text_blocks_round_trip_exactly() {
    assert!(!fixture::NATIVE.contains('\r'), "native LF capture required");
    fixture::assert_state(fixture::NATIVE, Some(false));
    let written = round_trip(fixture::NATIVE);
    fixture::assert_exact_blocks(fixture::NATIVE, &written);
    fixture::assert_state(&written, Some(false));
}

#[test]
fn yes_no_and_absence_keep_their_distinct_values_and_position() {
    for state in [Some(true), Some(false), None] {
        let source = fixture::with_state(state);
        fixture::assert_state(&source, state);
        let written = round_trip(&source);
        fixture::assert_state(&written, state);
        fixture::assert_exact_blocks(&source, &written);
        // Also exercise Text's public standalone parse/write contract.
        for block in fixture::blocks(&source).values() {
            let node = parse(block).unwrap();
            assert_eq!(Text::from_sexp(&node).unwrap().to_sexp(), node);
        }
    }
}

#[test]
fn unknown_scalar_and_list_children_survive_without_duplicate_content() {
    for state in [Some(true), Some(false), None] {
        let source = fixture::with_unknown(state);
        fixture::assert_state(&source, state);
        let written = round_trip(&source);
        fixture::assert_state(&written, state);
        fixture::assert_unknown(&source, &written);
    }
}

#[test]
fn constructor_does_not_invent_an_exclusion_flag_or_replay_content() {
    let text = Text::new("new \"text\" with \\slash", 1.27, 2.54);
    let node = text.to_sexp();
    assert!(node.find("exclude_from_sim").is_none());
    assert_eq!(
        node.args()
            .iter()
            .filter(|n| n.text() == Some(text.text.as_str()))
            .count(),
        1
    );
    assert_eq!(Text::from_sexp(&node).unwrap().to_sexp(), node);
}
