//! Independent source oracle. Never derives expected blocks from Symbol.
//! The shared native fixture and its provenance remain unchanged.

use konnect_sexp::parser::{parse_sexp, SexpNode};
use konnect_sexp::writer::find_direct_child_blocks;
use std::collections::{BTreeMap, BTreeSet};

pub const NATIVE: &str = include_str!("../fixtures/text_preservation_kicad10.kicad_sch");
pub const TARGET: &str = "083e79a4-747f-48eb-89ec-497063fa3806";
pub const MIRRORED: &str = "77bb0645-1955-4d0c-9936-5f6568f7bc8e";
pub const UUIDS: [&str; 25] = [
    "083e79a4-747f-48eb-89ec-497063fa3806",
    "24e34329-f9ae-4c46-9bf8-025b1c6d9b36",
    "29ef6bf1-cc9b-4478-affe-382f65a62c7c",
    "2a536d86-60ce-47d1-a7a6-7f997498f1a4",
    "31fe4e0d-615d-463a-960a-d846e705bf57",
    "379f7fd7-11be-48c8-8081-e23aa9633cd2",
    "5424024b-9c73-4529-b2d7-4135cf4f4826",
    "64b3ccda-14ab-4779-8d7d-dcb4a8685e04",
    "77bb0645-1955-4d0c-9936-5f6568f7bc8e",
    "7b6efc27-7552-4dfe-b337-b08fb5f64b34",
    "8af92826-d6bd-4ff7-92c3-2ddaa27a28f6",
    "96ee1e48-6c38-49e0-bd38-db0e4616f7bb",
    "a466852c-1d0a-46a3-96e0-2858aec5ee6f",
    "a75577a7-230f-4110-91b1-4f193517dc0c",
    "ad4523e9-7e18-42a7-8ff9-7f68a6d07442",
    "b58789dd-b4ae-4c23-8716-d2eb5a5017e9",
    "b6ad1337-3824-4e8a-8dec-58ba79a75eed",
    "bc4abb49-626d-494b-a9aa-ec713c163ae1",
    "cdc7583c-ad87-4580-ac97-0dcd7d01ebd7",
    "d158c0d5-a0da-4fa1-aa41-90b591654a45",
    "e602d9a5-3e5d-4b3b-8cd4-b5d98dd1f02e",
    "f987e89c-ed4c-4191-a15b-7fdce7e2e2cd",
    "fcd44701-4739-4141-9b99-b91dc9cd07b3",
    "fef6fbd9-9c5e-4479-8d68-cf394794f78b",
    "ff2a7df1-8112-4d44-b23e-353353155b30",
];

pub const FLAG_CASES: &[(&str, Option<&str>, bool)] = &[
    ("yes", Some("(fields_autoplaced yes)"), true),
    ("no", Some("(fields_autoplaced no)"), false),
    ("bare", Some("(fields_autoplaced)"), true),
    ("absence", None, false),
    ("unknown", Some("(fields_autoplaced future_value)"), true),
    ("opaque-false", Some("(fields_autoplaced false)"), true),
    ("quoted", Some("(fields_autoplaced \"no\")"), true),
    (
        "malformed",
        Some("(fields_autoplaced (future \"payload\") no)"),
        true,
    ),
    (
        "duplicates-no-first",
        Some("(fields_autoplaced no)\n\t\t(fields_autoplaced yes)"),
        false,
    ),
    (
        "duplicates-yes-first",
        Some("(fields_autoplaced yes)\n\t\t(fields_autoplaced no)"),
        true,
    ),
];

pub fn ranges(source: &str) -> Vec<(usize, usize)> {
    let root = parse_sexp(source).unwrap();
    assert_eq!(root.head(), Some("kicad_sch"));
    let ranges: Vec<_> = find_direct_child_blocks(source, "kicad_sch")
        .into_iter()
        .filter(|&(start, end)| parse_sexp(&source[start..end]).unwrap().head() == Some("symbol"))
        .collect();
    assert_eq!(ranges.len(), root.find_all("symbol").len());
    assert_eq!(
        ranges.len(),
        UUIDS.len(),
        "exactly 25 direct placed symbols"
    );
    ranges
}

/// Opening paren through matching close, with every internal byte unchanged.
/// No EOL, whitespace, number or quoting normalization.
pub fn blocks(source: &str) -> BTreeMap<String, &str> {
    let mut out = BTreeMap::new();
    for (start, end) in ranges(source) {
        let block = &source[start..end];
        let node = parse_sexp(block).unwrap();
        let uuid = node.find_str("uuid").unwrap().to_owned();
        assert!(out.insert(uuid, block).is_none(), "duplicate symbol UUID");
    }
    let expected: BTreeSet<_> = UUIDS.iter().copied().collect();
    let actual: BTreeSet<_> = out.keys().map(String::as_str).collect();
    assert_eq!(actual, expected, "exact native symbol UUID set");
    out
}

pub fn assert_native(source: &str) {
    assert!(!source.contains('\r'), "native LF bytes, never normalized");
    let blocks = blocks(source);
    let mut yes = 0;
    for block in blocks.values() {
        let node = parse_sexp(block).unwrap();
        assert_eq!(node.find_all("body_style").len(), 1);
        assert_eq!(node.find_str("body_style"), Some("1"));
        assert_eq!(node.find_all("in_pos_files").len(), 1);
        assert_eq!(node.find_str("in_pos_files"), Some("yes"));
        let flags = node.find_all("fields_autoplaced");
        if !flags.is_empty() {
            assert_eq!(flags.len(), 1);
            assert_eq!(flags[0].get(1).and_then(SexpNode::as_str), Some("yes"));
            yes += 1;
        }
    }
    assert_eq!(yes, 24, "24 native yes, one absent");
    let mirrored = parse_sexp(blocks[MIRRORED]).unwrap();
    assert_eq!(mirrored.find_str("mirror"), Some("y"));
    assert!(mirrored.find("fields_autoplaced").is_none());
}

pub fn assert_exact(before: &str, after: &str) {
    assert_eq!(
        blocks(after),
        blocks(before),
        "exact untouched symbol block bytes"
    );
}

pub fn assert_others_exact(before: &str, after: &str) {
    let original = blocks(before);
    for (uuid, block) in blocks(after) {
        if uuid != TARGET {
            assert_eq!(block, original[&uuid], "untouched symbol {uuid}");
        }
    }
}

fn nodes(block: &str, tag: &str) -> Vec<SexpNode> {
    parse_sexp(block)
        .unwrap()
        .find_all(tag)
        .into_iter()
        .cloned()
        .collect()
}

/// Values/multiplicity plus the three bounded canonical slots. This oracle
/// does not use the production Symbol parser or its boolean interpretation.
pub fn assert_attributes(before: &str, after: &str) {
    let written = blocks(after);
    for (uuid, original) in blocks(before) {
        let block = written[&uuid];
        for tag in ["body_style", "in_pos_files", "fields_autoplaced"] {
            assert_eq!(nodes(block, tag), nodes(original, tag), "{uuid}: {tag}");
        }
        let node = parse_sexp(block).unwrap();
        let children = node.children().unwrap();
        for (left, tag, right) in [
            ("unit", "body_style", "exclude_from_sim"),
            ("on_board", "in_pos_files", "dnp"),
            ("dnp", "fields_autoplaced", "uuid"),
        ] {
            let at = |tag| children.iter().position(|n| n.head() == Some(tag)).unwrap();
            let between = &children[at(left) + 1..at(right)];
            assert!(
                between.iter().all(|n| n.head() == Some(tag)),
                "{uuid}: only {tag} belongs between {left} and {right}"
            );
            assert_eq!(
                between.len(),
                node.find_all(tag).len(),
                "{uuid}: {tag} slot"
            );
        }
    }
}

fn edit_target(source: &str, edit: impl FnOnce(&str) -> String) -> String {
    let (start, end) = ranges(source)
        .into_iter()
        .find(|&(start, end)| {
            parse_sexp(&source[start..end]).unwrap().find_str("uuid") == Some(TARGET)
        })
        .unwrap();
    let mut out = source.to_owned();
    out.replace_range(start..end, &edit(&source[start..end]));
    out
}

/// Every derivative below is synthetic. The committed native fixture is never
/// rewritten, and these variants must not be submitted to native KiCad.
pub fn with_flag(clause: Option<&str>) -> String {
    edit_target(NATIVE, |block| {
        let original = "\t\t(fields_autoplaced yes)\n";
        assert_eq!(block.matches(original).count(), 1);
        let replacement = clause.map(|s| format!("\t\t{s}\n")).unwrap_or_default();
        block.replacen(original, &replacement, 1)
    })
}

pub fn with_raw_attributes(source: &str, body: &str, positions: &str) -> String {
    edit_target(source, |block| {
        let original_body = "\t\t(body_style 1)\n";
        let original_positions = "\t\t(in_pos_files yes)\n";
        assert_eq!(block.matches(original_body).count(), 1);
        assert_eq!(block.matches(original_positions).count(), 1);
        block
            .replacen(original_body, body, 1)
            .replacen(original_positions, positions, 1)
    })
}

pub const RAW_CASES: &[(&str, &str)] = &[
    ("", ""),
    ("\t\t(body_style 2)\n", "\t\t(in_pos_files no)\n"),
    ("\t\t(body_style \"2\")\n", "\t\t(in_pos_files \"no\")\n"),
    (
        "\t\t(body_style future_value)\n",
        "\t\t(in_pos_files future_value)\n",
    ),
    ("\t\t(body_style)\n", "\t\t(in_pos_files)\n"),
    (
        "\t\t(body_style \"not an integer\" (future 9))\n",
        "\t\t(in_pos_files (future \"no\") no)\n",
    ),
    (
        "\t\t(body_style 1)\n\t\t(body_style invalid)\n",
        "\t\t(in_pos_files no)\n\t\t(in_pos_files yes)\n",
    ),
];

pub fn with_unknown(source: &str) -> String {
    edit_target(source, |block| {
        let mut out = block.to_owned();
        let end = out.rfind("\n\t)").unwrap();
        out.insert_str(
            end,
            "\n\t\t(future_symbol_child (nested \"quoted \\\"payload\\\" (parentheses)\"))\
             \n\t\tfuture_bare_flag\n\t\t\"future quoted argument\"",
        );
        out
    })
}

fn unknown(block: &str) -> Vec<SexpNode> {
    parse_sexp(block)
        .unwrap()
        .children()
        .unwrap()
        .iter()
        .skip(1)
        .filter(|node| {
            node.head() == Some("future_symbol_child")
                || node.as_str() == Some("future_bare_flag")
                || node.as_str() == Some("future quoted argument")
        })
        .cloned()
        .collect()
}

pub fn assert_unknown(before: &str, after: &str) {
    let originals = blocks(before);
    let written = blocks(after);
    let expected = unknown(originals[TARGET]);
    assert_eq!(expected.len(), 3, "synthetic unknown-node precondition");
    assert_eq!(
        unknown(written[TARGET]),
        expected,
        "unknown contents and mutual order"
    );
}

pub fn assert_canonical_flag(source: &str, enabled: bool) {
    let all = blocks(source);
    let node = parse_sexp(all[TARGET]).unwrap();
    let flags = node.find_all("fields_autoplaced");
    assert_eq!(flags.len(), 1, "explicit bool edit writes exactly one flag");
    let expected = if enabled { "yes" } else { "no" };
    assert_eq!(
        flags[0],
        &SexpNode::List(vec![
            SexpNode::Atom("fields_autoplaced".to_owned()),
            SexpNode::Atom(expected.to_owned()),
        ]),
        "explicit bool edit writes canonical yes or no"
    );
}
