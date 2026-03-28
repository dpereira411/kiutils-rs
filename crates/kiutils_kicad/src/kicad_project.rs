use std::path::{Path, PathBuf};

use crate::batch::load_schematic_tree;
use crate::lib_table::{FpLibTableDocument, FpLibTableFile, SymLibTableDocument, SymLibTableFile};
use crate::pcb::{PcbDocument, PcbFile};
use crate::project::{ProjectDocument, ProjectFile};
use crate::schematic::SchematicDocument;
use crate::Error;

/// A loaded KiCad project with all associated files resolved.
///
/// Open with [`KiCadProject::open`]. Missing or unparseable optional files are
/// recorded in [`load_errors`](KiCadProject::load_errors) rather than causing
/// a hard failure, so callers can decide how to handle partial projects.
///
/// # Example
///
/// ```rust,no_run
/// use kiutils_kicad::KiCadProject;
///
/// let project = KiCadProject::open("MyProject.kicad_pro")?;
/// if let Some(sch) = project.root_schematic() {
///     println!("symbols: {}", sch.ast().symbol_count);
/// }
/// for (path, err) in &project.load_errors {
///     eprintln!("warning: {} — {}", path.display(), err);
/// }
/// # Ok::<(), kiutils_kicad::Error>(())
/// ```
#[derive(Debug)]
pub struct KiCadProject {
    /// The parsed `.kicad_pro` file.
    pub project: ProjectDocument,
    /// All schematics in the project, root first, sub-sheets in BFS order.
    /// Empty if the root schematic file was not found.
    pub schematics: Vec<SchematicDocument>,
    /// All PCB documents found in the project directory.
    /// Empty if no `.kicad_pcb` file matching the project name was found.
    pub pcbs: Vec<PcbDocument>,
    /// The project-local footprint library table, if present.
    pub fp_lib_table: Option<FpLibTableDocument>,
    /// The project-local symbol library table, if present.
    pub sym_lib_table: Option<SymLibTableDocument>,
    /// Files that were found but failed to parse, or were expected but missing.
    pub load_errors: Vec<(PathBuf, Error)>,
}

impl KiCadProject {
    /// Open a KiCad project from a `.kicad_pro` file path.
    ///
    /// Resolves the root schematic, PCB, and library tables relative to the
    /// project directory. Returns `Err` only if the `.kicad_pro` itself cannot
    /// be read or parsed. All other failures are collected in `load_errors`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        let project = ProjectFile::read(path)?;

        let dir = path.parent().unwrap_or(Path::new("."));
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();

        let mut load_errors: Vec<(PathBuf, Error)> = Vec::new();

        // Root schematic: same stem as .kicad_pro
        let sch_path = dir.join(format!("{stem}.kicad_sch"));
        let schematics = if sch_path.exists() {
            load_schematic_tree(&sch_path)
                .into_iter()
                .filter_map(|result| match result {
                    Ok(doc) => Some(doc),
                    Err(e) => {
                        load_errors.push((sch_path.clone(), e));
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        // PCB: same stem as .kicad_pro
        let pcb_path = dir.join(format!("{stem}.kicad_pcb"));
        let pcbs = if pcb_path.exists() {
            match PcbFile::read(&pcb_path) {
                Ok(doc) => vec![doc],
                Err(e) => {
                    load_errors.push((pcb_path, e));
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Footprint library table
        let fp_lib_path = dir.join("fp-lib-table");
        let fp_lib_table = if fp_lib_path.exists() {
            match FpLibTableFile::read(&fp_lib_path) {
                Ok(doc) => Some(doc),
                Err(e) => {
                    load_errors.push((fp_lib_path, e));
                    None
                }
            }
        } else {
            None
        };

        // Symbol library table
        let sym_lib_path = dir.join("sym-lib-table");
        let sym_lib_table = if sym_lib_path.exists() {
            match SymLibTableFile::read(&sym_lib_path) {
                Ok(doc) => Some(doc),
                Err(e) => {
                    load_errors.push((sym_lib_path, e));
                    None
                }
            }
        } else {
            None
        };

        Ok(KiCadProject {
            project,
            schematics,
            pcbs,
            fp_lib_table,
            sym_lib_table,
            load_errors,
        })
    }

    /// The root schematic (first in BFS order), if loaded.
    pub fn root_schematic(&self) -> Option<&SchematicDocument> {
        self.schematics.first()
    }

    /// The primary PCB, if loaded.
    pub fn primary_pcb(&self) -> Option<&PcbDocument> {
        self.pcbs.first()
    }

    /// Returns `true` if all expected project files loaded without errors.
    pub fn is_clean(&self) -> bool {
        self.load_errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{name}_{nanos}"));
        fs::create_dir_all(&dir).expect("create tmp dir");
        dir
    }

    #[test]
    fn open_project_with_all_files() {
        let dir = tmp_dir("kicad_project_full");
        let pro_path = dir.join("MyProject.kicad_pro");

        fs::write(
            &pro_path,
            r#"{"meta": {"version": 1}, "libraries": {"pinned_symbol_libs": [], "pinned_footprint_libs": []}}"#,
        )
        .expect("write pro");

        fs::write(
            dir.join("MyProject.kicad_sch"),
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\") (paper \"A4\"))\n",
        )
        .expect("write sch");

        fs::write(
            dir.join("MyProject.kicad_pcb"),
            "(kicad_pcb (version 20260101) (generator pcbnew))\n",
        )
        .expect("write pcb");

        fs::write(
            dir.join("fp-lib-table"),
            "(fp_lib_table (version 7) (lib (name \"A\") (type \"KiCad\") (uri \"${KIPRJMOD}/A.pretty\") (options \"\") (descr \"\")))\n",
        )
        .expect("write fp-lib-table");

        fs::write(
            dir.join("sym-lib-table"),
            "(sym_lib_table (version 7) (lib (name \"B\") (type \"KiCad\") (uri \"${KIPRJMOD}/B.kicad_sym\") (options \"\") (descr \"\")))\n",
        )
        .expect("write sym-lib-table");

        let project = KiCadProject::open(&pro_path).expect("open project");

        assert!(project.is_clean(), "load_errors: {:?}", project.load_errors);
        assert_eq!(project.schematics.len(), 1);
        assert_eq!(project.pcbs.len(), 1);
        assert!(project.fp_lib_table.is_some());
        assert!(project.sym_lib_table.is_some());
        assert!(project.root_schematic().is_some());
        assert!(project.primary_pcb().is_some());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn open_project_missing_optional_files() {
        let dir = tmp_dir("kicad_project_minimal");
        let pro_path = dir.join("Minimal.kicad_pro");

        fs::write(&pro_path, r#"{"meta": {"version": 1}}"#).expect("write pro");

        let project = KiCadProject::open(&pro_path).expect("open project");

        assert!(project.is_clean());
        assert!(project.schematics.is_empty());
        assert!(project.pcbs.is_empty());
        assert!(project.fp_lib_table.is_none());
        assert!(project.sym_lib_table.is_none());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn open_project_missing_pro_is_hard_error() {
        let result = KiCadProject::open("/nonexistent/path/Missing.kicad_pro");
        assert!(result.is_err());
    }
}
