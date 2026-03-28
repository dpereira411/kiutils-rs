use crate::diagnostic::{Diagnostic, Severity};
use crate::lib_table::LibTableDocument;
use crate::project::ProjectDocument;
use crate::schematic::SchematicDocument;

/// Validate that a project's pinned libraries are consistent with its library tables,
/// and that neither table contains duplicate library names.
///
/// Returns a list of [`Diagnostic`] warnings. An empty vec means the project is
/// internally consistent.
///
/// # Checks
///
/// - Every name in `project.pinned_symbol_libs` exists in `sym_lib_table`
/// - Every name in `project.pinned_footprint_libs` exists in `fp_lib_table`
/// - No duplicate `name` entries in `fp_lib_table`
/// - No duplicate `name` entries in `sym_lib_table`
pub fn validate_project_libs(
    project: &ProjectDocument,
    fp_lib_table: &LibTableDocument,
    sym_lib_table: &LibTableDocument,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Collect lib table names (active only — disabled libs are intentionally excluded)
    let fp_names: Vec<&str> = fp_lib_table
        .ast()
        .libraries
        .iter()
        .filter(|l| !l.disabled)
        .filter_map(|l| l.name.as_deref())
        .collect();

    let sym_names: Vec<&str> = sym_lib_table
        .ast()
        .libraries
        .iter()
        .filter(|l| !l.disabled)
        .filter_map(|l| l.name.as_deref())
        .collect();

    // Pinned footprint libs missing from fp-lib-table
    for pinned in &project.ast().pinned_footprint_libs {
        if !fp_names.contains(&pinned.as_str()) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "pinned_fp_lib_missing",
                message: format!("pinned footprint lib '{pinned}' is not present in fp-lib-table"),
                span: None,
                hint: Some(
                    "add the library to fp-lib-table or remove it from pinned_footprint_libs"
                        .to_string(),
                ),
            });
        }
    }

    // Pinned symbol libs missing from sym-lib-table
    for pinned in &project.ast().pinned_symbol_libs {
        if !sym_names.contains(&pinned.as_str()) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "pinned_sym_lib_missing",
                message: format!("pinned symbol lib '{pinned}' is not present in sym-lib-table"),
                span: None,
                hint: Some(
                    "add the library to sym-lib-table or remove it from pinned_symbol_libs"
                        .to_string(),
                ),
            });
        }
    }

    // Duplicate names in fp-lib-table
    check_duplicate_lib_names(
        fp_lib_table
            .ast()
            .libraries
            .iter()
            .filter_map(|l| l.name.as_deref()),
        "fp_lib_table_duplicate_name",
        "fp-lib-table",
        &mut diagnostics,
    );

    // Duplicate names in sym-lib-table
    check_duplicate_lib_names(
        sym_lib_table
            .ast()
            .libraries
            .iter()
            .filter_map(|l| l.name.as_deref()),
        "sym_lib_table_duplicate_name",
        "sym-lib-table",
        &mut diagnostics,
    );

    diagnostics
}

/// Validate that every symbol instance in a schematic references a library name
/// that exists in the symbol library table.
///
/// `lib_id` values have the form `LibraryName:SymbolName`. Only the library
/// name prefix is checked here — symbol-level existence requires loading the
/// library file itself.
///
/// # Checks
///
/// - Every `lib_id` prefix in symbol instances resolves to a non-disabled entry
///   in `sym_lib_table`
pub fn validate_schematic_symbols(
    schematic: &SchematicDocument,
    sym_lib_table: &LibTableDocument,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let active_libs: std::collections::HashSet<&str> = sym_lib_table
        .ast()
        .libraries
        .iter()
        .filter(|l| !l.disabled)
        .filter_map(|l| l.name.as_deref())
        .collect();

    for symbol in schematic.symbol_instances() {
        let Some(lib_id) = &symbol.lib_id else {
            continue;
        };
        // lib_id format: "LibraryName:SymbolName" — take the prefix
        let lib_name = lib_id.split(':').next().unwrap_or(lib_id.as_str());
        if !active_libs.contains(lib_name) {
            let reference = symbol.reference.as_deref().unwrap_or("<unknown>");
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "symbol_lib_not_in_table",
                message: format!(
                    "symbol '{reference}' uses lib_id '{lib_id}' but library '{lib_name}' is not in sym-lib-table"
                ),
                span: None,
                hint: Some(format!(
                    "add '{lib_name}' to sym-lib-table or update the symbol's lib_id"
                )),
            });
        }
    }

    diagnostics
}

/// Validate that every placed symbol in a schematic has a matching embedded
/// definition in the schematic's `lib_symbols` cache.
pub fn validate_schematic_embedded_lib_symbols(schematic: &SchematicDocument) -> Vec<Diagnostic> {
    schematic
        .missing_embedded_lib_symbol_lib_ids()
        .into_iter()
        .map(|lib_id| Diagnostic {
            severity: Severity::Error,
            code: "schematic_missing_embedded_lib_symbol",
            message: format!(
                "schematic references lib_id '{lib_id}' but lib_symbols has no matching embedded symbol"
            ),
            span: None,
            hint: Some(
                "refresh or fork the symbol so the schematic cache is updated before writing/opening in KiCad"
                    .to_string(),
            ),
        })
        .collect()
}

fn check_duplicate_lib_names<'a>(
    names: impl Iterator<Item = &'a str>,
    code: &'static str,
    table_label: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen = std::collections::HashSet::new();
    for name in names {
        if !seen.insert(name) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code,
                message: format!("duplicate library name '{name}' in {table_label}"),
                span: None,
                hint: Some(format!(
                    "remove or rename one of the '{name}' entries in {table_label}"
                )),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::lib_table::{FpLibTableFile, SymLibTableFile};
    use crate::project::ProjectFile;
    use crate::schematic::SchematicFile;

    fn tmp(name: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{nanos}.{ext}"))
    }

    fn write_fp_lib_table(libs: &[(&str, bool)]) -> PathBuf {
        let path = tmp("fp_lib_table", "fp-lib-table");
        let entries: String = libs
            .iter()
            .map(|(name, disabled)| {
                let disabled_flag = if *disabled { " (disabled yes)" } else { "" };
                format!(
                    " (lib (name \"{name}\") (type \"KiCad\") (uri \"${{{name}}}.pretty\") (options \"\") (descr \"\"){disabled_flag})"
                )
            })
            .collect();
        fs::write(&path, format!("(fp_lib_table (version 7){entries})\n"))
            .expect("write fp-lib-table");
        path
    }

    fn write_sym_lib_table(libs: &[(&str, bool)]) -> PathBuf {
        let path = tmp("sym_lib_table", "sym-lib-table");
        let entries: String = libs
            .iter()
            .map(|(name, disabled)| {
                let disabled_flag = if *disabled { " (disabled yes)" } else { "" };
                format!(
                    " (lib (name \"{name}\") (type \"KiCad\") (uri \"${{{name}}}.kicad_sym\") (options \"\") (descr \"\"){disabled_flag})"
                )
            })
            .collect();
        fs::write(&path, format!("(sym_lib_table (version 7){entries})\n"))
            .expect("write sym-lib-table");
        path
    }

    fn write_project(pinned_sym: &[&str], pinned_fp: &[&str]) -> PathBuf {
        let path = tmp("project", "kicad_pro");
        let sym_arr: String = pinned_sym
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let fp_arr: String = pinned_fp
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            &path,
            format!(
                "{{\"meta\":{{\"version\":1}},\"libraries\":{{\"pinned_symbol_libs\":[{sym_arr}],\"pinned_footprint_libs\":[{fp_arr}]}}}}\n"
            ),
        )
        .expect("write project");
        path
    }

    #[test]
    fn clean_project_has_no_diagnostics() {
        let fp = write_fp_lib_table(&[("MyFootprints", false)]);
        let sym = write_sym_lib_table(&[("MySymbols", false)]);
        let pro = write_project(&["MySymbols"], &["MyFootprints"]);

        let project = ProjectFile::read(&pro).unwrap();
        let fp_table = FpLibTableFile::read(&fp).unwrap();
        let sym_table = SymLibTableFile::read(&sym).unwrap();

        let diags = validate_project_libs(&project, &fp_table, &sym_table);
        assert!(diags.is_empty(), "{diags:?}");

        let _ = fs::remove_file(fp);
        let _ = fs::remove_file(sym);
        let _ = fs::remove_file(pro);
    }

    #[test]
    fn detects_pinned_lib_missing_from_table() {
        let fp = write_fp_lib_table(&[("A", false)]);
        let sym = write_sym_lib_table(&[("B", false)]);
        let pro = write_project(&["Missing_Sym"], &["Missing_Fp"]);

        let project = ProjectFile::read(&pro).unwrap();
        let fp_table = FpLibTableFile::read(&fp).unwrap();
        let sym_table = SymLibTableFile::read(&sym).unwrap();

        let diags = validate_project_libs(&project, &fp_table, &sym_table);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().any(|d| d.code == "pinned_fp_lib_missing"));
        assert!(diags.iter().any(|d| d.code == "pinned_sym_lib_missing"));

        let _ = fs::remove_file(fp);
        let _ = fs::remove_file(sym);
        let _ = fs::remove_file(pro);
    }

    #[test]
    fn disabled_lib_counts_as_missing_for_pinned() {
        let fp = write_fp_lib_table(&[("A", true)]); // disabled
        let sym = write_sym_lib_table(&[]);
        let pro = write_project(&[], &["A"]);

        let project = ProjectFile::read(&pro).unwrap();
        let fp_table = FpLibTableFile::read(&fp).unwrap();
        let sym_table = SymLibTableFile::read(&sym).unwrap();

        let diags = validate_project_libs(&project, &fp_table, &sym_table);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "pinned_fp_lib_missing");

        let _ = fs::remove_file(fp);
        let _ = fs::remove_file(sym);
        let _ = fs::remove_file(pro);
    }

    #[test]
    fn detects_duplicate_lib_names() {
        let fp = write_fp_lib_table(&[("A", false), ("A", false)]);
        let sym = write_sym_lib_table(&[("B", false), ("B", false)]);
        let pro = write_project(&[], &[]);

        let project = ProjectFile::read(&pro).unwrap();
        let fp_table = FpLibTableFile::read(&fp).unwrap();
        let sym_table = SymLibTableFile::read(&sym).unwrap();

        let diags = validate_project_libs(&project, &fp_table, &sym_table);
        assert_eq!(diags.len(), 2);
        assert!(diags
            .iter()
            .any(|d| d.code == "fp_lib_table_duplicate_name"));
        assert!(diags
            .iter()
            .any(|d| d.code == "sym_lib_table_duplicate_name"));

        let _ = fs::remove_file(fp);
        let _ = fs::remove_file(sym);
        let _ = fs::remove_file(pro);
    }

    #[test]
    fn schematic_symbols_resolve_against_table() {
        let sym = write_sym_lib_table(&[("Device", false)]);
        let sch_path = tmp("sch_validate", "kicad_sch");
        fs::write(
            &sch_path,
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\") (paper \"A4\") \
             (symbol (lib_id \"Device:R\") (at 0 0 0) (unit 1) (uuid \"a\") \
               (property \"Reference\" \"R1\" (at 0 0 0)) \
               (property \"Value\" \"10k\" (at 0 0 0))))\n",
        )
        .expect("write sch");

        let sch = SchematicFile::read(&sch_path).unwrap();
        let sym_table = SymLibTableFile::read(&sym).unwrap();

        let diags = validate_schematic_symbols(&sch, &sym_table);
        assert!(diags.is_empty(), "{diags:?}");

        let _ = fs::remove_file(sch_path);
        let _ = fs::remove_file(sym);
    }

    #[test]
    fn schematic_symbols_unknown_lib_emits_warning() {
        let sym = write_sym_lib_table(&[("Device", false)]);
        let sch_path = tmp("sch_validate_missing", "kicad_sch");
        fs::write(
            &sch_path,
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\") (paper \"A4\") \
             (symbol (lib_id \"UnknownLib:R\") (at 0 0 0) (unit 1) (uuid \"b\") \
               (property \"Reference\" \"R2\" (at 0 0 0)) \
               (property \"Value\" \"1k\" (at 0 0 0))))\n",
        )
        .expect("write sch");

        let sch = SchematicFile::read(&sch_path).unwrap();
        let sym_table = SymLibTableFile::read(&sym).unwrap();

        let diags = validate_schematic_symbols(&sch, &sym_table);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "symbol_lib_not_in_table");
        assert!(diags[0].message.contains("UnknownLib"));

        let _ = fs::remove_file(sch_path);
        let _ = fs::remove_file(sym);
    }

    #[test]
    fn missing_embedded_lib_symbol_is_reported_as_error() {
        let sch_path = tmp("sch_missing_embedded", "kicad_sch");
        fs::write(
            &sch_path,
            concat!(
                "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\")\n",
                "  (lib_symbols (symbol \"Device:R\" (property \"Reference\" \"R\" (at 0 0 0))))\n",
                "  (symbol (lib_id \"OtherLib:R\") (at 10 10 0)\n",
                "    (property \"Reference\" \"R1\" (at 0 0 0))\n",
                "    (property \"Value\" \"10k\" (at 0 0 0)))\n",
                ")\n",
            ),
        )
        .expect("write sch");

        let sch = SchematicFile::read(&sch_path).unwrap();
        let diags = validate_schematic_embedded_lib_symbols(&sch);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].code, "schematic_missing_embedded_lib_symbol");
        assert!(diags[0].message.contains("OtherLib:R"));

        let _ = fs::remove_file(sch_path);
    }
}
