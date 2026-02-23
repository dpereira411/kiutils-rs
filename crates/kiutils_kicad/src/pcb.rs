use std::fs;
use std::path::Path;

use kiutils_sexpr::{parse_one, Atom, CstDocument, Node};

use crate::diagnostic::{Diagnostic, Severity};
use crate::version::VersionPolicy;
use crate::{Error, UnknownNode, WriteMode};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbAst {
    pub version: Option<i32>,
    pub generator: Option<String>,
    pub generator_version: Option<String>,
    pub has_general: bool,
    pub has_paper: bool,
    pub has_title_block: bool,
    pub has_setup: bool,
    pub has_embedded_fonts: bool,
    pub layer_count: usize,
    pub property_count: usize,
    pub net_count: usize,
    pub footprint_count: usize,
    pub graphic_count: usize,
    pub trace_segment_count: usize,
    pub trace_arc_count: usize,
    pub via_count: usize,
    pub zone_count: usize,
    pub dimension_count: usize,
    pub target_count: usize,
    pub group_count: usize,
    pub generated_count: usize,
    pub unknown_nodes: Vec<UnknownNode>,
}

#[derive(Debug, Clone)]
pub struct PcbDocument {
    ast: PcbAst,
    cst: CstDocument,
    diagnostics: Vec<Diagnostic>,
}

impl PcbDocument {
    pub fn ast(&self) -> &PcbAst {
        &self.ast
    }

    pub fn ast_mut(&mut self) -> &mut PcbAst {
        &mut self.ast
    }

    pub fn cst(&self) -> &CstDocument {
        &self.cst
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.write_mode(path, WriteMode::Lossless)
    }

    pub fn write_mode<P: AsRef<Path>>(&self, path: P, mode: WriteMode) -> Result<(), Error> {
        match mode {
            WriteMode::Lossless => fs::write(path, self.cst.to_lossless_string())?,
            WriteMode::Canonical => fs::write(path, self.cst.to_canonical_string())?,
        }
        Ok(())
    }
}

pub struct PcbFile;

impl PcbFile {
    pub fn read<P: AsRef<Path>>(path: P) -> Result<PcbDocument, Error> {
        let raw = fs::read_to_string(path)?;
        let cst = parse_one(&raw)?;
        ensure_head(&cst, "kicad_pcb")?;

        let ast = parse_ast(&cst);
        let diagnostics = validate_version(ast.version)?;

        Ok(PcbDocument {
            ast,
            cst,
            diagnostics,
        })
    }
}

fn ensure_head(cst: &CstDocument, expected: &str) -> Result<(), Error> {
    let head = cst
        .nodes
        .first()
        .and_then(|n| match n {
            Node::List { items, .. } => items.first(),
            _ => None,
        })
        .and_then(|n| match n {
            Node::Atom {
                atom: Atom::Symbol(s),
                ..
            } => Some(s.as_str()),
            _ => None,
        });

    match head {
        Some(h) if h == expected => Ok(()),
        Some(h) => Err(Error::Validation(format!(
            "expected root token `{expected}`, got `{h}`"
        ))),
        None => Err(Error::Validation("missing root token".to_string())),
    }
}

fn parse_ast(cst: &CstDocument) -> PcbAst {
    let mut version = None;
    let mut generator = None;
    let mut generator_version = None;
    let mut has_general = false;
    let mut has_paper = false;
    let mut has_title_block = false;
    let mut has_setup = false;
    let mut has_embedded_fonts = false;
    let mut layer_count = 0usize;
    let mut property_count = 0usize;
    let mut net_count = 0usize;
    let mut footprint_count = 0usize;
    let mut graphic_count = 0usize;
    let mut trace_segment_count = 0usize;
    let mut trace_arc_count = 0usize;
    let mut via_count = 0usize;
    let mut zone_count = 0usize;
    let mut dimension_count = 0usize;
    let mut target_count = 0usize;
    let mut group_count = 0usize;
    let mut generated_count = 0usize;
    let mut unknown_nodes = Vec::new();

    if let Some(Node::List { items, .. }) = cst.nodes.first() {
        for (idx, item) in items.iter().enumerate() {
            if idx == 0 {
                continue;
            }
            match head_of(item) {
                Some("version") => {
                    version = second_atom_string(item).and_then(|v| v.parse::<i32>().ok());
                }
                Some("generator") => {
                    generator = second_atom_string(item);
                }
                Some("generator_version") => {
                    generator_version = second_atom_string(item);
                }
                Some("general") => has_general = true,
                Some("paper") => has_paper = true,
                Some("title_block") => has_title_block = true,
                Some("layers") => {
                    if let Node::List { items: inner, .. } = item {
                        layer_count = inner.len().saturating_sub(1);
                    }
                }
                Some("setup") => has_setup = true,
                Some("embedded_fonts") => has_embedded_fonts = true,
                Some("property") => property_count += 1,
                Some("net") => net_count += 1,
                Some("footprint") => footprint_count += 1,
                Some("segment") => trace_segment_count += 1,
                Some("arc") => trace_arc_count += 1,
                Some("via") => via_count += 1,
                Some("zone") => zone_count += 1,
                Some("dimension") => dimension_count += 1,
                Some("target") => target_count += 1,
                Some("group") => group_count += 1,
                Some("generated") => generated_count += 1,
                Some(h) if h.starts_with("gr_") => graphic_count += 1,
                _ => {
                    if let Some(unknown) = UnknownNode::from_node(item) {
                        unknown_nodes.push(unknown);
                    }
                }
            }
        }
    }

    PcbAst {
        version,
        generator,
        generator_version,
        has_general,
        has_paper,
        has_title_block,
        has_setup,
        has_embedded_fonts,
        layer_count,
        property_count,
        net_count,
        footprint_count,
        graphic_count,
        trace_segment_count,
        trace_arc_count,
        via_count,
        zone_count,
        dimension_count,
        target_count,
        group_count,
        generated_count,
        unknown_nodes,
    }
}

fn head_of(node: &Node) -> Option<&str> {
    let Node::List { items, .. } = node else {
        return None;
    };
    let Some(Node::Atom {
        atom: Atom::Symbol(head),
        ..
    }) = items.first()
    else {
        return None;
    };
    Some(head.as_str())
}

fn second_atom_string(node: &Node) -> Option<String> {
    let Node::List { items, .. } = node else {
        return None;
    };
    match items.get(1) {
        Some(Node::Atom {
            atom: Atom::Symbol(v),
            ..
        }) => Some(v.clone()),
        Some(Node::Atom {
            atom: Atom::Quoted(v),
            ..
        }) => Some(v.clone()),
        _ => None,
    }
}

fn validate_version(version: Option<i32>) -> Result<Vec<Diagnostic>, Error> {
    let policy = VersionPolicy::default();
    let mut diagnostics = Vec::new();

    if let Some(v) = version {
        if policy.reject_older && !policy.accepts(v) {
            return Err(Error::Validation(format!(
                "unsupported KiCad version {v}; expected v9+ format"
            )));
        }

        if policy.is_future_for_target(v) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "future_format",
                message: format!(
                    "version {v} is newer than target {:?}; keeping lossless CST for compatibility",
                    policy.target
                ),
                span: None,
                hint: Some("consider newer parser coverage for this token set".to_string()),
            });
        }
    }

    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn tmp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{nanos}.kicad_pcb"))
    }

    #[test]
    fn read_parses_version_and_preserves_lossless() {
        let path = tmp_file("pcb_read_ok");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.ast().version, Some(20260101));
        assert_eq!(doc.ast().generator.as_deref(), Some("pcbnew"));
        assert!(doc.ast().unknown_nodes.is_empty());
        assert_eq!(doc.cst().to_lossless_string(), src);

        let out = tmp_file("pcb_write_ok");
        doc.write(&out).expect("write");
        let roundtrip = fs::read_to_string(&out).expect("read out");
        assert_eq!(roundtrip, src);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn read_fails_on_invalid_root() {
        let path = tmp_file("pcb_bad_root");
        fs::write(&path, "(a)(b)").expect("write fixture");

        let err = PcbFile::read(&path).expect_err("must fail");
        match err {
            Error::Parse(_) => {}
            other => panic!("unexpected error: {other}"),
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_rejects_old_version() {
        let path = tmp_file("pcb_old_version");
        fs::write(&path, "(kicad_pcb (version 20220101))\n").expect("write fixture");

        let err = PcbFile::read(&path).expect_err("must fail");
        match err {
            Error::Validation(msg) => assert!(msg.contains("v9+")),
            other => panic!("unexpected error: {other}"),
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_warns_on_future_version() {
        let path = tmp_file("pcb_future_version");
        fs::write(&path, "(kicad_pcb (version 20270101))\n").expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.diagnostics().len(), 1);
        assert_eq!(doc.diagnostics()[0].code, "future_format");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn canonical_write_normalizes_spacing() {
        let path = tmp_file("pcb_canon_src");
        fs::write(&path, "(kicad_pcb   (version 20260101)   )\n").expect("write fixture");
        let doc = PcbFile::read(&path).expect("read");

        let out = tmp_file("pcb_canon_out");
        doc.write_mode(&out, WriteMode::Canonical).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert_eq!(written, "(kicad_pcb (version 20260101))\n");

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn captures_unknown_nodes_and_preserves_roundtrip() {
        let path = tmp_file("pcb_unknown");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew) (mystery_token 1 2))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.ast().unknown_nodes.len(), 1);
        assert_eq!(doc.ast().unknown_nodes[0].head.as_deref(), Some("mystery_token"));

        let out = tmp_file("pcb_unknown_out");
        doc.write(&out).expect("write");
        let roundtrip = fs::read_to_string(&out).expect("read out");
        assert_eq!(roundtrip, src);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn parses_top_level_counts() {
        let path = tmp_file("pcb_counts");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew)\n  (layers (0 F.Cu signal) (31 B.Cu signal))\n  (setup)\n  (net 0 \"\")\n  (footprint \"R_0603\")\n  (gr_line (start 0 0) (end 1 1))\n  (segment (start 0 0) (end 1 1))\n  (via (at 0 0) (size 1) (drill 0.5) (layers F.Cu B.Cu))\n  (zone)\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.ast().layer_count, 2);
        assert_eq!(doc.ast().net_count, 1);
        assert_eq!(doc.ast().footprint_count, 1);
        assert_eq!(doc.ast().graphic_count, 1);
        assert_eq!(doc.ast().trace_segment_count, 1);
        assert_eq!(doc.ast().via_count, 1);
        assert_eq!(doc.ast().zone_count, 1);
        assert!(doc.ast().has_setup);
        assert!(doc.ast().unknown_nodes.is_empty());

        let _ = fs::remove_file(path);
    }
}
