//! Synthetic private/quoted-name variants of an unchanged native KiCad fixture.
//! This is test-data generation, NOT a captured native private-field save.
//! See component_fields_kicad10.README.md and docs/stabilization/PRIVATE_FIELDS.md.

use konnect_sexp::{apply_edits, parse_sexp, writer::find_direct_child_blocks, SexpEdit};

pub const NATIVE: &str = include_str!("component_fields_kicad10.kicad_sch");

pub fn source(include_unknown_child: bool) -> String {
    let mut edits = Vec::new();
    let mut fields = 0;
    for (start, end) in find_direct_child_blocks(NATIVE, "kicad_sch") {
        let block = &NATIVE[start..end];
        if parse_sexp(block).unwrap().head() != Some("symbol") {
            continue;
        }
        for (a, b) in find_direct_child_blocks(block, "symbol") {
            let node = parse_sexp(&block[a..b]).unwrap();
            if node.head() != Some("property") {
                continue;
            }
            let name = node.get(1).and_then(|node| node.as_str()).unwrap();
            let name_start = start + a + "(property ".len();
            assert!(NATIVE[start + a..].starts_with("(property \""));
            if matches!(name, "MPN" | "LCSC") {
                edits.push(SexpEdit::insert(name_start, "private "));
                fields += 1;
                if name == "MPN" && include_unknown_child {
                    edits.push(SexpEdit::insert(
                        start + b - 1,
                        "\t(future_private_metadata (payload \"keep (nested) data\"))\n\t\t",
                    ));
                }
            } else if name == "Description" {
                // Quoted "private" is an ordinary field name, on the same
                // placement as two distinct bare-private fields.
                edits.push(SexpEdit::replace(
                    name_start,
                    name_start + "\"Description\"".len(),
                    "\"private\"",
                ));
            }
        }
    }
    assert_eq!(fields, 8, "four native placements, two private fields each");
    apply_edits(NATIVE.to_owned(), edits)
}
