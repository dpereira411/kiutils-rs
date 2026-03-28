use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kiutils_kicad::load_schematic_tree;
use kiutils_kicad::{
    fork_symbol_to_lib, push_symbol_to_lib, rename_symbol_in_schematic, replace_symbol_from_lib,
    replace_symbol_from_lib_with_options, update_symbols_from_lib,
    update_symbols_from_lib_with_options, DesignRulesFile, Error, FootprintFile,
    ForkSymbolToLibOptions, FpLibTableFile, PcbFile, ProjectFile, SchematicFile, SymLibTableFile,
    SymbolLibFile, UpdateFromLibOptions, WorksheetFile, WriteMode,
};

// ---------------------------------------------------------------------------
// Schematic symbol property editing
// ---------------------------------------------------------------------------

#[test]
fn schematic_upsert_symbol_instance_property() {
    let path = tmp_file("sch_upsert_prop", "kicad_sch");
    let src = "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n  (lib_symbols (symbol \"Device:R\"))\n  (symbol (lib_id \"Device:R\") (at 100 50 0) (property \"Reference\" \"R1\" (at 0 0 0)) (property \"Value\" \"10k\" (at 0 0 0)))\n)\n";
    fs::write(&path, src).expect("write fixture");

    let mut doc = SchematicFile::read(&path).expect("read");
    doc.upsert_symbol_instance_property("R1", "MPN", "RC0603FR-0710KL")
        .upsert_symbol_instance_property("R1", "Manufacturer", "Yageo");

    let out = tmp_file("sch_upsert_prop_out", "kicad_sch");
    doc.write(&out).expect("write");
    let reread = SchematicFile::read(&out).expect("reread");

    let symbols = reread.symbol_instances();
    assert_eq!(symbols.len(), 1);
    let r1 = &symbols[0];
    assert_eq!(r1.reference.as_deref(), Some("R1"));
    assert_eq!(r1.value.as_deref(), Some("10k"));
    assert!(r1
        .properties
        .iter()
        .any(|(k, v)| k == "MPN" && v == "RC0603FR-0710KL"));
    assert!(r1
        .properties
        .iter()
        .any(|(k, v)| k == "Manufacturer" && v == "Yageo"));

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(out);
}

#[test]
fn schematic_remove_symbol_instance_property() {
    let path = tmp_file("sch_remove_prop", "kicad_sch");
    let src = "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n  (lib_symbols (symbol \"Device:R\"))\n  (symbol (lib_id \"Device:R\") (property \"Reference\" \"R1\" (at 0 0 0)) (property \"Value\" \"10k\" (at 0 0 0)) (property \"MPN\" \"RC0603\" (at 0 0 0)))\n)\n";
    fs::write(&path, src).expect("write fixture");

    let mut doc = SchematicFile::read(&path).expect("read");
    doc.remove_symbol_instance_property("R1", "MPN");

    let out = tmp_file("sch_remove_prop_out", "kicad_sch");
    doc.write(&out).expect("write");
    let reread = SchematicFile::read(&out).expect("reread");

    let symbols = reread.symbol_instances();
    assert_eq!(symbols.len(), 1);
    assert!(!symbols[0].properties.iter().any(|(k, _)| k == "MPN"));
    assert_eq!(symbols[0].reference.as_deref(), Some("R1"));
    assert_eq!(symbols[0].value.as_deref(), Some("10k"));

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(out);
}

#[test]
fn schematic_upsert_preserves_roundtrip() {
    let path = tmp_file("sch_roundtrip_prop", "kicad_sch");
    let src = "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n  (lib_symbols (symbol \"Device:R\"))\n  (symbol (lib_id \"Device:R\") (at 100 50 0) (property \"Reference\" \"R1\" (at 0 0 0) (effects (font (size 1.27 1.27)))) (property \"Value\" \"10k\" (at 0 0 0) (effects (font (size 1.27 1.27)))))\n)\n";
    fs::write(&path, src).expect("write fixture");

    let mut doc = SchematicFile::read(&path).expect("read");
    // Upsert MPN then write, re-read, upsert same value again — should be no-op
    doc.upsert_symbol_instance_property("R1", "MPN", "TEST123");

    let out = tmp_file("sch_roundtrip_prop_out", "kicad_sch");
    doc.write(&out).expect("write");

    let mut doc2 = SchematicFile::read(&out).expect("reread");
    let cst_before = doc2.cst().to_lossless_string().to_string();
    doc2.upsert_symbol_instance_property("R1", "MPN", "TEST123"); // no-op
    let cst_after = doc2.cst().to_lossless_string();
    assert_eq!(cst_before, cst_after, "no-op upsert should preserve CST");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(out);
}

#[test]
fn schematic_rename_creates_embedded_target_and_removes_unused_source() {
    let path = tmp_file("sch_rename_embedded", "kicad_sch");
    let src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
        "  (lib_symbols\n",
        "    (symbol \"OldLib:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (symbol \"R_0_1\"))\n",
        "  )\n",
        "  (symbol (lib_id \"OldLib:R\") (at 100 50 0)\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        ")\n",
    );
    fs::write(&path, src).expect("write fixture");

    let final_lib_id =
        rename_symbol_in_schematic(&path, "R1", "NewLib:R").expect("rename_symbol_in_schematic");
    assert_eq!(final_lib_id, "NewLib:R");

    let raw = fs::read_to_string(&path).expect("read back");
    assert!(raw.contains("(symbol \"NewLib:R\""));
    assert!(raw.contains("(symbol \"R_0_1\""));
    assert!(!raw.contains("(symbol \"OldLib:R\""));
    assert!(raw.contains("(lib_id \"NewLib:R\")"));

    let reread = SchematicFile::read(&path).expect("reread");
    let r1 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("R1"))
        .expect("R1 missing");
    assert_eq!(r1.lib_id.as_deref(), Some("NewLib:R"));

    let _ = fs::remove_file(path);
}

#[test]
fn schematic_rename_keeps_old_embedded_symbol_when_other_instances_still_use_it() {
    let path = tmp_file("sch_rename_keep_old", "kicad_sch");
    let src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
        "  (lib_symbols\n",
        "    (symbol \"OldLib:R\")\n",
        "  )\n",
        "  (symbol (lib_id \"OldLib:R\") (at 100 50 0)\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        "  (symbol (lib_id \"OldLib:R\") (at 120 50 0)\n",
        "    (property \"Reference\" \"R2\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        ")\n",
    );
    fs::write(&path, src).expect("write fixture");

    rename_symbol_in_schematic(&path, "R1", "NewLib:R").expect("rename_symbol_in_schematic");

    let raw = fs::read_to_string(&path).expect("read back");
    assert!(raw.contains("(symbol \"OldLib:R\")"));
    assert!(raw.contains("(symbol \"NewLib:R\")"));

    let reread = SchematicFile::read(&path).expect("reread");
    let syms = reread.symbol_instances();
    let r1 = syms
        .iter()
        .find(|s| s.reference.as_deref() == Some("R1"))
        .expect("R1 missing");
    let r2 = syms
        .iter()
        .find(|s| s.reference.as_deref() == Some("R2"))
        .expect("R2 missing");
    assert_eq!(r1.lib_id.as_deref(), Some("NewLib:R"));
    assert_eq!(r2.lib_id.as_deref(), Some("OldLib:R"));

    let _ = fs::remove_file(path);
}

#[test]
fn replace_symbol_from_lib_replaces_embedded_body_and_keeps_instance_value_by_default() {
    let dir = tmp_dir("replace_symbol_from_lib");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("User Library.kicad_sym");
    fs::write(
        &sch_path,
        concat!(
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
            "  (lib_symbols\n",
            "    (symbol \"User Library:Conn_02x05_Odd_Even\"\n",
            "      (property \"Reference\" \"J\" (at 0 0 0))\n",
            "      (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0))\n",
            "      (property \"Description\" \"old body\" (at 0 0 0))\n",
            "      (symbol \"Conn_02x05_Odd_Even_1_1\"))\n",
            "  )\n",
            "  (symbol (lib_id \"User Library:Conn_02x05_Odd_Even\") (at 10 10 0)\n",
            "    (property \"Reference\" \"J4\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0))\n",
            "    (property \"Description\" \"instance old\" (at 0 0 0)))\n",
            ")\n",
        ),
    )
    .expect("write sch");
    fs::write(
        &lib_path,
        concat!(
            "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
            "  (symbol \"User Library:Conn_02x05_Eurorack_Power\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Eurorack_Power\" (at 0 0 0))\n",
            "    (property \"Description\" \"library body\" (at 0 0 0))\n",
            "    (symbol \"Conn_02x05_Eurorack_Power_1_1\")\n",
            "  )\n",
            ")\n",
        ),
    )
    .expect("write lib");

    let lib_id = replace_symbol_from_lib(&sch_path, "J4", &lib_path, "Conn_02x05_Eurorack_Power")
        .expect("replace from lib");
    assert_eq!(lib_id, "User Library:Conn_02x05_Eurorack_Power");

    let raw = fs::read_to_string(&sch_path).expect("read sch");
    assert!(raw.contains("(symbol \"User Library:Conn_02x05_Eurorack_Power\""));
    assert!(raw.contains("library body"));
    assert!(!raw.contains("(symbol \"User Library:Conn_02x05_Odd_Even\""));
    assert!(raw.contains("(lib_id \"User Library:Conn_02x05_Eurorack_Power\")"));

    let reread = SchematicFile::read(&sch_path).expect("reread");
    let j4 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("J4"))
        .expect("J4 missing");
    assert_eq!(
        j4.lib_id.as_deref(),
        Some("User Library:Conn_02x05_Eurorack_Power")
    );
    assert_eq!(j4.value.as_deref(), Some("Conn_02x05_Odd_Even"));
    assert!(j4
        .properties
        .iter()
        .any(|(k, v)| k == "Description" && v == "library body"));

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn replace_symbol_from_lib_accepts_bare_symbol_names_in_library() {
    let dir = tmp_dir("replace_symbol_from_lib_bare");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("User Library.kicad_sym");
    fs::write(
        &sch_path,
        concat!(
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
            "  (lib_symbols (symbol \"User Library:Conn_02x05_Odd_Even\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0))))\n",
            "  (symbol (lib_id \"User Library:Conn_02x05_Odd_Even\") (at 10 10 0)\n",
            "    (property \"Reference\" \"J4\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0)))\n",
            ")\n",
        ),
    )
    .expect("write sch");
    fs::write(
        &lib_path,
        concat!(
            "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
            "  (symbol \"Conn_02x05_Eurorack_Power\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Eurorack_Power\" (at 0 0 0))\n",
            "    (symbol \"Conn_02x05_Eurorack_Power_1_1\")\n",
            "  )\n",
            ")\n",
        ),
    )
    .expect("write lib");

    let lib_id = replace_symbol_from_lib(&sch_path, "J4", &lib_path, "Conn_02x05_Eurorack_Power")
        .expect("replace from lib");
    assert_eq!(lib_id, "User Library:Conn_02x05_Eurorack_Power");

    let raw = fs::read_to_string(&sch_path).expect("read sch");
    assert!(raw.contains("(symbol \"User Library:Conn_02x05_Eurorack_Power\""));
    assert!(raw.contains("(symbol \"Conn_02x05_Eurorack_Power_1_1\""));
    assert!(raw.contains("(lib_id \"User Library:Conn_02x05_Eurorack_Power\")"));

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn replace_symbol_from_lib_can_override_instance_value() {
    let dir = tmp_dir("replace_symbol_from_lib_override");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("User Library.kicad_sym");
    fs::write(
        &sch_path,
        concat!(
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
            "  (lib_symbols (symbol \"User Library:Conn_02x05_Odd_Even\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0))))\n",
            "  (symbol (lib_id \"User Library:Conn_02x05_Odd_Even\") (at 10 10 0)\n",
            "    (property \"Reference\" \"J4\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0)))\n",
            ")\n",
        ),
    )
    .expect("write sch");
    fs::write(
        &lib_path,
        concat!(
            "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
            "  (symbol \"User Library:Conn_02x05_Eurorack_Power\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Eurorack_Power\" (at 0 0 0))\n",
            "  )\n",
            ")\n",
        ),
    )
    .expect("write lib");

    replace_symbol_from_lib_with_options(
        &sch_path,
        "J4",
        &lib_path,
        "Conn_02x05_Eurorack_Power",
        UpdateFromLibOptions {
            overwrite_value: true,
        },
    )
    .expect("replace from lib");

    let reread = SchematicFile::read(&sch_path).expect("reread");
    let j4 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("J4"))
        .expect("J4 missing");
    assert_eq!(j4.value.as_deref(), Some("Conn_02x05_Eurorack_Power"));

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn replace_symbol_from_lib_does_not_materialize_symbol_metadata_as_instance_properties() {
    let dir = tmp_dir("replace_symbol_from_lib_metadata");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("User Library.kicad_sym");
    fs::write(
        &sch_path,
        concat!(
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
            "  (lib_symbols (symbol \"User Library:Conn_02x05_Odd_Even\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0))))\n",
            "  (symbol (lib_id \"User Library:Conn_02x05_Odd_Even\") (at 10 10 0)\n",
            "    (property \"Reference\" \"J4\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Odd_Even\" (at 0 0 0)))\n",
            ")\n",
        ),
    )
    .expect("write sch");
    fs::write(
        &lib_path,
        concat!(
            "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
            "  (symbol \"User Library:Conn_02x05_Eurorack_Power\"\n",
            "    (property \"Reference\" \"J\" (at 0 0 0))\n",
            "    (property \"Value\" \"Conn_02x05_Eurorack_Power\" (at 0 0 0))\n",
            "    (property \"ki_keywords\" \"connector\" (at 0 0 0))\n",
            "    (property \"ki_fp_filters\" \"Connector*:*_2x??_*\" (at 0 0 0))\n",
            "    (property \"Description\" \"library body\" (at 0 0 0))\n",
            "  )\n",
            ")\n",
        ),
    )
    .expect("write lib");

    replace_symbol_from_lib(&sch_path, "J4", &lib_path, "Conn_02x05_Eurorack_Power")
        .expect("replace from lib");

    let reread = SchematicFile::read(&sch_path).expect("reread");
    let j4 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("J4"))
        .expect("J4 missing");
    assert!(j4.properties.iter().all(|(k, _)| k != "ki_keywords"));
    assert!(j4.properties.iter().all(|(k, _)| k != "ki_fp_filters"));
    assert!(j4
        .properties
        .iter()
        .any(|(k, v)| k == "Description" && v == "library body"));

    let raw = fs::read_to_string(&sch_path).expect("read sch");
    assert!(
        raw.contains("(property \"ki_keywords\" \"connector\""),
        "embedded symbol should retain ki_keywords metadata"
    );
    assert!(
        raw.contains("(property \"ki_fp_filters\" \"Connector*:*_2x??_*\""),
        "embedded symbol should retain ki_fp_filters metadata"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(dir);
}

// Schematic with a full lib_symbols section and instance properties matching a real KiCad file.
// Value = lib default ("L"), Footprint = "" (empty) — these are the exact values the agent
// reported as failing to update.
const KICAD_LIKE_SCH: &str = concat!(
    "(kicad_sch (version 20260101) (generator \"eeschema\") (generator_version \"9.0\")\n",
    "  (uuid \"root-uuid\")\n",
    "  (paper \"A4\")\n",
    "  (lib_symbols\n",
    "    (symbol \"Device:L\"\n",
    "      (property \"Reference\" \"L\" (at 0 1.27 0) (effects (font (size 1.27 1.27))))\n",
    "      (property \"Value\" \"L\" (at 0 -1.27 0) (effects (font (size 1.27 1.27))))\n",
    "      (property \"Footprint\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "      (property \"Datasheet\" \"~\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "    )\n",
    "    (symbol \"Device:C\"\n",
    "      (property \"Reference\" \"C\" (at 0.365 1.016 0) (effects (font (size 1.27 1.27)) (justify left)))\n",
    "      (property \"Value\" \"C\" (at 0.365 -1.016 0) (effects (font (size 1.27 1.27)) (justify left)))\n",
    "      (property \"Footprint\" \"\" (at 0.9652 0 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "      (property \"Datasheet\" \"~\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "    )\n",
    "  )\n",
    "  (symbol (lib_id \"Device:L\") (at 152.4 95.25 0) (unit 1)\n",
    "    (in_bom yes) (on_board yes) (dnp no)\n",
    "    (uuid \"l1-inst-uuid\")\n",
    "    (property \"Reference\" \"L1\" (at 153.67 95.25 90) (effects (font (size 1.27 1.27)) (justify left)))\n",
    "    (property \"Value\" \"L\" (at 151.13 95.25 90) (effects (font (size 1.27 1.27)) (justify left)))\n",
    "    (property \"Footprint\" \"\" (at 152.4 95.25 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "    (property \"Datasheet\" \"~\" (at 152.4 95.25 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "    (pin \"1\" (uuid \"pin1-uuid\"))\n",
    "    (pin \"2\" (uuid \"pin2-uuid\"))\n",
    "  )\n",
    "  (symbol (lib_id \"Device:C\") (at 120 100 0) (unit 1)\n",
    "    (in_bom yes) (on_board yes) (dnp no)\n",
    "    (uuid \"c1-inst-uuid\")\n",
    "    (property \"Reference\" \"C1\" (at 121.27 98.425 0) (effects (font (size 1.27 1.27)) (justify left)))\n",
    "    (property \"Value\" \"C\" (at 121.27 101.6 0) (effects (font (size 1.27 1.27)) (justify left)))\n",
    "    (property \"Footprint\" \"\" (at 120 100 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "    (property \"Datasheet\" \"~\" (at 120 100 0) (effects (font (size 1.27 1.27)) (hide yes)))\n",
    "    (pin \"1\" (uuid \"pin3-uuid\"))\n",
    "    (pin \"2\" (uuid \"pin4-uuid\"))\n",
    "  )\n",
    "  (wire (pts (xy 0 0) (xy 10 0)) (uuid \"w1\"))\n",
    ")\n",
);

#[test]
fn set_value_from_lib_default_persists() {
    // Bug reported: set-property Value on a symbol whose current value is the lib default
    // ("L") did not persist — reverted to lib_symbols default after write+reread.
    let path = tmp_file("sch_set_value_default", "kicad_sch");
    fs::write(&path, KICAD_LIKE_SCH).expect("write fixture");

    let mut doc = SchematicFile::read(&path).expect("read");
    let before = doc.symbol_instances();
    assert_eq!(
        before
            .iter()
            .find(|s| s.reference.as_deref() == Some("L1"))
            .unwrap()
            .value
            .as_deref(),
        Some("L"),
        "precondition: L1.Value should start as lib default \"L\""
    );

    doc.upsert_symbol_instance_property("L1", "Value", "10uH");
    let out = tmp_file("sch_set_value_default_out", "kicad_sch");
    doc.write(&out).expect("write");

    let reread = SchematicFile::read(&out).expect("reread");
    let l1 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("L1"))
        .expect("L1 lost after reread");
    assert_eq!(
        l1.value.as_deref(),
        Some("10uH"),
        "Value should be \"10uH\" after set-property, not reverted to lib default"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(out);
}

#[test]
fn set_footprint_from_empty_persists() {
    // Bug reported: set-property Footprint on C1/C2/C5 didn't persist (empty Footprint
    // remained). Required a second explicit set-property call to stick.
    let path = tmp_file("sch_set_fp_empty", "kicad_sch");
    fs::write(&path, KICAD_LIKE_SCH).expect("write fixture");

    let mut doc = SchematicFile::read(&path).expect("read");
    let before = doc.symbol_instances();
    assert_eq!(
        before
            .iter()
            .find(|s| s.reference.as_deref() == Some("C1"))
            .unwrap()
            .footprint
            .as_deref(),
        Some(""),
        "precondition: C1.Footprint should start as empty string"
    );

    doc.upsert_symbol_instance_property("C1", "Footprint", "Capacitor_SMD:C_0402_1005Metric");
    let out = tmp_file("sch_set_fp_empty_out", "kicad_sch");
    doc.write(&out).expect("write");

    let reread = SchematicFile::read(&out).expect("reread");
    let c1 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("C1"))
        .expect("C1 lost after reread");
    assert_eq!(
        c1.footprint.as_deref(),
        Some("Capacitor_SMD:C_0402_1005Metric"),
        "Footprint should persist after set-property, not revert to empty"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(out);
}

#[test]
fn set_value_then_footprint_both_persist() {
    // Tests the sequential CLI workflow: set Value, write, re-read, set Footprint, write,
    // re-read — both changes must survive.
    let path = tmp_file("sch_val_then_fp", "kicad_sch");
    fs::write(&path, KICAD_LIKE_SCH).expect("write fixture");

    // Step 1: set Value
    let mut doc = SchematicFile::read(&path).expect("read");
    doc.upsert_symbol_instance_property("L1", "Value", "10uH");
    doc.write(&path).expect("write after value");

    // Step 2: re-read then set Footprint (simulates separate CLI invocations)
    let mut doc2 = SchematicFile::read(&path).expect("re-read");
    doc2.upsert_symbol_instance_property("L1", "Footprint", "Inductor_SMD:L_0805_2012Metric");
    doc2.write(&path).expect("write after footprint");

    // Step 3: verify BOTH changes persisted
    let reread = SchematicFile::read(&path).expect("final reread");
    let l1 = reread
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("L1"))
        .expect("L1 lost");
    assert_eq!(
        l1.value.as_deref(),
        Some("10uH"),
        "Value should still be \"10uH\" after subsequent Footprint set"
    );
    assert_eq!(
        l1.footprint.as_deref(),
        Some("Inductor_SMD:L_0805_2012Metric"),
        "Footprint should persist after being set"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn has_symbol_instance_returns_correct() {
    let path = tmp_file("sch_has_sym", "kicad_sch");
    fs::write(&path, KICAD_LIKE_SCH).expect("write fixture");
    let doc = SchematicFile::read(&path).expect("read");

    assert!(doc.has_symbol_instance("L1"));
    assert!(doc.has_symbol_instance("C1"));
    assert!(!doc.has_symbol_instance("R99"));
    assert!(
        !doc.has_symbol_instance("l1"),
        "reference match must be case-sensitive"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn schematic_symbol_instances_extracts_all() {
    let path = tmp_file("sch_list_symbols", "kicad_sch");
    let src = "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n  (symbol (lib_id \"Device:R\") (property \"Reference\" \"R1\" (at 0 0 0)) (property \"Value\" \"10k\" (at 0 0 0)) (property \"Footprint\" \"R_0603\" (at 0 0 0)))\n  (symbol (lib_id \"Device:C\") (property \"Reference\" \"C1\" (at 0 0 0)) (property \"Value\" \"100nF\" (at 0 0 0)))\n)\n";
    fs::write(&path, src).expect("write fixture");

    let doc = SchematicFile::read(&path).expect("read");
    let symbols = doc.symbol_instances();

    assert_eq!(symbols.len(), 2);

    assert_eq!(symbols[0].reference.as_deref(), Some("R1"));
    assert_eq!(symbols[0].lib_id.as_deref(), Some("Device:R"));
    assert_eq!(symbols[0].value.as_deref(), Some("10k"));
    assert_eq!(symbols[0].footprint.as_deref(), Some("R_0603"));

    assert_eq!(symbols[1].reference.as_deref(), Some("C1"));
    assert_eq!(symbols[1].lib_id.as_deref(), Some("Device:C"));
    assert_eq!(symbols[1].value.as_deref(), Some("100nF"));
    assert_eq!(symbols[1].footprint, None);

    let _ = fs::remove_file(path);
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn tmp_file(name: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("kiutils_kicad_{name}_{nanos}.{ext}"))
}

/// Create a uniquely-named temporary directory. Callers must remove it when done.
fn tmp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kiutils_kicad_{tag}_{nanos}"));
    fs::create_dir_all(&dir).expect("create tmp_dir");
    dir
}

#[test]
// ANCHOR: pcb_roundtrip_test
fn pcb_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_pcb");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = PcbFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("pcb", "kicad_pcb");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}
// ANCHOR_END: pcb_roundtrip_test

#[test]
fn footprint_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_mod");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = FootprintFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("fp", "kicad_mod");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}

#[test]
fn libtable_fixture_unknown_and_canonical() {
    let src_path = fixture("fp-lib-table");

    let doc = FpLibTableFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().library_count, 1);
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("fplib", "table");
    doc.write_mode(&out, WriteMode::Canonical).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert!(got.contains("fp_lib_table"));

    let _ = fs::remove_file(out);
}

#[test]
fn symlib_fixture_unknown_and_canonical() {
    let src_path = fixture("sym-lib-table");

    let doc = SymLibTableFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().library_count, 1);
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("symlib", "table");
    doc.write_mode(&out, WriteMode::Canonical).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert!(got.contains("sym_lib_table"));

    let _ = fs::remove_file(out);
}

#[test]
fn dru_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_dru");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = DesignRulesFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().rule_count, 1);
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("dru", "kicad_dru");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}

#[test]
fn project_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_pro");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = ProjectFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().pinned_footprint_libs, vec!["A"]);
    assert_eq!(doc.ast().unknown_fields.len(), 1);

    let out = tmp_file("pro", "kicad_pro");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}

#[test]
fn symbol_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_sym");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = SymbolLibFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().symbol_count, 1);
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("sym", "kicad_sym");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}

#[test]
fn schematic_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_sch");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = SchematicFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().symbol_count, 1);
    assert_eq!(doc.ast().wire_count, 1);
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("sch", "kicad_sch");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}

#[test]
fn worksheet_fixture_roundtrip_lossless_and_unknown() {
    let src_path = fixture("sample.kicad_wks");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = WorksheetFile::read(&src_path).expect("parse");
    assert_eq!(doc.ast().line_count, 1);
    assert_eq!(doc.ast().rect_count, 1);
    assert_eq!(doc.ast().tbtext_count, 1);
    assert_eq!(doc.ast().unknown_nodes.len(), 1);

    let out = tmp_file("wks", "kicad_wks");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(out);
}

#[test]
fn pcb_multi_unknown_roundtrip_lossless() {
    let src = "(kicad_pcb (version 20260101) (generator pcbnew) (mystery_a 1) (mystery_b \"x\"))\n";
    let path = tmp_file("pcb_multi_unknown", "kicad_pcb");
    fs::write(&path, src).expect("write fixture");

    let doc = PcbFile::read(&path).expect("parse");
    assert_eq!(doc.ast().unknown_nodes.len(), 2);

    let out = tmp_file("pcb_multi_unknown_out", "kicad_pcb");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src);

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(out);
}

#[test]
fn footprint_rejects_malformed_root() {
    let path = tmp_file("footprint_bad_root", "kicad_mod");
    fs::write(&path, "(foo \"R_0603\" (version 20260101))\n").expect("write fixture");

    let err = FootprintFile::read(&path).expect_err("must fail");
    match err {
        Error::Validation(msg) => assert!(msg.contains("expected root token `footprint`")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_rejects_malformed_root() {
    let path = tmp_file("symbol_bad_root", "kicad_sym");
    fs::write(&path, "(foo (version 20260101))\n").expect("write fixture");

    let err = SymbolLibFile::read(&path).expect_err("must fail");
    match err {
        Error::Validation(msg) => assert!(msg.contains("expected root token `kicad_symbol_lib`")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn schematic_rejects_malformed_root() {
    let path = tmp_file("schematic_bad_root", "kicad_sch");
    fs::write(&path, "(foo (version 20260101))\n").expect("write fixture");

    let err = SchematicFile::read(&path).expect_err("must fail");
    match err {
        Error::Validation(msg) => assert!(msg.contains("expected root token `kicad_sch`")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn worksheet_rejects_malformed_root() {
    let path = tmp_file("worksheet_bad_root", "kicad_wks");
    fs::write(&path, "(foo (version 20260101))\n").expect("write fixture");

    let err = WorksheetFile::read(&path).expect_err("must fail");
    match err {
        Error::Validation(msg) => assert!(msg.contains("expected root token `kicad_wks`")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn fplib_rejects_malformed_root() {
    let path = tmp_file("fplib_bad_root", "table");
    fs::write(&path, "(sym_lib_table (version 7))\n").expect("write fixture");

    let err = FpLibTableFile::read(&path).expect_err("must fail");
    match err {
        Error::Validation(msg) => assert!(msg.contains("expected root token `fp_lib_table`")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn symlib_rejects_malformed_root() {
    let path = tmp_file("symlib_bad_root", "table");
    fs::write(&path, "(fp_lib_table (version 7))\n").expect("write fixture");

    let err = SymLibTableFile::read(&path).expect_err("must fail");
    match err {
        Error::Validation(msg) => assert!(msg.contains("expected root token `sym_lib_table`")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn future_version_adds_diagnostic_for_pcb_and_footprint() {
    let pcb_path = tmp_file("pcb_future_diag", "kicad_pcb");
    fs::write(
        &pcb_path,
        "(kicad_pcb (version 20270101) (generator pcbnew))\n",
    )
    .expect("write pcb");
    let pcb_doc = PcbFile::read(&pcb_path).expect("parse pcb");
    assert_eq!(pcb_doc.diagnostics().len(), 1);
    assert_eq!(pcb_doc.diagnostics()[0].code, "future_format");

    let fp_path = tmp_file("fp_future_diag", "kicad_mod");
    fs::write(
        &fp_path,
        "(footprint \"R\" (version 20270101) (generator pcbnew))\n",
    )
    .expect("write footprint");
    let fp_doc = FootprintFile::read(&fp_path).expect("parse footprint");
    assert_eq!(fp_doc.diagnostics().len(), 1);
    assert_eq!(fp_doc.diagnostics()[0].code, "future_format");

    let _ = fs::remove_file(pcb_path);
    let _ = fs::remove_file(fp_path);
}

#[test]
fn pcb_accepts_quoted_atoms_for_numeric_and_text_fields() {
    let src = "(kicad_pcb (version \"20260101\") (generator \"pcbnew\") (layers (0 \"F.Cu\" \"signal\")) (net 1 \"GND\"))\n";
    let path = tmp_file("pcb_quoted_atoms", "kicad_pcb");
    fs::write(&path, src).expect("write fixture");

    let doc = PcbFile::read(&path).expect("parse");
    assert_eq!(doc.ast().version, Some(20260101));
    assert_eq!(doc.ast().generator.as_deref(), Some("pcbnew"));
    assert_eq!(
        doc.ast().layers.first().and_then(|l| l.name.as_deref()),
        Some("F.Cu")
    );
    assert_eq!(
        doc.ast()
            .layers
            .first()
            .and_then(|l| l.layer_type.as_deref()),
        Some("signal")
    );
    assert_eq!(
        doc.ast().nets.first().and_then(|n| n.name.as_deref()),
        Some("GND")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn footprint_accepts_quoted_version_and_generator() {
    let src = "(footprint \"R\" (version \"20260101\") (generator \"pcbnew\") (layer \"F.Cu\") (property \"Reference\" \"R1\"))\n";
    let path = tmp_file("footprint_quoted_atoms", "kicad_mod");
    fs::write(&path, src).expect("write fixture");

    let doc = FootprintFile::read(&path).expect("parse");
    assert_eq!(doc.ast().version, Some(20260101));
    assert_eq!(doc.ast().generator.as_deref(), Some("pcbnew"));
    assert_eq!(doc.ast().layer.as_deref(), Some("F.Cu"));
    assert_eq!(doc.ast().property_count, 1);

    let _ = fs::remove_file(path);
}

// ─── Phase 6: real-world fixture corpus ───────────────────────────────────────

#[test]
fn corpus_multi_unit_sym_roundtrip() {
    let src_path = fixture("multi_unit_sym.kicad_sym");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = SymbolLibFile::read(&src_path).expect("parse");
    // Two top-level symbols: Device:R (single-unit) and Op:LM358 (multi-unit, 3 sub-symbols)
    assert_eq!(doc.ast().symbol_count, 2, "expected 2 top-level symbols");

    let summaries = &doc.ast().symbols;
    let names: Vec<_> = summaries.iter().filter_map(|s| s.name.as_deref()).collect();
    assert!(names.contains(&"Device:R"), "Device:R not found");
    assert!(names.contains(&"Op:LM358"), "Op:LM358 not found");

    // Op:LM358 has 4 properties (Reference, Value, Footprint, Datasheet)
    let lm358 = summaries
        .iter()
        .find(|s| s.name.as_deref() == Some("Op:LM358"))
        .unwrap();
    assert_eq!(lm358.properties.len(), 4);

    let out = tmp_file("corpus_sym", "kicad_sym");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src, "lossless roundtrip failed for multi_unit_sym");

    let _ = fs::remove_file(out);
}

#[test]
fn corpus_hierarchical_sch_roundtrip() {
    let src_path = fixture("hierarchical.kicad_sch");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = SchematicFile::read(&src_path).expect("parse");
    let ast = doc.ast();

    assert_eq!(ast.symbol_count, 2);
    assert_eq!(ast.wire_count, 3);
    assert_eq!(ast.junction_count, 1);
    assert_eq!(ast.no_connect_count, 1);
    assert_eq!(ast.bus_count, 1);
    assert_eq!(ast.bus_entry_count, 1);
    assert_eq!(ast.hierarchical_label_count, 1);
    assert_eq!(ast.sheet_count, 1);

    let filenames = doc.sheet_filenames();
    assert_eq!(filenames, vec!["sub_sheet.kicad_sch"]);

    let syms = doc.symbol_instances();
    assert_eq!(syms.len(), 2);
    assert!(syms.iter().any(|s| s.reference.as_deref() == Some("R1")));
    assert!(syms.iter().any(|s| s.reference.as_deref() == Some("C1")));

    let out = tmp_file("corpus_hier_sch", "kicad_sch");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(
        got, src,
        "lossless roundtrip failed for hierarchical.kicad_sch"
    );

    let _ = fs::remove_file(out);
}

#[test]
fn corpus_sub_sheet_roundtrip() {
    let src_path = fixture("sub_sheet.kicad_sch");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = SchematicFile::read(&src_path).expect("parse");
    let ast = doc.ast();
    assert_eq!(ast.symbol_count, 1);
    assert_eq!(ast.wire_count, 1);
    assert_eq!(ast.hierarchical_label_count, 1);

    let out = tmp_file("corpus_sub_sch", "kicad_sch");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(
        got, src,
        "lossless roundtrip failed for sub_sheet.kicad_sch"
    );

    let _ = fs::remove_file(out);
}

#[test]
fn corpus_load_schematic_tree() {
    let root = fixture("hierarchical.kicad_sch");
    let results = load_schematic_tree(&root);
    let sheets: Vec<_> = results
        .into_iter()
        .map(|r| r.expect("schematic load failed"))
        .collect();

    // Root + one sub-sheet
    assert_eq!(sheets.len(), 2, "expected root + 1 sub-sheet");
    // Root is first
    assert_eq!(sheets[0].ast().symbol_count, 2);
    // Sub-sheet is second
    assert_eq!(sheets[1].ast().symbol_count, 1);
    assert_eq!(sheets[1].ast().hierarchical_label_count, 1);
}

#[test]
fn corpus_complex_pcb_roundtrip() {
    let src_path = fixture("complex.kicad_pcb");
    let src = fs::read_to_string(&src_path).expect("read fixture");

    let doc = PcbFile::read(&src_path).expect("parse");
    let ast = doc.ast();

    assert_eq!(ast.footprint_count, 2);
    assert_eq!(ast.net_count, 3);
    assert_eq!(ast.trace_segment_count, 2);
    assert_eq!(ast.via_count, 1);
    assert_eq!(ast.zone_count, 1);
    assert_eq!(ast.group_count, 1);
    assert_eq!(ast.layer_count, 4);

    let out = tmp_file("corpus_complex_pcb", "kicad_pcb");
    doc.write(&out).expect("write");
    let got = fs::read_to_string(&out).expect("read out");
    assert_eq!(got, src, "lossless roundtrip failed for complex.kicad_pcb");

    let _ = fs::remove_file(out);
}

// ---------------------------------------------------------------------------
// push_symbol_to_lib
// ---------------------------------------------------------------------------

/// Push a symbol from schematic's lib_symbols into a different library.
/// Verifies: library updated with new lib_id, schematic instance updated,
/// new entry in lib_symbols, old entry removed (no other instance uses it).
#[test]
fn push_symbol_to_lib_cross_library() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
        "  (lib_symbols\n",
        "    (symbol \"Device:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (property \"Value\" \"R\" (at 0 0 0))\n",
        "      (symbol \"Device:R_1_1\"\n",
        "        (pin passive line (at 0 2.54 270) (length 0)\n",
        "          (name \"~\" (effects (font (size 1.27 1.27))))\n",
        "          (number \"1\" (effects (font (size 1.27 1.27))))))))\n",
        "  (symbol (lib_id \"Device:R\") (at 100 50 0) (unit 1) (uuid \"s1\")\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:Existing\"\n",
        "    (property \"Reference\" \"X\" (at 0 0 0))\n",
        "    (property \"Value\" \"Existing\" (at 0 0 0)))\n",
        ")\n",
    );

    // Use a temp dir so the lib file stem is exactly "MyLib"
    let dir = tmp_dir("push_cross");
    let sch_path = dir.join("top.kicad_sch");
    let lib_path = dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let result_lib_id = push_symbol_to_lib(&sch_path, "R1", &lib_path).expect("push_symbol_to_lib");
    assert_eq!(result_lib_id, "MyLib:R");

    // Library should have 2 symbols: MyLib:Existing + MyLib:R
    let lib = SymbolLibFile::read(&lib_path).expect("reread lib");
    assert_eq!(lib.ast().symbol_count, 2);
    let pushed = lib
        .ast()
        .symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("MyLib:R"))
        .expect("MyLib:R present in library");
    assert_eq!(pushed.pin_count, 1, "pin count preserved");

    // Schematic: R1 should now reference MyLib:R
    let sch = SchematicFile::read(&sch_path).expect("reread sch");
    let instance = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("R1"))
        .expect("R1 present");
    assert_eq!(instance.lib_id.as_deref(), Some("MyLib:R"));

    // lib_symbols should have MyLib:R and not Device:R (check raw file content)
    let sch_content = fs::read_to_string(&sch_path).expect("read sch content");
    assert!(
        sch_content.contains("\"MyLib:R\""),
        "MyLib:R in lib_symbols of schematic"
    );
    assert!(
        !sch_content.contains("\"Device:R\""),
        "Device:R removed from schematic"
    );

    let _ = fs::remove_dir_all(dir);
}

/// Push a symbol into a library that doesn't yet contain it — it should be appended.
#[test]
fn push_symbol_to_lib_appends_new() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u2\")\n",
        "  (lib_symbols\n",
        "    (symbol \"Device:C\"\n",
        "      (property \"Reference\" \"C\" (at 0 0 0))\n",
        "      (property \"Value\" \"C\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"Device:C\") (at 50 50 0) (unit 1) (uuid \"s2\")\n",
        "    (property \"Reference\" \"C1\" (at 0 0 0))\n",
        "    (property \"Value\" \"100nF\" (at 0 0 0)))\n",
        ")\n",
    );
    // Library starts with no symbols
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        ")\n",
    );

    let dir = tmp_dir("push_append");
    let sch_path = dir.join("top.kicad_sch");
    let lib_path = dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    push_symbol_to_lib(&sch_path, "C1", &lib_path).expect("push_symbol_to_lib");

    let lib = SymbolLibFile::read(&lib_path).expect("reread lib");
    assert_eq!(lib.ast().symbol_count, 1, "symbol appended to empty lib");
    assert!(
        lib.ast()
            .symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("MyLib:C")),
        "MyLib:C present"
    );

    let _ = fs::remove_dir_all(dir);
}

/// Push a symbol back to the library it already came from (same lib name in filename).
/// Only the library file should be updated; the schematic should be unchanged.
#[test]
fn push_symbol_to_lib_same_library() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u3\")\n",
        "  (lib_symbols\n",
        "    (symbol \"MyLib:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (property \"Value\" \"R_updated\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 100 50 0) (unit 1) (uuid \"s3\")\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\"\n",
        "    (property \"Reference\" \"R\" (at 0 0 0))\n",
        "    (property \"Value\" \"R_old\" (at 0 0 0)))\n",
        ")\n",
    );

    let dir = tmp_dir("push_same");
    let sch_path = dir.join("top.kicad_sch");
    let lib_path = dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let sch_before = fs::read_to_string(&sch_path).expect("read sch before");

    let result = push_symbol_to_lib(&sch_path, "R1", &lib_path).expect("push_symbol_to_lib");
    assert_eq!(result, "MyLib:R");

    // Library should be updated (Value changes from R_old to R_updated)
    let lib = SymbolLibFile::read(&lib_path).expect("reread lib");
    let sym = lib
        .ast()
        .symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("MyLib:R"))
        .expect("MyLib:R present");
    assert!(
        sym.properties
            .iter()
            .any(|(k, v)| k == "Value" && v == "R_updated"),
        "library Value updated"
    );

    // Schematic should be unchanged (not rewritten when lib name matches)
    let sch_after = fs::read_to_string(&sch_path).expect("read sch after");
    assert_eq!(
        sch_before, sch_after,
        "schematic not modified for same-library push"
    );

    let _ = fs::remove_dir_all(dir);
}

/// Error case: reference not found in schematic.
#[test]
fn push_symbol_to_lib_error_reference_not_found() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u4\")\n",
        "  (lib_symbols)\n",
        ")\n",
    );
    let lib_src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n)\n";

    let sch_path = tmp_file("push_err_ref_sch", "kicad_sch");
    let lib_path = tmp_file("MyLib", "kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let result = push_symbol_to_lib(&sch_path, "DOESNOTEXIST", &lib_path);
    assert!(
        matches!(result, Err(Error::Validation(_))),
        "expected Validation error for missing reference"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
}

/// Error case: instance exists but lib_symbols has no embedded definition.
#[test]
fn push_symbol_to_lib_error_no_embedded_definition() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u5\")\n",
        "  (lib_symbols)\n",
        "  (symbol (lib_id \"Device:R\") (at 100 50 0) (unit 1) (uuid \"s5\")\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n)\n";

    let sch_path = tmp_file("push_err_embed_sch", "kicad_sch");
    let lib_path = tmp_file("MyLib", "kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let result = push_symbol_to_lib(&sch_path, "R1", &lib_path);
    assert!(
        matches!(result, Err(Error::Validation(_))),
        "expected Validation error for missing embedded definition"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
}

#[test]
fn fork_symbol_to_lib_cross_library_with_new_name() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u11\")\n",
        "  (lib_symbols\n",
        "    (symbol \"Device:D\"\n",
        "      (property \"Reference\" \"D\" (at 0 0 0))\n",
        "      (property \"Value\" \"D\" (at 0 0 0))\n",
        "      (symbol \"Device:D_1_1\"\n",
        "        (pin passive line (at 0 0 0) (length 2.54)\n",
        "          (name \"K\" (effects (font (size 1.27 1.27))))\n",
        "          (number \"1\" (effects (font (size 1.27 1.27)))))))\n",
        "    )\n",
        "  (symbol (lib_id \"Device:D\") (at 100 50 0) (unit 1) (uuid \"s11\")\n",
        "    (property \"Reference\" \"D15\" (at 0 0 0))\n",
        "    (property \"Value\" \"BAV99\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n)\n";

    let dir = tmp_dir("fork_cross_named");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("Custom.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let lib_id = fork_symbol_to_lib(
        &sch_path,
        "D15",
        &lib_path,
        "FastDiode",
        ForkSymbolToLibOptions::default(),
    )
    .expect("fork symbol");
    assert_eq!(lib_id, "Custom:FastDiode");

    let lib = SymbolLibFile::read(&lib_path).expect("read lib");
    assert!(lib
        .ast()
        .symbols
        .iter()
        .any(|s| s.name.as_deref() == Some("Custom:FastDiode")));

    let raw_lib = fs::read_to_string(&lib_path).expect("lib text");
    assert!(raw_lib.contains("\"Custom:FastDiode_1_1\""));

    let sch = SchematicFile::read(&sch_path).expect("read sch");
    let d15 = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("D15"))
        .expect("D15");
    assert_eq!(d15.lib_id.as_deref(), Some("Custom:FastDiode"));

    let raw_sch = fs::read_to_string(&sch_path).expect("sch text");
    assert!(raw_sch.contains("\"Custom:FastDiode\""));
    assert!(!raw_sch.contains("\"Device:D\""));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fork_symbol_to_lib_renames_local_unit_prefixes() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u11b\")\n",
        "  (lib_symbols\n",
        "    (symbol \"User Library:LED\"\n",
        "      (property \"Reference\" \"D\" (at 0 0 0))\n",
        "      (property \"Value\" \"LED\" (at 0 0 0))\n",
        "      (symbol \"LED_0_1\" (polyline (pts (xy 0 0) (xy 1 0)) (stroke (width 0) (type default)) (fill (type none))))\n",
        "      (symbol \"LED_1_1\"\n",
        "        (pin passive line (at -3.81 0 0) (length 2.54)\n",
        "          (name \"K\" (effects (font (size 1.27 1.27))))\n",
        "          (number \"1\" (effects (font (size 1.27 1.27)))))))\n",
        "    )\n",
        "  (symbol (lib_id \"User Library:LED\") (at 100 50 0) (unit 1) (uuid \"s11b\")\n",
        "    (property \"Reference\" \"D15\" (at 0 0 0))\n",
        "    (property \"Value\" \"LED\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n)\n";

    let dir = tmp_dir("fork_local_prefix");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("Custom.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let lib_id = fork_symbol_to_lib(
        &sch_path,
        "D15",
        &lib_path,
        "L-934GD",
        ForkSymbolToLibOptions::default(),
    )
    .expect("fork symbol");
    assert_eq!(lib_id, "Custom:L-934GD");

    let raw_lib = fs::read_to_string(&lib_path).expect("lib text");
    assert!(raw_lib.contains("\"Custom:L-934GD\""));
    assert!(raw_lib.contains("\"L-934GD_0_1\""));
    assert!(raw_lib.contains("\"L-934GD_1_1\""));
    assert!(!raw_lib.contains("\"LED_0_1\""));
    assert!(!raw_lib.contains("\"LED_1_1\""));

    let raw_sch = fs::read_to_string(&sch_path).expect("sch text");
    assert!(raw_sch.contains("\"L-934GD_0_1\""));
    assert!(raw_sch.contains("\"L-934GD_1_1\""));
    assert!(!raw_sch.contains("\"LED_0_1\""));
    assert!(!raw_sch.contains("\"LED_1_1\""));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fork_symbol_to_lib_only_updates_target_reference() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u12\")\n",
        "  (lib_symbols\n",
        "    (symbol \"Device:D\"\n",
        "      (property \"Reference\" \"D\" (at 0 0 0))\n",
        "      (property \"Value\" \"D\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"Device:D\") (at 100 50 0) (unit 1) (uuid \"s12a\")\n",
        "    (property \"Reference\" \"D15\" (at 0 0 0))\n",
        "    (property \"Value\" \"BAV99\" (at 0 0 0)))\n",
        "  (symbol (lib_id \"Device:D\") (at 120 50 0) (unit 1) (uuid \"s12b\")\n",
        "    (property \"Reference\" \"D16\" (at 0 0 0))\n",
        "    (property \"Value\" \"BAV99\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n)\n";

    let dir = tmp_dir("fork_target_only");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("Custom.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    fork_symbol_to_lib(
        &sch_path,
        "D15",
        &lib_path,
        "FastDiode",
        ForkSymbolToLibOptions::default(),
    )
    .expect("fork symbol");

    let instances = SchematicFile::read(&sch_path)
        .expect("read sch")
        .symbol_instances();
    assert!(instances.iter().any(|s| {
        s.reference.as_deref() == Some("D15") && s.lib_id.as_deref() == Some("Custom:FastDiode")
    }));
    assert!(instances.iter().any(|s| {
        s.reference.as_deref() == Some("D16") && s.lib_id.as_deref() == Some("Device:D")
    }));

    let raw_sch = fs::read_to_string(&sch_path).expect("sch text");
    assert!(
        raw_sch.contains("\"Device:D\""),
        "old embedded symbol kept for D16"
    );
    assert!(raw_sch.contains("\"Custom:FastDiode\""));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fork_symbol_to_lib_collision_errors_without_override() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u13\")\n",
        "  (lib_symbols\n",
        "    (symbol \"Device:D\"\n",
        "      (property \"Reference\" \"D\" (at 0 0 0))\n",
        "      (property \"Value\" \"D\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"Device:D\") (at 100 50 0) (unit 1) (uuid \"s13\")\n",
        "    (property \"Reference\" \"D15\" (at 0 0 0))\n",
        "    (property \"Value\" \"BAV99\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"Custom:FastDiode\"\n",
        "    (property \"Reference\" \"D\" (at 0 0 0))\n",
        "    (property \"Value\" \"OLD\" (at 0 0 0)))\n",
        ")\n",
    );

    let dir = tmp_dir("fork_collision");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("Custom.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let err = fork_symbol_to_lib(
        &sch_path,
        "D15",
        &lib_path,
        "FastDiode",
        ForkSymbolToLibOptions::default(),
    )
    .expect_err("must fail");
    assert!(matches!(err, Error::Validation(_)));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fork_symbol_to_lib_collision_replaces_with_override() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u14\")\n",
        "  (lib_symbols\n",
        "    (symbol \"Device:D\"\n",
        "      (property \"Reference\" \"D\" (at 0 0 0))\n",
        "      (property \"Value\" \"NEW\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"Device:D\") (at 100 50 0) (unit 1) (uuid \"s14\")\n",
        "    (property \"Reference\" \"D15\" (at 0 0 0))\n",
        "    (property \"Value\" \"BAV99\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"Custom:FastDiode\"\n",
        "    (property \"Reference\" \"D\" (at 0 0 0))\n",
        "    (property \"Value\" \"OLD\" (at 0 0 0)))\n",
        ")\n",
    );

    let dir = tmp_dir("fork_override");
    let sch_path = dir.join("demo.kicad_sch");
    let lib_path = dir.join("Custom.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let lib_id = fork_symbol_to_lib(
        &sch_path,
        "D15",
        &lib_path,
        "FastDiode",
        ForkSymbolToLibOptions { overwrite: true },
    )
    .expect("override fork");
    assert_eq!(lib_id, "Custom:FastDiode");

    let raw_lib = fs::read_to_string(&lib_path).expect("lib text");
    assert!(raw_lib.contains("(property \"Value\" \"NEW\""));
    assert!(!raw_lib.contains("(property \"Value\" \"OLD\""));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn update_from_lib_replaces_embedded_symbol_only() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u6\")\n",
        "  (lib_symbols\n",
        "    (symbol \"MyLib:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (property \"Value\" \"R_old\" (at 0 0 0))\n",
        "      (property \"Datasheet\" \"old\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 100 50 0) (unit 1) (uuid \"s6\")\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"instance-local\" (at 0 0 0)))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 120 50 0) (unit 1) (uuid \"s6b\")\n",
        "    (property \"Reference\" \"R2\" (at 0 0 0))\n",
        "    (property \"Value\" \"22k\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"instance-r2\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\"\n",
        "    (property \"Reference\" \"R\" (at 0 0 0))\n",
        "    (property \"Value\" \"R_new\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"new\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_sch");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let report =
        update_symbols_from_lib(&sch_path, &lib_path, Some("R1"), false).expect("update from lib");
    assert_eq!(report.reference.as_deref(), Some("R1"));
    assert_eq!(report.updated_symbols, vec!["MyLib:R".to_string()]);
    assert!(report.skipped_missing_symbols.is_empty());

    let sch = SchematicFile::read(&sch_path).expect("reread sch");
    let instance = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("R1"))
        .expect("R1 present");
    assert_eq!(instance.value.as_deref(), Some("10k"));
    assert!(
        instance
            .properties
            .iter()
            .any(|(k, v)| k == "Datasheet" && v == "new"),
        "targeted placed instance should be rewritten from library"
    );
    let instance_r2 = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("R2"))
        .expect("R2 present");
    assert!(
        instance_r2
            .properties
            .iter()
            .any(|(k, v)| k == "Datasheet" && v == "instance-r2"),
        "non-targeted instance should stay untouched"
    );

    let raw = fs::read_to_string(&sch_path).expect("read sch");
    assert!(
        raw.contains("(property \"Value\" \"R_new\""),
        "embedded symbol refreshed"
    );
    assert!(
        raw.contains("(property \"Value\" \"10k\""),
        "instance value preserved"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}

#[test]
fn update_from_lib_can_override_instance_value() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u6v\")\n",
        "  (lib_symbols\n",
        "    (symbol \"MyLib:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (property \"Value\" \"R_old\" (at 0 0 0))\n",
        "      (property \"Datasheet\" \"old\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 100 50 0) (unit 1) (uuid \"s6v\")\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"instance-local\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\"\n",
        "    (property \"Reference\" \"R\" (at 0 0 0))\n",
        "    (property \"Value\" \"R_new\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"new\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_override_value");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let report = update_symbols_from_lib_with_options(
        &sch_path,
        &lib_path,
        Some("R1"),
        false,
        UpdateFromLibOptions {
            overwrite_value: true,
        },
    )
    .expect("update from lib");
    assert_eq!(report.reference.as_deref(), Some("R1"));
    assert_eq!(report.updated_symbols, vec!["MyLib:R".to_string()]);

    let sch = SchematicFile::read(&sch_path).expect("reread sch");
    let instance = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("R1"))
        .expect("R1 present");
    assert_eq!(instance.value.as_deref(), Some("R_new"));
    assert!(
        instance
            .properties
            .iter()
            .any(|(k, v)| k == "Datasheet" && v == "new"),
        "targeted placed instance should be rewritten from library"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}

fn update_from_lib_does_not_materialize_symbol_metadata_as_instance_properties() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u6m\")\n",
        "  (lib_symbols\n",
        "    (symbol \"MyLib:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (property \"Value\" \"R_old\" (at 0 0 0))\n",
        "      (property \"Description\" \"old\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 100 50 0) (unit 1) (uuid \"s6m\")\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0))\n",
        "    (property \"Description\" \"instance-local\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\"\n",
        "    (property \"Reference\" \"R\" (at 0 0 0))\n",
        "    (property \"Value\" \"R_new\" (at 0 0 0))\n",
        "    (property \"ki_keywords\" \"resistor\" (at 0 0 0))\n",
        "    (property \"ki_fp_filters\" \"R_*\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"new\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_metadata");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    update_symbols_from_lib(&sch_path, &lib_path, Some("R1"), false).expect("update from lib");

    let sch = SchematicFile::read(&sch_path).expect("reread sch");
    let instance = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some("R1"))
        .expect("R1 present");
    assert!(instance.properties.iter().all(|(k, _)| k != "ki_keywords"));
    assert!(instance
        .properties
        .iter()
        .all(|(k, _)| k != "ki_fp_filters"));
    assert!(
        instance
            .properties
            .iter()
            .any(|(k, v)| k == "Datasheet" && v == "new"),
        "general properties should still sync from library"
    );

    let raw = fs::read_to_string(&sch_path).expect("read sch");
    assert!(
        raw.contains("(property \"ki_keywords\" \"resistor\""),
        "embedded symbol should retain ki_keywords metadata"
    );
    assert!(
        raw.contains("(property \"ki_fp_filters\" \"R_*\""),
        "embedded symbol should retain ki_fp_filters metadata"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}

#[test]
fn update_from_lib_skips_missing_symbol_names() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u7\")\n",
        "  (lib_symbols\n",
        "    (symbol \"MyLib:R\" (property \"Reference\" \"R\" (at 0 0 0)))\n",
        "    (symbol \"MyLib:C\" (property \"Reference\" \"C\" (at 0 0 0))))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\" (property \"Reference\" \"R\" (at 0 0 0)) (property \"Datasheet\" \"r\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_skip_sch");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let report =
        update_symbols_from_lib(&sch_path, &lib_path, None, true).expect("update from lib");
    assert_eq!(report.updated_symbols, vec!["MyLib:R".to_string()]);
    assert_eq!(report.skipped_missing_symbols, vec!["MyLib:C".to_string()]);

    let raw = fs::read_to_string(&sch_path).expect("read sch");
    assert!(raw.contains("\"Datasheet\" \"r\""));
    assert!(
        raw.contains("(symbol \"MyLib:C\""),
        "missing symbol left untouched"
    );

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}

#[test]
fn update_from_lib_all_updates_all_matching_instances() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u10\")\n",
        "  (lib_symbols\n",
        "    (symbol \"MyLib:R\"\n",
        "      (property \"Reference\" \"R\" (at 0 0 0))\n",
        "      (property \"Datasheet\" \"old\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 10 10 0)\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"1k\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"local1\" (at 0 0 0)))\n",
        "  (symbol (lib_id \"MyLib:R\") (at 20 10 0)\n",
        "    (property \"Reference\" \"R2\" (at 0 0 0))\n",
        "    (property \"Value\" \"2k\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"local2\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\"\n",
        "    (property \"Reference\" \"R\" (at 0 0 0))\n",
        "    (property \"Datasheet\" \"new-all\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_all");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let report =
        update_symbols_from_lib(&sch_path, &lib_path, None, true).expect("update from lib");
    assert_eq!(report.updated_symbols, vec!["MyLib:R".to_string()]);

    let instances = SchematicFile::read(&sch_path)
        .expect("reread sch")
        .symbol_instances();
    assert!(instances.iter().any(|s| {
        s.reference.as_deref() == Some("R1")
            && s.properties
                .iter()
                .any(|(k, v)| k == "Datasheet" && v == "new-all")
    }));
    assert!(instances.iter().any(|s| {
        s.reference.as_deref() == Some("R2")
            && s.properties
                .iter()
                .any(|(k, v)| k == "Datasheet" && v == "new-all")
    }));

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}

#[test]
fn update_from_lib_errors_when_prefix_absent() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u8\")\n",
        "  (lib_symbols\n",
        "    (symbol \"OtherLib:R\" (property \"Reference\" \"R\" (at 0 0 0))))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\" (property \"Reference\" \"R\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_err_sch");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let err = update_symbols_from_lib(&sch_path, &lib_path, None, true).expect_err("must fail");
    assert!(matches!(err, Error::Validation(_)));

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}

#[test]
fn update_from_lib_single_reference_errors_for_wrong_library() {
    let sch_src = concat!(
        "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u9\")\n",
        "  (lib_symbols (symbol \"OtherLib:R\" (property \"Reference\" \"R\" (at 0 0 0))))\n",
        "  (symbol (lib_id \"OtherLib:R\") (at 10 10 0)\n",
        "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
        "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
        ")\n",
    );
    let lib_src = concat!(
        "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n",
        "  (symbol \"MyLib:R\" (property \"Reference\" \"R\" (at 0 0 0)))\n",
        ")\n",
    );

    let sch_dir = tmp_dir("update_from_lib_wrong_lib");
    let sch_path = sch_dir.join("demo.kicad_sch");
    let lib_path = sch_dir.join("MyLib.kicad_sym");
    fs::write(&sch_path, sch_src).expect("write sch");
    fs::write(&lib_path, lib_src).expect("write lib");

    let err =
        update_symbols_from_lib(&sch_path, &lib_path, Some("R1"), false).expect_err("must fail");
    assert!(matches!(err, Error::Validation(_)));

    let _ = fs::remove_file(sch_path);
    let _ = fs::remove_file(lib_path);
    let _ = fs::remove_dir(sch_dir);
}
