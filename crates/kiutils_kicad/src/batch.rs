use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::{
    Error, PcbDocument, PcbFile, SchematicDocument, SchematicFile, SymbolLibDocument, SymbolLibFile,
};

pub fn read_pcbs(paths: &[PathBuf]) -> Vec<Result<PcbDocument, Error>> {
    read_pcbs_impl(paths)
}

#[cfg(feature = "parallel")]
fn read_pcbs_impl(paths: &[PathBuf]) -> Vec<Result<PcbDocument, Error>> {
    use rayon::prelude::*;
    paths.par_iter().map(PcbFile::read).collect()
}

#[cfg(not(feature = "parallel"))]
fn read_pcbs_impl(paths: &[PathBuf]) -> Vec<Result<PcbDocument, Error>> {
    paths.iter().map(PcbFile::read).collect()
}

pub fn read_pcbs_from_refs<P: AsRef<Path>>(paths: &[P]) -> Vec<Result<PcbDocument, Error>> {
    let owned = paths
        .iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    read_pcbs(&owned)
}

pub fn read_schematics(paths: &[PathBuf]) -> Vec<Result<SchematicDocument, Error>> {
    read_schematics_impl(paths)
}

#[cfg(feature = "parallel")]
fn read_schematics_impl(paths: &[PathBuf]) -> Vec<Result<SchematicDocument, Error>> {
    use rayon::prelude::*;
    paths.par_iter().map(SchematicFile::read).collect()
}

#[cfg(not(feature = "parallel"))]
fn read_schematics_impl(paths: &[PathBuf]) -> Vec<Result<SchematicDocument, Error>> {
    paths.iter().map(SchematicFile::read).collect()
}

pub fn read_schematics_from_refs<P: AsRef<Path>>(
    paths: &[P],
) -> Vec<Result<SchematicDocument, Error>> {
    let owned = paths
        .iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    read_schematics(&owned)
}

pub fn read_symbol_libs(paths: &[PathBuf]) -> Vec<Result<SymbolLibDocument, Error>> {
    read_symbol_libs_impl(paths)
}

#[cfg(feature = "parallel")]
fn read_symbol_libs_impl(paths: &[PathBuf]) -> Vec<Result<SymbolLibDocument, Error>> {
    use rayon::prelude::*;
    paths.par_iter().map(SymbolLibFile::read).collect()
}

#[cfg(not(feature = "parallel"))]
fn read_symbol_libs_impl(paths: &[PathBuf]) -> Vec<Result<SymbolLibDocument, Error>> {
    paths.iter().map(SymbolLibFile::read).collect()
}

pub fn read_symbol_libs_from_refs<P: AsRef<Path>>(
    paths: &[P],
) -> Vec<Result<SymbolLibDocument, Error>> {
    let owned = paths
        .iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    read_symbol_libs(&owned)
}

/// Load a schematic and all sub-sheets it references, recursively.
///
/// Returns sheets in BFS order (root first). Cycles are detected by canonical
/// path and skipped. Files that fail to parse are returned as `Err` entries at
/// the position they were encountered in the traversal.
pub fn load_schematic_tree(root: &Path) -> Vec<Result<SchematicDocument, Error>> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    let mut results: Vec<Result<SchematicDocument, Error>> = Vec::new();

    queue.push_back(root.to_path_buf());

    while let Some(path) = queue.pop_front() {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !visited.insert(canonical) {
            continue;
        }

        match SchematicFile::read(&path) {
            Ok(doc) => {
                let base_dir = path.parent().unwrap_or(Path::new("."));
                for filename in doc.sheet_filenames() {
                    queue.push_back(base_dir.join(&filename));
                }
                results.push(Ok(doc));
            }
            Err(e) => {
                results.push(Err(e));
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn tmp_file(name: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{nanos}.{ext}"))
    }

    #[test]
    fn batch_reads_multiple_pcbs() {
        let p1 = tmp_file("batch_pcb1", "kicad_pcb");
        let p2 = tmp_file("batch_pcb2", "kicad_pcb");
        fs::write(&p1, "(kicad_pcb (version 20260101))\n").expect("write p1");
        fs::write(&p2, "(kicad_pcb (version 20260101))\n").expect("write p2");

        let results = read_pcbs(&[p1.clone(), p2.clone()]);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));

        let _ = fs::remove_file(p1);
        let _ = fs::remove_file(p2);
    }

    #[test]
    fn batch_reads_multiple_schematics() {
        let p1 = tmp_file("batch_sch1", "kicad_sch");
        let p2 = tmp_file("batch_sch2", "kicad_sch");
        fs::write(
            &p1,
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\"))\n",
        )
        .expect("write p1");
        fs::write(
            &p2,
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u2\"))\n",
        )
        .expect("write p2");

        let results = read_schematics(&[p1.clone(), p2.clone()]);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(results[0].as_ref().unwrap().ast().version, Some(20260101));

        let _ = fs::remove_file(p1);
        let _ = fs::remove_file(p2);
    }

    #[test]
    fn batch_reads_multiple_symbol_libs() {
        let p1 = tmp_file("batch_sym1", "kicad_sym");
        let p2 = tmp_file("batch_sym2", "kicad_sym");
        fs::write(
            &p1,
            "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor))\n",
        )
        .expect("write p1");
        fs::write(
            &p2,
            "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor))\n",
        )
        .expect("write p2");

        let results = read_symbol_libs(&[p1.clone(), p2.clone()]);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(results[0].as_ref().unwrap().ast().symbol_count, 0);

        let _ = fs::remove_file(p1);
        let _ = fs::remove_file(p2);
    }

    #[test]
    fn batch_schematics_propagates_errors() {
        let p1 = tmp_file("batch_sch_ok", "kicad_sch");
        let p2 = tmp_file("batch_sch_bad", "kicad_sch");
        fs::write(
            &p1,
            "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\"))\n",
        )
        .expect("write p1");
        fs::write(&p2, "((not valid s-expression").expect("write bad");

        let results = read_schematics(&[p1.clone(), p2.clone()]);
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());

        let _ = fs::remove_file(p1);
        let _ = fs::remove_file(p2);
    }
}
