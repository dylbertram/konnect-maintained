use konnect_schematic_editor::{
    sexp::{atom, parser::parse, qstr, tagged, SexpNode},
    Property, Schematic,
};

#[path = "../../konnect-core/tests/fixtures/private_fields.rs"]
mod fixture;

#[test]
fn private_property_round_trip_keeps_flag_name_value_and_ordered_children() {
    for private in [false, true] {
        for name in ["MPN", "private", "quoted \"name\""] {
            let mut children = vec![atom("property")];
            if private {
                children.push(atom("private"));
            }
            children.extend([
                qstr(name),
                qstr("value \"with quotes\" and \\backslash"),
                tagged("at", vec![atom("10"), atom("20"), atom("90")]),
                tagged("hide", vec![atom("yes")]),
                tagged("future", vec![qstr("(nested) text")]),
                tagged("effects", vec![tagged("font", vec![atom("bold")])]),
                atom("future_scalar"),
            ]);
            let input = SexpNode::List(children);
            let property = Property::from_sexp(&input).unwrap();
            assert_eq!(property.name, name);
            assert_eq!(property.value, "value \"with quotes\" and \\backslash");
            assert_eq!(property.private, private);
            assert_eq!(property.to_sexp(), input);
            let written = konnect_schematic_editor::sexp::writer::write(&property.to_sexp());
            assert_eq!(parse(&written).unwrap(), input);
        }
    }
}

#[test]
fn private_property_missing_name_or_value_is_not_a_shifted_field() {
    for input in [
        "(property)",
        "(property private)",
        "(property private \"MPN\")",
        "(property private \"MPN\" (at 1 2 0))",
        "(property \"MPN\")",
        "(property \"MPN\" (at 1 2 0))",
        "(property \"private\")",
    ] {
        assert!(
            Property::from_sexp(&parse(input).unwrap()).is_none(),
            "{input}"
        );
    }
    for input in [
        "(property \"private\" \"\")",
        "(property private \"MPN\" \"\")",
    ] {
        let property = Property::from_sexp(&parse(input).unwrap()).unwrap();
        assert_eq!(property.value, "");
    }
    assert!(!Property::new("private", "ordinary field").private);
}

#[test]
fn private_properties_survive_a_whole_file_model_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("component_fields.kicad_sch");
    let source = fixture::source(true);
    std::fs::write(&path, &source).unwrap();
    let schematic = Schematic::load(&path).unwrap();
    assert_eq!(schematic.symbols.len(), 4);
    schematic.save(&path).unwrap();
    let committed = std::fs::read_to_string(&path).unwrap();
    let before = parse(&source).unwrap();
    let after = parse(&committed).unwrap();
    // Compare property nodes, not the writer against its own reloaded model.
    // This detects value loss even when parsing and writing share the defect.
    for symbol in before.find_all("symbol") {
        let uuid = symbol.get_value("uuid").unwrap();
        let written = after
            .find_all("symbol")
            .into_iter()
            .find(|symbol| symbol.get_value("uuid") == Some(uuid))
            .unwrap();
        assert_eq!(written.find_all("property"), symbol.find_all("property"));
    }
    assert_eq!(after.find("lib_symbols"), before.find("lib_symbols"));
    let reloaded = Schematic::load(&path).unwrap();
    for symbol in reloaded.symbols.iter() {
        assert_eq!(
            symbol
                .properties
                .iter()
                .filter(|field| field.private)
                .count(),
            2
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

#[test]
fn native_properties_without_a_private_flag_do_not_gain_one() {
    let schematic = Schematic::from_source("component_fields.kicad_sch", fixture::NATIVE).unwrap();
    for symbol in schematic.symbols.iter() {
        for property in &symbol.properties {
            assert!(!property.private);
            assert!(matches!(
                property.to_sexp().args().first(),
                Some(SexpNode::Str(_))
            ));
        }
    }
}
