//! Shared independent source oracle. No production Text fields are used here,
//! so the same regressions compile against the pre-fix Text implementation.

use konnect_sexp::parser::{parse_sexp, SexpNode};
use konnect_sexp::writer::find_direct_child_blocks;
use std::collections::BTreeMap;

pub const NATIVE: &str = include_str!("../fixtures/text_preservation_kicad10.kicad_sch");
pub const UUIDS: [&str; 2] = [
    "7a1204f2-6377-4d90-b301-42e19a80a973",
    "fdd0a080-44f5-467e-8b25-1ae4c1590a2a",
];

pub fn ranges(source: &str) -> Vec<(usize, usize)> {
    let root = parse_sexp(source).unwrap();
    assert_eq!(root.head(), Some("kicad_sch"));
    let ranges: Vec<_> = find_direct_child_blocks(source, "kicad_sch")
        .into_iter()
        .filter(|&(start, end)| {
            parse_sexp(&source[start..end]).unwrap().head() == Some("text")
        })
        .collect();
    assert_eq!(ranges.len(), root.find_all("text").len());
    assert_eq!(ranges.len(), UUIDS.len(), "fixture must contain both texts");
    ranges
}

/// Source slices from opening paren through matching closing paren, including
/// every internal byte. No line ending, whitespace or quoting normalization.
pub fn blocks(source: &str) -> BTreeMap<String, &str> {
    let mut out = BTreeMap::new();
    for (start, end) in ranges(source) {
        let block = &source[start..end];
        let node = parse_sexp(block).unwrap();
        let uuid = node.find_str("uuid").unwrap().to_owned();
        assert!(UUIDS.contains(&uuid.as_str()), "unexpected text UUID {uuid}");
        assert!(out.insert(uuid, block).is_none(), "duplicate text UUID");
    }
    assert_eq!(out.len(), UUIDS.len());
    out
}

pub fn assert_exact_blocks(before: &str, after: &str) {
    assert_eq!(blocks(after), blocks(before), "untouched text block bytes");
}

/// Controlled synthetic derivatives of the native fixture, not native captures.
pub fn with_state(state: Option<bool>) -> String {
    let mut source = NATIVE.to_owned();
    for (start, end) in ranges(NATIVE).into_iter().rev() {
        let original = &NATIVE[start..end];
        let old = "\t\t(exclude_from_sim no)\n";
        assert_eq!(original.matches(old).count(), 1);
        let replacement = match state {
            Some(true) => "\t\t(exclude_from_sim yes)\n",
            Some(false) => old,
            None => "",
        };
        source.replace_range(start..end, &original.replacen(old, replacement, 1));
    }
    source
}

pub fn assert_state(source: &str, state: Option<bool>) {
    let originals = blocks(NATIVE);
    for (uuid, block) in blocks(source) {
        let node = parse_sexp(block).unwrap();
        let original = parse_sexp(originals[&uuid]).unwrap();
        let children = node.children().unwrap();
        assert_eq!(node.get(1), original.get(1), "text content changed");
        // Only the leading content string belongs here, never a replayed copy.
        assert_eq!(
            children
                .iter()
                .skip(1)
                .filter(|n| Some(*n) == node.get(1))
                .count(),
            1,
            "text string duplicated for {uuid}"
        );
        let attributes = node.find_all("exclude_from_sim");
        assert_eq!(attributes.len(), usize::from(state.is_some()));
        if let Some(excluded) = state {
            assert_eq!(
                attributes[0].get(1).and_then(SexpNode::as_str),
                Some(if excluded { "yes" } else { "no" }),
                "{uuid}: exclusion value"
            );
            let exclusion = children
                .iter()
                .position(|n| n.head() == Some("exclude_from_sim"))
                .unwrap();
            let at = children.iter().position(|n| n.head() == Some("at")).unwrap();
            assert!(exclusion < at, "{uuid}: exclusion must precede at");
        }
    }
}

pub fn with_unknown(state: Option<bool>) -> String {
    let source = with_state(state);
    let mut out = source.clone();
    for (start, end) in ranges(&source).into_iter().rev() {
        let block = &source[start..end];
        let at = block.find("\t\t(at ").unwrap();
        let mut edited = block.to_owned();
        edited.insert_str(
            at,
            "\t\t(future_text_child (nested \"quoted \\\"payload\\\" (parentheses)\"))\n\
             \t\tfuture_bare_flag\n\t\t\"future quoted argument\"\n",
        );
        assert_ne!(edited, block);
        out.replace_range(start..end, &edited);
    }
    out
}

fn unknown_children(block: &str) -> Vec<SexpNode> {
    parse_sexp(block)
        .unwrap()
        .children()
        .unwrap()
        .iter()
        .skip(2)
        .filter(|n| {
            !n.head()
                .is_some_and(|tag| ["exclude_from_sim", "at", "effects", "uuid"].contains(&tag))
        })
        .cloned()
        .collect()
}

/// Preserve unknown values, nesting, scalar/list distinction and their mutual
/// order. Their slots among modelled children are intentionally not asserted.
pub fn assert_unknown(before: &str, after: &str) {
    let written = blocks(after);
    for (uuid, block) in blocks(before) {
        let expected = unknown_children(block);
        assert_eq!(expected.len(), 3, "synthetic unknown-child precondition");
        assert_eq!(unknown_children(written[&uuid]), expected, "{uuid}");
    }
}
