use std::path::{Path, PathBuf};

use kiutils_rs::{
    fork_symbol_to_lib, update_symbols_from_lib, update_symbols_from_lib_with_options,
    DesignRulesFile, FootprintFile, ForkSymbolToLibOptions, FpLibTableFile, PcbFile, ProjectFile,
    SchematicFile, SymLibTableFile, SymbolLibFile, UpdateFromLibOptions, UpdateFromLibReport,
    WorksheetFile, WriteMode,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("kiutils_kicad")
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn facade_reads_all_v1_document_types() {
    let pcb = PcbFile::read(fixture("sample.kicad_pcb")).expect("pcb parse");
    assert_eq!(pcb.ast().version, Some(20260101));

    let footprint = FootprintFile::read(fixture("sample.kicad_mod")).expect("footprint parse");
    assert_eq!(footprint.ast().version, Some(20260101));

    let fplib = FpLibTableFile::read(fixture("fp-lib-table")).expect("fplib parse");
    assert_eq!(fplib.ast().library_count, 1);

    let symlib = SymLibTableFile::read(fixture("sym-lib-table")).expect("symlib parse");
    assert_eq!(symlib.ast().library_count, 1);

    let dru = DesignRulesFile::read(fixture("sample.kicad_dru")).expect("dru parse");
    assert_eq!(dru.ast().rule_count, 1);

    let project = ProjectFile::read(fixture("sample.kicad_pro")).expect("project parse");
    assert!(project.ast().pinned_symbol_libs.is_empty());
    assert_eq!(project.ast().pinned_footprint_libs, vec!["A"]);
}

#[test]
fn facade_reads_schematic_symbol_lib_worksheet() {
    let sch = SchematicFile::read(fixture("sample.kicad_sch")).expect("schematic parse");
    assert_eq!(sch.ast().version, Some(20260101));
    assert_eq!(sch.ast().symbol_count, 1);
    assert!(!sch.symbol_instances().is_empty());
    assert!(!sch.sheet_filenames().is_empty() || sch.ast().sheet_count == 0);

    let sym = SymbolLibFile::read(fixture("sample.kicad_sym")).expect("symbol lib parse");
    assert_eq!(sym.ast().version, Some(20260101));
    assert_eq!(sym.ast().symbol_count, 1);

    let wks = WorksheetFile::read(fixture("sample.kicad_wks")).expect("worksheet parse");
    assert_eq!(wks.ast().version, Some(20260101));
}

#[test]
fn facade_exposes_schematic_symbol_instance_edit() {
    let mut sch = SchematicFile::read(fixture("sample.kicad_sch")).expect("schematic parse");
    sch.upsert_symbol_instance_property("R1", "MPN", "RC0603FR-0710KL");
    let instances = sch.symbol_instances();
    let r1 = instances
        .iter()
        .find(|s| s.reference.as_deref() == Some("R1"));
    assert!(r1.is_some());
    let props: std::collections::HashMap<_, _> = r1.unwrap().properties.iter().cloned().collect();
    assert_eq!(
        props.get("MPN").map(|s| s.as_str()),
        Some("RC0603FR-0710KL")
    );
}

#[test]
fn facade_exposes_symbol_lib_rename_and_property_edit() {
    let mut sym = SymbolLibFile::read(fixture("sample.kicad_sym")).expect("symbol lib parse");
    sym.upsert_symbol_property("R", "Datasheet", "https://example.com/r.pdf");
    assert_eq!(sym.ast().symbols[0].name.as_deref(), Some("R"));
}

#[test]
fn facade_exposes_write_mode() {
    assert_ne!(WriteMode::Lossless, WriteMode::Canonical);
}

#[test]
fn facade_exposes_update_from_lib_api() {
    let report = UpdateFromLibReport {
        library_prefix: "MyLib".to_string(),
        reference: Some("R1".to_string()),
        updated_symbols: vec!["MyLib:R".to_string()],
        skipped_missing_symbols: vec!["MyLib:C".to_string()],
    };
    let _ = report;
    let _options = UpdateFromLibOptions {
        overwrite_value: true,
    };
    let _ = update_symbols_from_lib::<&str, &str>;
    let _ = update_symbols_from_lib_with_options::<&str, &str>;
}

#[test]
fn facade_exposes_fork_symbol_to_lib_api() {
    let _options = ForkSymbolToLibOptions { overwrite: true };
    let _ = fork_symbol_to_lib::<&str, &str, &str, &str>;
}

#[test]
fn facade_exposes_project_setters_and_libtable_upsert() {
    let mut project = ProjectFile::read(fixture("sample.kicad_pro")).expect("project parse");
    project
        .set_pinned_symbol_libs(vec!["SYM_A"])
        .set_pinned_footprint_libs(vec!["FP_A"]);
    assert_eq!(project.ast().pinned_symbol_libs, vec!["SYM_A"]);
    assert_eq!(project.ast().pinned_footprint_libs, vec!["FP_A"]);

    let mut fplib = FpLibTableFile::read(fixture("fp-lib-table")).expect("fplib parse");
    fplib.upsert_library_uri("A", "${KIPRJMOD}/A.pretty");
    assert_eq!(
        fplib.ast().libraries[0].uri.as_deref(),
        Some("${KIPRJMOD}/A.pretty")
    );
}
