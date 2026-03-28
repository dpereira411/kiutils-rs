use std::fs;
use std::path::Path;

use kiutils_sexpr::{parse_one, Atom, CstDocument, Node};

use crate::diagnostic::Diagnostic;
use crate::sections::{parse_paper, parse_title_block, ParsedPaper, ParsedTitleBlock};
use crate::sexpr_edit::{
    atom_quoted, atom_symbol, ensure_root_head_any, find_property_index, list_node,
    mutate_root_and_refresh, paper_standard_node, paper_user_node,
    remove_property as remove_property_node, upsert_node, upsert_property_preserve_tail,
    upsert_scalar, upsert_section_child_scalar,
};
use crate::sexpr_utils::{
    atom_as_f64, atom_as_i32, atom_as_string, head_of, second_atom_bool, second_atom_f64,
    second_atom_i32, second_atom_string,
};
use crate::version_diag::collect_version_diagnostics;
use crate::{Error, UnknownNode, VersionPolicy, WriteMode};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbLayer {
    pub ordinal: Option<i32>,
    pub name: Option<String>,
    pub layer_type: Option<String>,
    pub user_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbNet {
    pub code: Option<i32>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbFootprintSummary {
    pub lib_id: Option<String>,
    pub layer: Option<String>,
    pub at: Option<[f64; 2]>,
    pub rotation: Option<f64>,
    pub uuid: Option<String>,
    pub property_count: usize,
    pub pad_count: usize,
    pub model_count: usize,
    pub zone_count: usize,
    pub group_count: usize,
    pub graphic_count: usize,
    pub fp_line_count: usize,
    pub fp_rect_count: usize,
    pub fp_circle_count: usize,
    pub fp_arc_count: usize,
    pub fp_poly_count: usize,
    pub fp_curve_count: usize,
    pub fp_text_count: usize,
    pub fp_text_box_count: usize,
    pub reference: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbSegmentSummary {
    pub start: Option<[f64; 2]>,
    pub end: Option<[f64; 2]>,
    pub width: Option<f64>,
    pub layer: Option<String>,
    pub net: Option<i32>,
    pub uuid: Option<String>,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbArcSummary {
    pub start: Option<[f64; 2]>,
    pub mid: Option<[f64; 2]>,
    pub end: Option<[f64; 2]>,
    pub width: Option<f64>,
    pub layer: Option<String>,
    pub net: Option<i32>,
    pub uuid: Option<String>,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbViaSummary {
    pub at: Option<[f64; 2]>,
    pub size: Option<f64>,
    pub drill: Option<f64>,
    pub drill_x: Option<f64>,
    pub drill_y: Option<f64>,
    pub drill_shape: Option<String>,
    pub net: Option<i32>,
    pub via_type: Option<String>,
    pub layers: Vec<String>,
    pub uuid: Option<String>,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbZoneSummary {
    pub net: Option<i32>,
    pub net_name: Option<String>,
    pub name: Option<String>,
    pub layer: Option<String>,
    pub layers: Vec<String>,
    pub hatch: Option<String>,
    pub fill_enabled: Option<bool>,
    pub polygon_count: usize,
    pub filled_polygon_count: usize,
    pub has_keepout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbGeneratedSummary {
    pub uuid: Option<String>,
    pub generated_type: Option<String>,
    pub name: Option<String>,
    pub layer: Option<String>,
    pub last_netname: Option<String>,
    pub members_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbDimensionSummary {
    pub dimension_type: Option<String>,
    pub layer: Option<String>,
    pub gr_text_count: usize,
    pub format_present: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbTargetSummary {
    pub shape: Option<String>,
    pub at: Option<[f64; 2]>,
    pub size: Option<f64>,
    pub width: Option<f64>,
    pub layer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbGroupSummary {
    pub name: Option<String>,
    pub group_id: Option<String>,
    pub member_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbGraphicSummary {
    pub token: String,
    pub layer: Option<String>,
    pub text: Option<String>,
    pub start: Option<[f64; 2]>,
    pub end: Option<[f64; 2]>,
    pub center: Option<[f64; 2]>,
    pub uuid: Option<String>,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbProperty {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbSetupSummary {
    pub has_stackup: bool,
    pub stackup_layer_count: usize,
    pub has_plot_settings: bool,
    pub pad_to_mask_clearance: Option<f64>,
    pub solder_mask_min_width: Option<f64>,
    pub aux_axis_origin: Option<[f64; 2]>,
    pub grid_origin: Option<[f64; 2]>,
    pub setup_tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbGeneralSummary {
    pub thickness: Option<f64>,
    pub legacy_teardrops: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbPaperSummary {
    pub kind: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub orientation: Option<String>,
}

impl From<ParsedPaper> for PcbPaperSummary {
    fn from(value: ParsedPaper) -> Self {
        Self {
            kind: value.kind,
            width: value.width,
            height: value.height,
            orientation: value.orientation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbTitleBlockSummary {
    pub title: Option<String>,
    pub date: Option<String>,
    pub revision: Option<String>,
    pub company: Option<String>,
    pub comments: Vec<String>,
}

impl From<ParsedTitleBlock> for PcbTitleBlockSummary {
    fn from(value: ParsedTitleBlock) -> Self {
        Self {
            title: value.title,
            date: value.date,
            revision: value.revision,
            company: value.company,
            comments: value.comments,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PcbAst {
    pub version: Option<i32>,
    pub generator: Option<String>,
    pub generator_version: Option<String>,
    pub has_general: bool,
    pub has_paper: bool,
    pub has_title_block: bool,
    pub general: Option<PcbGeneralSummary>,
    pub paper: Option<PcbPaperSummary>,
    pub title_block: Option<PcbTitleBlockSummary>,
    pub has_setup: bool,
    pub setup: Option<PcbSetupSummary>,
    pub has_embedded_fonts: bool,
    pub has_embedded_files: bool,
    pub embedded_file_count: usize,
    pub properties: Vec<PcbProperty>,
    pub layers: Vec<PcbLayer>,
    pub nets: Vec<PcbNet>,
    pub footprints: Vec<PcbFootprintSummary>,
    pub segments: Vec<PcbSegmentSummary>,
    pub arcs: Vec<PcbArcSummary>,
    pub vias: Vec<PcbViaSummary>,
    pub zones: Vec<PcbZoneSummary>,
    pub generated_items: Vec<PcbGeneratedSummary>,
    pub dimensions: Vec<PcbDimensionSummary>,
    pub targets: Vec<PcbTargetSummary>,
    pub groups: Vec<PcbGroupSummary>,
    pub graphics: Vec<PcbGraphicSummary>,
    pub layer_count: usize,
    pub property_count: usize,
    pub net_count: usize,
    pub footprint_count: usize,
    pub graphic_count: usize,
    pub gr_line_count: usize,
    pub gr_rect_count: usize,
    pub gr_circle_count: usize,
    pub gr_arc_count: usize,
    pub gr_poly_count: usize,
    pub gr_curve_count: usize,
    pub gr_text_count: usize,
    pub gr_text_box_count: usize,
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
    ast_dirty: bool,
    policy: VersionPolicy,
}

impl PcbDocument {
    pub fn ast(&self) -> &PcbAst {
        &self.ast
    }

    pub fn ast_mut(&mut self) -> &mut PcbAst {
        self.ast_dirty = true;
        &mut self.ast
    }

    pub fn set_version(&mut self, version: i32) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_scalar(items, "version", atom_symbol(version.to_string()), 1)
        })
    }

    pub fn set_generator<S: Into<String>>(&mut self, generator: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_scalar(items, "generator", atom_symbol(generator.into()), 1)
        })
    }

    pub fn set_generator_version<S: Into<String>>(&mut self, generator_version: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_scalar(
                items,
                "generator_version",
                atom_quoted(generator_version.into()),
                1,
            )
        })
    }

    pub fn set_paper_standard<S: Into<String>>(
        &mut self,
        kind: S,
        orientation: Option<&str>,
    ) -> &mut Self {
        let node = paper_standard_node(kind.into(), orientation.map(|v| v.to_string()));
        self.mutate_root_items(|items| upsert_node(items, "paper", node, 1))
    }

    pub fn set_paper_user(
        &mut self,
        width: f64,
        height: f64,
        orientation: Option<&str>,
    ) -> &mut Self {
        let node = paper_user_node(width, height, orientation.map(|v| v.to_string()));
        self.mutate_root_items(|items| upsert_node(items, "paper", node, 1))
    }

    pub fn set_title<S: Into<String>>(&mut self, title: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_section_child_scalar(items, "title_block", 1, "title", atom_quoted(title.into()))
        })
    }

    pub fn set_date<S: Into<String>>(&mut self, date: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_section_child_scalar(items, "title_block", 1, "date", atom_quoted(date.into()))
        })
    }

    pub fn set_revision<S: Into<String>>(&mut self, revision: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_section_child_scalar(
                items,
                "title_block",
                1,
                "rev",
                atom_quoted(revision.into()),
            )
        })
    }

    pub fn set_company<S: Into<String>>(&mut self, company: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_section_child_scalar(
                items,
                "title_block",
                1,
                "company",
                atom_quoted(company.into()),
            )
        })
    }

    pub fn upsert_property<K: Into<String>, V: Into<String>>(
        &mut self,
        key: K,
        value: V,
    ) -> &mut Self {
        let key = key.into();
        let value = value.into();
        self.mutate_root_items(|items| upsert_property_preserve_tail(items, &key, &value, 1))
    }

    pub fn remove_property(&mut self, key: &str) -> &mut Self {
        let key = key.to_string();
        self.mutate_root_items(|items| remove_property_node(items, &key, 1))
    }

    /// Append a copper trace segment.
    ///
    /// `layer` is a KiCad layer name like `"F.Cu"` or `"B.Cu"`.
    /// `net` is the integer net number (0 = unconnected).
    pub fn add_trace<L: Into<String>>(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
        layer: L,
        net: i32,
    ) -> &mut Self {
        let node = pcb_segment_node(x1, y1, x2, y2, width, &layer.into(), net);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Remove all trace segments whose start/end coordinates exactly match the given values.
    pub fn remove_trace_at(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> &mut Self {
        self.mutate_root_items(|items| {
            let before = items.len();
            items.retain(|node| !segment_pts_match(node, x1, y1, x2, y2));
            items.len() != before
        })
    }

    /// Append a through-hole via between `"F.Cu"` and `"B.Cu"`.
    ///
    /// `size` is the via pad diameter; `drill` is the drill hole diameter (both in mm).
    /// `net` is the integer net number.
    pub fn add_via(&mut self, x: f64, y: f64, size: f64, drill: f64, net: i32) -> &mut Self {
        let node = pcb_via_node(x, y, size, drill, net);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Append a minimal footprint placeholder.
    ///
    /// Creates a footprint with Reference and Value properties and no pads.
    /// `lib_ref` is the library reference like `"Resistor_SMD:R_0603"`.
    /// `layer` is `"F.Cu"` or `"B.Cu"`.
    pub fn add_footprint<S: Into<String>>(
        &mut self,
        lib_ref: S,
        x: f64,
        y: f64,
        layer: S,
        reference: S,
        value: S,
    ) -> &mut Self {
        let node = pcb_footprint_node(
            &lib_ref.into(),
            x,
            y,
            &layer.into(),
            &reference.into(),
            &value.into(),
        );
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Move a footprint to a new position, preserving all its internal structure.
    ///
    /// Updates the `(at x y [rotation])` node of the first footprint whose
    /// `Reference` property matches `reference`.  If `rotation` is `Some`, the
    /// rotation is set to that value; if `None`, any existing rotation is left
    /// unchanged.  Returns `self` unchanged when no matching footprint is found.
    pub fn move_footprint<R: Into<String>>(
        &mut self,
        reference: R,
        x: f64,
        y: f64,
        rotation: Option<f64>,
    ) -> &mut Self {
        let reference = reference.into();
        self.mutate_root_items(|items| {
            for node in items.iter_mut() {
                if !footprint_has_reference(node, &reference) {
                    continue;
                }
                let Node::List {
                    items: fp_items, ..
                } = node
                else {
                    continue;
                };
                for child in fp_items.iter_mut() {
                    if head_of(child) != Some("at") {
                        continue;
                    }
                    let Node::List {
                        items: at_items, ..
                    } = child
                    else {
                        continue;
                    };
                    if let Some(xi) = at_items.get_mut(1) {
                        *xi = atom_symbol(pcb_fmt(x));
                    }
                    if let Some(yi) = at_items.get_mut(2) {
                        *yi = atom_symbol(pcb_fmt(y));
                    }
                    if let Some(rot) = rotation {
                        if at_items.len() >= 4 {
                            at_items[3] = atom_symbol(pcb_fmt(rot));
                        } else {
                            at_items.push(atom_symbol(pcb_fmt(rot)));
                        }
                    }
                    return true;
                }
            }
            false
        })
    }

    /// Remove all footprints whose `Reference` property matches `reference`.
    pub fn remove_footprint<R: Into<String>>(&mut self, reference: R) -> &mut Self {
        let reference = reference.into();
        self.mutate_root_items(|items| {
            let before = items.len();
            items.retain(|node| !footprint_has_reference(node, &reference));
            items.len() != before
        })
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
        if self.ast_dirty {
            return Err(Error::Validation(
                "ast_mut changes are not serializable; use document setter APIs".to_string(),
            ));
        }
        match mode {
            WriteMode::Lossless => fs::write(path, self.cst.to_lossless_string())?,
            WriteMode::Canonical => fs::write(path, self.cst.to_canonical_string())?,
        }
        Ok(())
    }

    fn mutate_root_items<F>(&mut self, mutate: F) -> &mut Self
    where
        F: FnOnce(&mut Vec<Node>) -> bool,
    {
        let policy = self.policy;
        mutate_root_and_refresh(
            &mut self.cst,
            &mut self.ast,
            &mut self.diagnostics,
            mutate,
            parse_ast,
            move |_cst, ast| collect_diagnostics(ast.version, &policy),
        );
        self.ast_dirty = false;
        self
    }
}

pub struct PcbFile;

impl PcbFile {
    pub fn read<P: AsRef<Path>>(path: P) -> Result<PcbDocument, Error> {
        Self::read_with_policy(path, VersionPolicy::default())
    }

    pub fn read_with_policy<P: AsRef<Path>>(
        path: P,
        policy: VersionPolicy,
    ) -> Result<PcbDocument, Error> {
        let raw = fs::read_to_string(path)?;
        let cst = parse_one(&raw)?;
        ensure_root_head_any(&cst, &["kicad_pcb"])?;

        let ast = parse_ast(&cst);
        let diagnostics = collect_diagnostics(ast.version, &policy);

        Ok(PcbDocument {
            ast,
            cst,
            diagnostics,
            ast_dirty: false,
            policy,
        })
    }
}

fn collect_diagnostics(version: Option<i32>, policy: &VersionPolicy) -> Vec<Diagnostic> {
    collect_version_diagnostics(version, policy)
}

fn parse_ast(cst: &CstDocument) -> PcbAst {
    let mut version = None;
    let mut generator = None;
    let mut generator_version = None;
    let mut has_general = false;
    let mut has_paper = false;
    let mut has_title_block = false;
    let mut general = None;
    let mut paper = None;
    let mut title_block = None;
    let mut has_setup = false;
    let mut setup = None;
    let mut has_embedded_fonts = false;
    let mut has_embedded_files = false;
    let mut embedded_file_count = 0usize;
    let mut properties = Vec::new();
    let mut layers = Vec::new();
    let mut nets = Vec::new();
    let mut footprints = Vec::new();
    let mut segments = Vec::new();
    let mut arcs = Vec::new();
    let mut vias = Vec::new();
    let mut zones = Vec::new();
    let mut generated_items = Vec::new();
    let mut dimensions = Vec::new();
    let mut targets = Vec::new();
    let mut groups = Vec::new();
    let mut graphics = Vec::new();
    let mut layer_count = 0usize;
    let mut property_count = 0usize;
    let mut net_count = 0usize;
    let mut footprint_count = 0usize;
    let mut graphic_count = 0usize;
    let mut gr_line_count = 0usize;
    let mut gr_rect_count = 0usize;
    let mut gr_circle_count = 0usize;
    let mut gr_arc_count = 0usize;
    let mut gr_poly_count = 0usize;
    let mut gr_curve_count = 0usize;
    let mut gr_text_count = 0usize;
    let mut gr_text_box_count = 0usize;
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
                Some("general") => {
                    has_general = true;
                    general = Some(parse_general_summary(item));
                    collect_unrecognized_children(
                        item,
                        &["thickness", "legacy_teardrops"],
                        &mut unknown_nodes,
                    );
                }
                Some("paper") => {
                    has_paper = true;
                    paper = Some(parse_paper(item).into());
                }
                Some("title_block") => {
                    has_title_block = true;
                    title_block = Some(parse_title_block(item).into());
                    collect_unrecognized_children(
                        item,
                        &["title", "date", "rev", "company", "comment"],
                        &mut unknown_nodes,
                    );
                }
                Some("layers") => {
                    if let Node::List { items: inner, .. } = item {
                        layers = parse_layers(inner);
                        layer_count = layers.len();
                    }
                }
                Some("setup") => {
                    has_setup = true;
                    setup = Some(parse_setup_summary(item));
                    collect_unrecognized_setup_children(item, &mut unknown_nodes);
                }
                Some("embedded_fonts") => has_embedded_fonts = true,
                Some("embedded_files") => {
                    has_embedded_files = true;
                    if let Node::List { items: inner, .. } = item {
                        embedded_file_count = inner
                            .iter()
                            .filter(|n| matches!(head_of(n), Some("file")))
                            .count();
                    }
                }
                Some("property") => {
                    property_count += 1;
                    if let Some(p) = parse_property(item) {
                        properties.push(p);
                    }
                }
                Some("net") => {
                    net_count += 1;
                    nets.push(parse_net(item));
                }
                Some("footprint") => {
                    footprint_count += 1;
                    footprints.push(parse_footprint_summary(item));
                }
                Some("segment") => {
                    trace_segment_count += 1;
                    segments.push(parse_segment_summary(item));
                }
                Some("arc") => {
                    trace_arc_count += 1;
                    arcs.push(parse_arc_summary(item));
                }
                Some("via") => {
                    via_count += 1;
                    vias.push(parse_via_summary(item));
                }
                Some("zone") => {
                    zone_count += 1;
                    zones.push(parse_zone_summary(item));
                }
                Some("dimension") => {
                    dimension_count += 1;
                    dimensions.push(parse_dimension_summary(item));
                }
                Some("target") => {
                    target_count += 1;
                    targets.push(parse_target_summary(item));
                }
                Some("group") => {
                    group_count += 1;
                    groups.push(parse_group_summary(item));
                }
                Some("generated") => {
                    generated_count += 1;
                    generated_items.push(parse_generated_summary(item));
                }
                Some("gr_line") => {
                    graphic_count += 1;
                    gr_line_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_line"));
                }
                Some("gr_rect") => {
                    graphic_count += 1;
                    gr_rect_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_rect"));
                }
                Some("gr_circle") => {
                    graphic_count += 1;
                    gr_circle_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_circle"));
                }
                Some("gr_arc") => {
                    graphic_count += 1;
                    gr_arc_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_arc"));
                }
                Some("gr_poly") => {
                    graphic_count += 1;
                    gr_poly_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_poly"));
                }
                Some("gr_curve") => {
                    graphic_count += 1;
                    gr_curve_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_curve"));
                }
                Some("gr_text") => {
                    graphic_count += 1;
                    gr_text_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_text"));
                }
                Some("gr_text_box") => {
                    graphic_count += 1;
                    gr_text_box_count += 1;
                    graphics.push(parse_graphic_summary(item, "gr_text_box"));
                }
                Some(h) if h.starts_with("gr_") => {
                    graphic_count += 1;
                    graphics.push(parse_graphic_summary(item, h));
                }
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
        general,
        paper,
        title_block,
        has_setup,
        setup,
        has_embedded_fonts,
        has_embedded_files,
        embedded_file_count,
        properties,
        layers,
        nets,
        footprints,
        segments,
        arcs,
        vias,
        zones,
        generated_items,
        dimensions,
        targets,
        groups,
        graphics,
        layer_count,
        property_count,
        net_count,
        footprint_count,
        graphic_count,
        gr_line_count,
        gr_rect_count,
        gr_circle_count,
        gr_arc_count,
        gr_poly_count,
        gr_curve_count,
        gr_text_count,
        gr_text_box_count,
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

fn parse_layers(items: &[Node]) -> Vec<PcbLayer> {
    let mut out = Vec::new();
    for entry in items.iter().skip(1) {
        let Node::List { items: fields, .. } = entry else {
            continue;
        };
        let ordinal = fields.first().and_then(atom_as_i32);
        let name = fields.get(1).and_then(atom_as_string);
        let layer_type = fields.get(2).and_then(atom_as_string);
        let user_name = fields.get(3).and_then(atom_as_string);
        out.push(PcbLayer {
            ordinal,
            name,
            layer_type,
            user_name,
        });
    }
    out
}

fn parse_net(node: &Node) -> PcbNet {
    let Node::List { items, .. } = node else {
        return PcbNet {
            code: None,
            name: None,
        };
    };
    let code = items.get(1).and_then(atom_as_i32);
    let name = items.get(2).and_then(atom_as_string);
    PcbNet { code, name }
}

fn parse_footprint_summary(node: &Node) -> PcbFootprintSummary {
    let Node::List { items, .. } = node else {
        return PcbFootprintSummary {
            lib_id: None,
            layer: None,
            at: None,
            rotation: None,
            uuid: None,
            property_count: 0,
            pad_count: 0,
            model_count: 0,
            zone_count: 0,
            group_count: 0,
            graphic_count: 0,
            fp_line_count: 0,
            fp_rect_count: 0,
            fp_circle_count: 0,
            fp_arc_count: 0,
            fp_poly_count: 0,
            fp_curve_count: 0,
            fp_text_count: 0,
            fp_text_box_count: 0,
            reference: None,
            value: None,
        };
    };

    let lib_id = items.get(1).and_then(atom_as_string);
    let mut layer = None;
    let mut at = None;
    let mut rotation = None;
    let mut uuid = None;
    let mut property_count = 0usize;
    let mut pad_count = 0usize;
    let mut model_count = 0usize;
    let mut zone_count = 0usize;
    let mut group_count = 0usize;
    let mut graphic_count = 0usize;
    let mut fp_line_count = 0usize;
    let mut fp_rect_count = 0usize;
    let mut fp_circle_count = 0usize;
    let mut fp_arc_count = 0usize;
    let mut fp_poly_count = 0usize;
    let mut fp_curve_count = 0usize;
    let mut fp_text_count = 0usize;
    let mut fp_text_box_count = 0usize;
    let mut reference = None;
    let mut value = None;

    for child in items.iter().skip(2) {
        let Some(head) = head_of(child) else {
            continue;
        };
        match head {
            "layer" => {
                layer = second_atom_string(child);
            }
            "at" => {
                let (xy, rot) = parse_xy_and_angle(child);
                at = xy;
                rotation = rot;
            }
            "uuid" => uuid = second_atom_string(child),
            "property" => {
                property_count += 1;
                let Node::List { items: props, .. } = child else {
                    continue;
                };
                let key = props.get(1).and_then(atom_as_string);
                let val = props.get(2).and_then(atom_as_string);
                match key.as_deref() {
                    Some("Reference") => reference = val,
                    Some("Value") => value = val,
                    _ => {}
                }
            }
            "pad" => pad_count += 1,
            "model" => model_count += 1,
            "zone" => zone_count += 1,
            "group" => group_count += 1,
            "fp_line" => {
                graphic_count += 1;
                fp_line_count += 1;
            }
            "fp_rect" => {
                graphic_count += 1;
                fp_rect_count += 1;
            }
            "fp_circle" => {
                graphic_count += 1;
                fp_circle_count += 1;
            }
            "fp_arc" => {
                graphic_count += 1;
                fp_arc_count += 1;
            }
            "fp_poly" => {
                graphic_count += 1;
                fp_poly_count += 1;
            }
            "fp_curve" => {
                graphic_count += 1;
                fp_curve_count += 1;
            }
            "fp_text" => {
                graphic_count += 1;
                fp_text_count += 1;
            }
            "fp_text_box" => {
                graphic_count += 1;
                fp_text_box_count += 1;
            }
            _ => {}
        }
    }

    PcbFootprintSummary {
        lib_id,
        layer,
        at,
        rotation,
        uuid,
        property_count,
        pad_count,
        model_count,
        zone_count,
        group_count,
        graphic_count,
        fp_line_count,
        fp_rect_count,
        fp_circle_count,
        fp_arc_count,
        fp_poly_count,
        fp_curve_count,
        fp_text_count,
        fp_text_box_count,
        reference,
        value,
    }
}

fn parse_segment_summary(node: &Node) -> PcbSegmentSummary {
    let mut start = None;
    let mut end = None;
    let mut width = None;
    let mut layer = None;
    let mut net = None;
    let mut uuid = None;
    let mut locked = false;
    if let Node::List { items, .. } = node {
        locked = items
            .iter()
            .any(|n| matches!(n, Node::Atom { atom: Atom::Symbol(s), .. } if s == "locked"));
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("start") => start = parse_xy(child),
                Some("end") => end = parse_xy(child),
                Some("width") => width = second_atom_f64(child),
                Some("layer") => layer = second_atom_string(child),
                Some("net") => net = second_atom_i32(child),
                Some("uuid") => uuid = second_atom_string(child),
                Some("locked") => locked = true,
                _ => {}
            }
        }
    }
    PcbSegmentSummary {
        start,
        end,
        width,
        layer,
        net,
        uuid,
        locked,
    }
}

fn parse_arc_summary(node: &Node) -> PcbArcSummary {
    let mut start = None;
    let mut mid = None;
    let mut end = None;
    let mut width = None;
    let mut layer = None;
    let mut net = None;
    let mut uuid = None;
    let mut locked = false;
    if let Node::List { items, .. } = node {
        locked = items
            .iter()
            .any(|n| matches!(n, Node::Atom { atom: Atom::Symbol(s), .. } if s == "locked"));
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("start") => start = parse_xy(child),
                Some("mid") => mid = parse_xy(child),
                Some("end") => end = parse_xy(child),
                Some("width") => width = second_atom_f64(child),
                Some("layer") => layer = second_atom_string(child),
                Some("net") => net = second_atom_i32(child),
                Some("uuid") => uuid = second_atom_string(child),
                Some("locked") => locked = true,
                _ => {}
            }
        }
    }
    PcbArcSummary {
        start,
        mid,
        end,
        width,
        layer,
        net,
        uuid,
        locked,
    }
}

fn parse_via_summary(node: &Node) -> PcbViaSummary {
    let mut at = None;
    let mut size = None;
    let mut drill = None;
    let mut drill_x = None;
    let mut drill_y = None;
    let mut drill_shape = None;
    let mut net = None;
    let mut via_type = None;
    let mut layers = Vec::new();
    let mut uuid = None;
    let mut locked = false;
    if let Node::List { items, .. } = node {
        // Some formats encode via type as second symbol: (via blind ...)
        via_type = items.get(1).and_then(|n| match n {
            Node::Atom {
                atom: Atom::Symbol(s),
                ..
            } if matches!(s.as_str(), "blind" | "micro" | "through") => Some(s.clone()),
            _ => None,
        });
        locked = items
            .iter()
            .any(|n| matches!(n, Node::Atom { atom: Atom::Symbol(s), .. } if s == "locked"));
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("at") => at = parse_xy(child),
                Some("size") => size = second_atom_f64(child),
                Some("drill") => {
                    drill = second_atom_f64(child);
                    if let Node::List {
                        items: drill_items, ..
                    } = child
                    {
                        if let Some(shape) = drill_items.get(1).and_then(atom_as_string) {
                            if shape.parse::<f64>().is_err() {
                                drill_shape = Some(shape);
                                drill_x = drill_items.get(2).and_then(atom_as_f64);
                                drill_y = drill_items.get(3).and_then(atom_as_f64);
                            } else {
                                drill_x = drill_items.get(1).and_then(atom_as_f64);
                                drill_y = drill_items.get(2).and_then(atom_as_f64);
                            }
                        }
                    }
                }
                Some("net") => net = second_atom_i32(child),
                Some("layers") => {
                    if let Node::List { items: inner, .. } = child {
                        layers = inner.iter().skip(1).filter_map(atom_as_string).collect();
                    }
                }
                Some("uuid") => uuid = second_atom_string(child),
                Some("locked") => locked = true,
                _ => {}
            }
        }
    }
    PcbViaSummary {
        at,
        size,
        drill,
        drill_x,
        drill_y,
        drill_shape,
        net,
        via_type,
        layers,
        uuid,
        locked,
    }
}

fn parse_zone_summary(node: &Node) -> PcbZoneSummary {
    let mut net = None;
    let mut net_name = None;
    let mut name = None;
    let mut layer = None;
    let mut layers = Vec::new();
    let mut hatch = None;
    let mut fill_enabled = None;
    let mut polygon_count = 0usize;
    let mut filled_polygon_count = 0usize;
    let mut has_keepout = false;
    if let Node::List { items, .. } = node {
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("net") => net = second_atom_i32(child),
                Some("net_name") => net_name = second_atom_string(child),
                Some("name") => name = second_atom_string(child),
                Some("layer") => layer = second_atom_string(child),
                Some("layers") => {
                    if let Node::List { items: inner, .. } = child {
                        layers = inner.iter().skip(1).filter_map(atom_as_string).collect();
                    }
                }
                Some("hatch") => hatch = second_atom_string(child),
                Some("fill") => fill_enabled = second_atom_bool(child),
                Some("polygon") => polygon_count += 1,
                Some("filled_polygon") => filled_polygon_count += 1,
                Some("keepout") => has_keepout = true,
                _ => {}
            }
        }
    }
    PcbZoneSummary {
        net,
        net_name,
        name,
        layer,
        layers,
        hatch,
        fill_enabled,
        polygon_count,
        filled_polygon_count,
        has_keepout,
    }
}

fn parse_generated_summary(node: &Node) -> PcbGeneratedSummary {
    let mut uuid = None;
    let mut generated_type = None;
    let mut name = None;
    let mut layer = None;
    let mut last_netname = None;
    let mut members_count = 0usize;

    if let Node::List { items, .. } = node {
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("uuid") => uuid = second_atom_string(child),
                Some("type") => generated_type = second_atom_string(child),
                Some("name") => name = second_atom_string(child),
                Some("layer") => layer = second_atom_string(child),
                Some("last_netname") => last_netname = second_atom_string(child),
                Some("members") => {
                    if let Some(members) = second_atom_string(child) {
                        members_count = members.split_whitespace().count();
                    }
                }
                _ => {}
            }
        }
    }

    PcbGeneratedSummary {
        uuid,
        generated_type,
        name,
        layer,
        last_netname,
        members_count,
    }
}

fn parse_dimension_summary(node: &Node) -> PcbDimensionSummary {
    let mut dimension_type = None;
    let mut layer = None;
    let mut gr_text_count = 0usize;
    let mut format_present = false;
    if let Node::List { items, .. } = node {
        // Some formats use: (dimension aligned ...), others: (dimension (type aligned) ...)
        dimension_type = items.get(1).and_then(atom_as_string);
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("type") => dimension_type = second_atom_string(child),
                Some("layer") => layer = second_atom_string(child),
                Some("gr_text") => gr_text_count += 1,
                Some("format") => format_present = true,
                _ => {}
            }
        }
    }
    PcbDimensionSummary {
        dimension_type,
        layer,
        gr_text_count,
        format_present,
    }
}

fn parse_target_summary(node: &Node) -> PcbTargetSummary {
    let mut shape = None;
    let mut at = None;
    let mut size = None;
    let mut width = None;
    let mut layer = None;
    if let Node::List { items, .. } = node {
        shape = items.get(1).and_then(atom_as_string);
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("at") => at = parse_xy(child),
                Some("size") => size = second_atom_f64(child),
                Some("width") => width = second_atom_f64(child),
                Some("layer") => layer = second_atom_string(child),
                _ => {}
            }
        }
    }
    PcbTargetSummary {
        shape,
        at,
        size,
        width,
        layer,
    }
}

fn parse_group_summary(node: &Node) -> PcbGroupSummary {
    let mut name = None;
    let mut group_id = None;
    let mut member_count = 0usize;
    if let Node::List { items, .. } = node {
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("name") => name = second_atom_string(child),
                Some("id") => group_id = second_atom_string(child),
                Some("members") => {
                    if let Node::List { items: inner, .. } = child {
                        member_count = inner.len().saturating_sub(1);
                    }
                }
                _ => {}
            }
        }
    }
    PcbGroupSummary {
        name,
        group_id,
        member_count,
    }
}

fn parse_graphic_summary(node: &Node, token: &str) -> PcbGraphicSummary {
    let mut layer = None;
    let mut text = None;
    let mut start = None;
    let mut end = None;
    let mut center = None;
    let mut uuid = None;
    let mut locked = false;

    if let Node::List { items, .. } = node {
        // Only text graphics treat second token as content.
        if matches!(token, "gr_text" | "gr_text_box") {
            text = items.get(1).and_then(atom_as_string);
        }
        locked = items
            .iter()
            .any(|n| matches!(n, Node::Atom { atom: Atom::Symbol(s), .. } if s == "locked"));
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("layer") => layer = second_atom_string(child),
                Some("start") => start = parse_xy(child),
                Some("end") => end = parse_xy(child),
                Some("center") => center = parse_xy(child),
                Some("uuid") => uuid = second_atom_string(child),
                Some("locked") => locked = true,
                _ => {}
            }
        }
    }

    PcbGraphicSummary {
        token: token.to_string(),
        layer,
        text,
        start,
        end,
        center,
        uuid,
        locked,
    }
}

fn parse_property(node: &Node) -> Option<PcbProperty> {
    let Node::List { items, .. } = node else {
        return None;
    };
    if !matches!(
        items.first().and_then(atom_as_string).as_deref(),
        Some("property")
    ) {
        return None;
    }
    let key = items.get(1).and_then(atom_as_string)?;
    let value = items.get(2).and_then(atom_as_string)?;
    Some(PcbProperty { key, value })
}

fn parse_general_summary(node: &Node) -> PcbGeneralSummary {
    let mut thickness = None;
    let mut legacy_teardrops = None;
    if let Node::List { items, .. } = node {
        for child in items.iter().skip(1) {
            match head_of(child) {
                Some("thickness") => thickness = second_atom_f64(child),
                Some("legacy_teardrops") => legacy_teardrops = second_atom_bool(child),
                _ => {}
            }
        }
    }
    PcbGeneralSummary {
        thickness,
        legacy_teardrops,
    }
}

fn parse_setup_summary(node: &Node) -> PcbSetupSummary {
    let mut has_stackup = false;
    let mut stackup_layer_count = 0usize;
    let mut has_plot_settings = false;
    let mut pad_to_mask_clearance = None;
    let mut solder_mask_min_width = None;
    let mut aux_axis_origin = None;
    let mut grid_origin = None;
    let mut setup_tokens = Vec::new();

    if let Node::List { items, .. } = node {
        for child in items.iter().skip(1) {
            if let Some(head) = head_of(child) {
                setup_tokens.push(head.to_string());
                match head {
                    "stackup" => {
                        has_stackup = true;
                        if let Node::List {
                            items: stackup_items,
                            ..
                        } = child
                        {
                            stackup_layer_count = stackup_items
                                .iter()
                                .filter(|n| matches!(head_of(n), Some("layer")))
                                .count();
                        }
                    }
                    "pcbplotparams" => has_plot_settings = true,
                    "pad_to_mask_clearance" => pad_to_mask_clearance = second_atom_f64(child),
                    "solder_mask_min_width" => solder_mask_min_width = second_atom_f64(child),
                    "aux_axis_origin" => aux_axis_origin = parse_xy(child),
                    "grid_origin" => grid_origin = parse_xy(child),
                    _ => {}
                }
            }
        }
    }

    PcbSetupSummary {
        has_stackup,
        stackup_layer_count,
        has_plot_settings,
        pad_to_mask_clearance,
        solder_mask_min_width,
        aux_axis_origin,
        grid_origin,
        setup_tokens,
    }
}

fn collect_unrecognized_children(node: &Node, known_heads: &[&str], out: &mut Vec<UnknownNode>) {
    let Node::List { items, .. } = node else {
        return;
    };
    for child in items.iter().skip(1) {
        match head_of(child) {
            Some(head) if known_heads.contains(&head) => {}
            _ => {
                if let Some(unknown) = UnknownNode::from_node(child) {
                    out.push(unknown);
                }
            }
        }
    }
}

fn collect_unrecognized_setup_children(node: &Node, out: &mut Vec<UnknownNode>) {
    let Node::List { items, .. } = node else {
        return;
    };
    for child in items.iter().skip(1) {
        match head_of(child) {
            Some("stackup") => {
                collect_unrecognized_children(child, &["layer"], out);
            }
            Some("pcbplotparams")
            | Some("pad_to_mask_clearance")
            | Some("solder_mask_min_width")
            | Some("aux_axis_origin")
            | Some("grid_origin") => {}
            _ => {
                if let Some(unknown) = UnknownNode::from_node(child) {
                    out.push(unknown);
                }
            }
        }
    }
}

fn parse_xy(node: &Node) -> Option<[f64; 2]> {
    let Node::List { items, .. } = node else {
        return None;
    };
    let x = items.get(1).and_then(atom_as_string)?.parse::<f64>().ok()?;
    let y = items.get(2).and_then(atom_as_string)?.parse::<f64>().ok()?;
    Some([x, y])
}

fn parse_xy_and_angle(node: &Node) -> (Option<[f64; 2]>, Option<f64>) {
    let Node::List { items, .. } = node else {
        return (None, None);
    };
    let x = items.get(1).and_then(atom_as_f64);
    let y = items.get(2).and_then(atom_as_f64);
    let rot = items.get(3).and_then(atom_as_f64);
    match (x, y) {
        (Some(x), Some(y)) => (Some([x, y]), rot),
        _ => (None, rot),
    }
}

// ─── Private structural-editing helpers ──────────────────────────────────────

fn pcb_generate_uuid() -> String {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = t.as_secs();
    let nanos = t.subsec_nanos();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        secs & 0xffff_ffff,
        (secs >> 16) & 0xffff,
        nanos & 0xffff,
        (nanos >> 16) & 0xffff,
        nanos as u64 | (secs << 20)
    )
}

fn pcb_fmt(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn pcb_segment_node(x1: f64, y1: f64, x2: f64, y2: f64, width: f64, layer: &str, net: i32) -> Node {
    list_node(vec![
        atom_symbol("segment"),
        list_node(vec![
            atom_symbol("start"),
            atom_symbol(pcb_fmt(x1)),
            atom_symbol(pcb_fmt(y1)),
        ]),
        list_node(vec![
            atom_symbol("end"),
            atom_symbol(pcb_fmt(x2)),
            atom_symbol(pcb_fmt(y2)),
        ]),
        list_node(vec![atom_symbol("width"), atom_symbol(pcb_fmt(width))]),
        list_node(vec![atom_symbol("layer"), atom_quoted(layer)]),
        list_node(vec![atom_symbol("net"), atom_symbol(net.to_string())]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(pcb_generate_uuid())]),
    ])
}

fn pcb_via_node(x: f64, y: f64, size: f64, drill: f64, net: i32) -> Node {
    list_node(vec![
        atom_symbol("via"),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(pcb_fmt(x)),
            atom_symbol(pcb_fmt(y)),
        ]),
        list_node(vec![atom_symbol("size"), atom_symbol(pcb_fmt(size))]),
        list_node(vec![atom_symbol("drill"), atom_symbol(pcb_fmt(drill))]),
        list_node(vec![
            atom_symbol("layers"),
            atom_quoted("F.Cu"),
            atom_quoted("B.Cu"),
        ]),
        list_node(vec![atom_symbol("net"), atom_symbol(net.to_string())]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(pcb_generate_uuid())]),
    ])
}

fn pcb_footprint_property_node(key: &str, value: &str, layer: &str) -> Node {
    list_node(vec![
        atom_symbol("property"),
        atom_quoted(key),
        atom_quoted(value),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol("0"),
            atom_symbol("0"),
            atom_symbol("0"),
        ]),
        list_node(vec![atom_symbol("layer"), atom_quoted(layer)]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(pcb_generate_uuid())]),
    ])
}

fn pcb_footprint_node(
    lib_ref: &str,
    x: f64,
    y: f64,
    layer: &str,
    reference: &str,
    value: &str,
) -> Node {
    list_node(vec![
        atom_symbol("footprint"),
        atom_quoted(lib_ref),
        list_node(vec![atom_symbol("layer"), atom_quoted(layer)]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(pcb_generate_uuid())]),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(pcb_fmt(x)),
            atom_symbol(pcb_fmt(y)),
        ]),
        pcb_footprint_property_node(
            "Reference",
            reference,
            if layer == "B.Cu" {
                "B.SilkS"
            } else {
                "F.SilkS"
            },
        ),
        pcb_footprint_property_node(
            "Value",
            value,
            if layer == "B.Cu" { "B.Fab" } else { "F.Fab" },
        ),
    ])
}

fn segment_pts_match(node: &Node, x1: f64, y1: f64, x2: f64, y2: f64) -> bool {
    (|| -> Option<bool> {
        if head_of(node) != Some("segment") {
            return Some(false);
        }
        let Node::List { items, .. } = node else {
            return Some(false);
        };
        let parse_pair = |head: &str| -> Option<(f64, f64)> {
            let child = items.iter().skip(1).find(|n| head_of(n) == Some(head))?;
            let Node::List {
                items: child_items, ..
            } = child
            else {
                return None;
            };
            let x = child_items
                .get(1)
                .and_then(atom_as_string)?
                .parse::<f64>()
                .ok()?;
            let y = child_items
                .get(2)
                .and_then(atom_as_string)?
                .parse::<f64>()
                .ok()?;
            Some((x, y))
        };
        let start = parse_pair("start")?;
        let end = parse_pair("end")?;
        Some(start == (x1, y1) && end == (x2, y2))
    })()
    .unwrap_or(false)
}

fn footprint_has_reference(node: &Node, reference: &str) -> bool {
    if head_of(node) != Some("footprint") {
        return false;
    }
    let Node::List { items, .. } = node else {
        return false;
    };
    if let Some(idx) = find_property_index(items, "Reference", 1) {
        if let Some(Node::List {
            items: prop_items, ..
        }) = items.get(idx)
        {
            return prop_items.get(2).and_then(atom_as_string).as_deref() == Some(reference);
        }
    }
    false
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
    fn read_warns_on_legacy_version() {
        let path = tmp_file("pcb_old_version");
        fs::write(&path, "(kicad_pcb (version 20220101))\n").expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.diagnostics().len(), 1);
        assert_eq!(doc.diagnostics()[0].code, "legacy_format");

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
        assert_eq!(
            doc.ast().unknown_nodes[0].head.as_deref(),
            Some("mystery_token")
        );

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
        let src = "(kicad_pcb (version 20260101) (generator pcbnew)\n  (general (thickness 1.6) (legacy_teardrops no))\n  (paper \"User\" 100 80 portrait)\n  (title_block (title \"Demo\") (date \"2026-02-23\") (rev \"A\") (company \"Milind\") (comment 1 \"c1\") (comment 2 \"c2\"))\n  (property \"Owner\" \"Milind\")\n  (layers (0 F.Cu signal) (31 B.Cu signal))\n  (setup (stackup (layer \"F.Cu\" (type \"copper\")) (layer \"B.Cu\" (type \"copper\"))) (pcbplotparams) (pad_to_mask_clearance 0.1) (solder_mask_min_width 0.0) (aux_axis_origin 10 20) (grid_origin 11 21))\n  (net 0 \"\")\n  (footprint \"R_0603\"\n    (at 10 20 90)\n    (layer F.Cu)\n    (uuid \"fp-1\")\n    (property \"Reference\" \"R1\")\n    (property \"Value\" \"1k\")\n    (fp_line (start 0 0) (end 1 1) (layer F.SilkS))\n    (fp_text reference \"R1\" (at 0 0) (layer F.SilkS))\n    (pad \"1\" smd rect (at 0 0) (size 1 1) (layers F.Cu F.Mask))\n    (model \"r.step\")\n  )\n  (gr_line locked (start 0 0) (end 1 1) (layer F.Cu) (uuid \"g-1\"))\n  (segment locked (start 0 0) (end 1 1) (width 0.25) (layer F.Cu) (net 0) (uuid \"s-1\"))\n  (arc (start 0 0) (mid 0.5 0.5) (end 1 1) (width 0.25) (layer F.Cu) (net 0) (uuid \"a-1\"))\n  (via blind locked (at 0 0) (size 1) (drill oval 0.5 0.25) (layers F.Cu B.Cu) (uuid \"v-1\"))\n  (zone)\n  (dimension aligned (layer F.Cu) (gr_text \"1.0\" (at 0 0)))\n  (target plus (at 1 2) (size 1) (width 0.1) (layer F.Cu))\n  (group (name \"G\") (id \"abc\") (members \"u1\" \"u2\"))\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.ast().layer_count, 2);
        assert_eq!(doc.ast().layers.len(), 2);
        assert_eq!(doc.ast().layers[0].name.as_deref(), Some("F.Cu"));
        assert_eq!(doc.ast().property_count, 1);
        assert_eq!(doc.ast().properties.len(), 1);
        assert_eq!(doc.ast().properties[0].key, "Owner");
        assert!(doc.ast().has_general);
        assert!(doc.ast().has_paper);
        assert!(doc.ast().has_title_block);
        assert_eq!(
            doc.ast().general.as_ref().and_then(|g| g.thickness),
            Some(1.6)
        );
        assert_eq!(
            doc.ast().general.as_ref().and_then(|g| g.legacy_teardrops),
            Some(false)
        );
        assert_eq!(
            doc.ast().paper.as_ref().and_then(|p| p.kind.clone()),
            Some("User".to_string())
        );
        assert_eq!(doc.ast().paper.as_ref().and_then(|p| p.width), Some(100.0));
        assert_eq!(doc.ast().paper.as_ref().and_then(|p| p.height), Some(80.0));
        assert_eq!(
            doc.ast().paper.as_ref().and_then(|p| p.orientation.clone()),
            Some("portrait".to_string())
        );
        assert_eq!(
            doc.ast().title_block.as_ref().and_then(|t| t.title.clone()),
            Some("Demo".to_string())
        );
        assert_eq!(
            doc.ast().title_block.as_ref().map(|t| t.comments.len()),
            Some(2)
        );
        assert_eq!(
            doc.ast()
                .title_block
                .as_ref()
                .and_then(|t| t.comments.first().cloned()),
            Some("c1".to_string())
        );
        assert_eq!(doc.ast().setup.as_ref().map(|s| s.has_stackup), Some(true));
        assert_eq!(
            doc.ast().setup.as_ref().map(|s| s.stackup_layer_count),
            Some(2)
        );
        assert_eq!(
            doc.ast().setup.as_ref().map(|s| s.has_plot_settings),
            Some(true)
        );
        assert_eq!(
            doc.ast()
                .setup
                .as_ref()
                .and_then(|s| s.pad_to_mask_clearance),
            Some(0.1)
        );
        assert_eq!(doc.ast().net_count, 1);
        assert_eq!(doc.ast().nets.len(), 1);
        assert_eq!(doc.ast().nets[0].name.as_deref(), Some(""));
        assert_eq!(doc.ast().footprint_count, 1);
        assert_eq!(doc.ast().footprints.len(), 1);
        assert_eq!(doc.ast().footprints[0].lib_id.as_deref(), Some("R_0603"));
        assert_eq!(doc.ast().footprints[0].layer.as_deref(), Some("F.Cu"));
        assert_eq!(doc.ast().footprints[0].at, Some([10.0, 20.0]));
        assert_eq!(doc.ast().footprints[0].rotation, Some(90.0));
        assert_eq!(doc.ast().footprints[0].uuid.as_deref(), Some("fp-1"));
        assert_eq!(doc.ast().footprints[0].reference.as_deref(), Some("R1"));
        assert_eq!(doc.ast().footprints[0].value.as_deref(), Some("1k"));
        assert_eq!(doc.ast().footprints[0].property_count, 2);
        assert_eq!(doc.ast().footprints[0].pad_count, 1);
        assert_eq!(doc.ast().footprints[0].model_count, 1);
        assert_eq!(doc.ast().footprints[0].graphic_count, 2);
        assert_eq!(doc.ast().footprints[0].fp_line_count, 1);
        assert_eq!(doc.ast().footprints[0].fp_text_count, 1);
        assert_eq!(doc.ast().graphic_count, 1);
        assert_eq!(doc.ast().gr_line_count, 1);
        assert_eq!(doc.ast().graphics.len(), 1);
        assert_eq!(doc.ast().graphics[0].token, "gr_line");
        assert_eq!(doc.ast().graphics[0].layer.as_deref(), Some("F.Cu"));
        assert!(doc.ast().graphics[0].locked);
        assert_eq!(doc.ast().graphics[0].uuid.as_deref(), Some("g-1"));
        assert_eq!(doc.ast().trace_segment_count, 1);
        assert_eq!(doc.ast().segments.len(), 1);
        assert_eq!(doc.ast().segments[0].layer.as_deref(), Some("F.Cu"));
        assert!(doc.ast().segments[0].locked);
        assert_eq!(doc.ast().segments[0].uuid.as_deref(), Some("s-1"));
        assert_eq!(doc.ast().trace_arc_count, 1);
        assert_eq!(doc.ast().arcs.len(), 1);
        assert_eq!(doc.ast().arcs[0].uuid.as_deref(), Some("a-1"));
        assert_eq!(doc.ast().via_count, 1);
        assert_eq!(doc.ast().vias.len(), 1);
        assert_eq!(doc.ast().vias[0].via_type.as_deref(), Some("blind"));
        assert!(doc.ast().vias[0].locked);
        assert_eq!(doc.ast().vias[0].drill, None);
        assert_eq!(doc.ast().vias[0].drill_shape.as_deref(), Some("oval"));
        assert_eq!(doc.ast().vias[0].drill_x, Some(0.5));
        assert_eq!(doc.ast().vias[0].drill_y, Some(0.25));
        assert_eq!(doc.ast().vias[0].layers.len(), 2);
        assert_eq!(doc.ast().zone_count, 1);
        assert_eq!(doc.ast().zones.len(), 1);
        assert_eq!(doc.ast().zones[0].polygon_count, 0);
        assert_eq!(doc.ast().dimension_count, 1);
        assert_eq!(doc.ast().dimensions.len(), 1);
        assert_eq!(doc.ast().dimensions[0].layer.as_deref(), Some("F.Cu"));
        assert_eq!(doc.ast().target_count, 1);
        assert_eq!(doc.ast().targets.len(), 1);
        assert_eq!(doc.ast().targets[0].shape.as_deref(), Some("plus"));
        assert_eq!(doc.ast().group_count, 1);
        assert_eq!(doc.ast().groups.len(), 1);
        assert_eq!(doc.ast().groups[0].member_count, 2);
        assert!(!doc.ast().has_embedded_files);
        assert_eq!(doc.ast().embedded_file_count, 0);
        assert!(doc.ast().has_setup);
        assert!(doc.ast().unknown_nodes.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_embedded_files_regression() {
        let path = tmp_file("pcb_embedded_files");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew)\n  (embedded_files\n    (file (name \"A.bin\") (type \"binary\") (data \"abc\"))\n    (file (name \"B.bin\") (type \"binary\") (data \"def\"))\n  )\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert!(doc.ast().has_embedded_files);
        assert_eq!(doc.ast().embedded_file_count, 2);
        assert!(doc.ast().unknown_nodes.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_dimension_type_child_token_regression() {
        let path = tmp_file("pcb_dimension_type");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew)\n  (dimension\n    (type aligned)\n    (layer \"Cmts.User\")\n    (format (units 2))\n    (gr_text \"10.0 mm\" (at 0 0))\n  )\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.ast().dimension_count, 1);
        assert_eq!(doc.ast().dimensions.len(), 1);
        assert_eq!(
            doc.ast().dimensions[0].dimension_type.as_deref(),
            Some("aligned")
        );
        assert_eq!(doc.ast().dimensions[0].layer.as_deref(), Some("Cmts.User"));
        assert!(doc.ast().dimensions[0].format_present);
        assert_eq!(doc.ast().dimensions[0].gr_text_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_standard_paper_orientation() {
        let path = tmp_file("pcb_std_paper");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew) (paper A4 portrait))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert!(doc.ast().has_paper);
        assert_eq!(
            doc.ast().paper.as_ref().and_then(|p| p.kind.clone()),
            Some("A4".to_string())
        );
        assert_eq!(doc.ast().paper.as_ref().and_then(|p| p.width), None);
        assert_eq!(doc.ast().paper.as_ref().and_then(|p| p.height), None);
        assert_eq!(
            doc.ast().paper.as_ref().and_then(|p| p.orientation.clone()),
            Some("portrait".to_string())
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn edit_roundtrip_updates_core_fields_and_preserves_unknowns() {
        let path = tmp_file("pcb_edit_roundtrip");
        let src = "(kicad_pcb (version 20241229) (generator pcbnew)\n  (paper \"A4\")\n  (title_block (title \"Old\") (date \"2025-01-01\") (rev \"A\") (company \"OldCo\"))\n  (property \"Owner\" \"Milind\")\n  (future_token 1 2)\n)\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.set_version(20260101)
            .set_generator("kiutils")
            .set_generator_version("dev")
            .set_paper_standard("A3", Some("portrait"))
            .set_title("Roundtrip Demo")
            .set_date("2026-02-25")
            .set_revision("B")
            .set_company("Lords")
            .upsert_property("Owner", "Milind Sharma")
            .upsert_property("Build", "2")
            .remove_property("DoesNotExist");

        let out = tmp_file("pcb_edit_roundtrip_out");
        doc.write(&out).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert!(written.contains("(future_token 1 2)"));

        let reread = PcbFile::read(&out).expect("reread");
        assert_eq!(reread.ast().version, Some(20260101));
        assert_eq!(reread.ast().generator.as_deref(), Some("kiutils"));
        assert_eq!(reread.ast().generator_version.as_deref(), Some("dev"));
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.kind.clone()),
            Some("A3".to_string())
        );
        assert_eq!(
            reread
                .ast()
                .paper
                .as_ref()
                .and_then(|p| p.orientation.clone()),
            Some("portrait".to_string())
        );
        assert_eq!(
            reread
                .ast()
                .title_block
                .as_ref()
                .and_then(|t| t.title.clone()),
            Some("Roundtrip Demo".to_string())
        );
        assert_eq!(reread.ast().property_count, 2);
        assert_eq!(reread.ast().unknown_nodes.len(), 1);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn lossless_write_preserves_unrelated_formatting_for_targeted_edit() {
        let path = tmp_file("pcb_lossless_targeted_edit");
        let src =
            "(kicad_pcb  (version   20241229)\n  (generator pcbnew)\n  (future_token   1   2)\n)\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.set_version(20260101);

        let out = tmp_file("pcb_lossless_targeted_edit_out");
        doc.write(&out).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert_eq!(
            written,
            "(kicad_pcb  (version 20260101)\n  (generator pcbnew)\n  (future_token   1   2)\n)\n"
        );

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn no_op_setter_keeps_lossless_raw_unchanged() {
        let path = tmp_file("pcb_noop_setter");
        let src = "(kicad_pcb  (version   20241229)\n  (generator pcbnew)\n)\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.set_version(20241229);

        let out = tmp_file("pcb_noop_setter_out");
        doc.write(&out).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert_eq!(written, src);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn ast_mut_write_returns_validation_error() {
        let path = tmp_file("pcb_ast_mut_write_error");
        let src = "(kicad_pcb (version 20241229) (generator pcbnew))\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.ast_mut().version = Some(20260101);

        let out = tmp_file("pcb_ast_mut_write_error_out");
        let err = doc.write(&out).expect_err("write should fail");
        match err {
            Error::Validation(msg) => {
                assert!(msg.contains("ast_mut changes are not serializable"));
            }
            _ => panic!("expected validation error"),
        }

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn set_paper_preserves_unknown_tail_children() {
        let path = tmp_file("pcb_paper_preserve_tail");
        let src =
            "(kicad_pcb (version 20241229) (generator pcbnew) (paper \"A4\" portrait (mystery 1)))\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.set_paper_standard("A3", Some("portrait"));

        let out = tmp_file("pcb_paper_preserve_tail_out");
        doc.write(&out).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert!(written.contains("(paper \"A3\" portrait (mystery 1))"));

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn set_version_preserves_unknown_tail_children() {
        let path = tmp_file("pcb_version_preserve_tail");
        let src = "(kicad_pcb (version 20241229 extra_token) (generator pcbnew))\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.set_version(20260101);

        let out = tmp_file("pcb_version_preserve_tail_out");
        doc.write(&out).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert!(written.contains("(version 20260101 extra_token)"));

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn captures_nested_unknown_nodes_in_setup_section() {
        let path = tmp_file("pcb_nested_unknown_setup");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew)\n  (setup (stackup (layer \"F.Cu\") (mystery_stackup_token x)) (mystery_setup_token y))\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert!(doc
            .ast()
            .unknown_nodes
            .iter()
            .any(|n| n.head.as_deref() == Some("mystery_setup_token")));
        assert!(doc
            .ast()
            .unknown_nodes
            .iter()
            .any(|n| n.head.as_deref() == Some("mystery_stackup_token")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn non_text_graphics_do_not_parse_locked_as_text() {
        let path = tmp_file("pcb_graphic_text_regression");
        let src = "(kicad_pcb (version 20241229) (generator pcbnew)\n  (gr_line locked (start 0 0) (end 1 1) (layer \"F.SilkS\"))\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = PcbFile::read(&path).expect("read");
        assert_eq!(doc.ast().graphics.len(), 1);
        assert_eq!(doc.ast().graphics[0].token, "gr_line");
        assert_eq!(doc.ast().graphics[0].text, None);
        assert!(doc.ast().graphics[0].locked);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn edit_roundtrip_updates_user_paper_dimensions() {
        let path = tmp_file("pcb_edit_paper_user");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew) (paper A4))\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.set_paper_user(100.0, 80.0, Some("landscape"));

        let out = tmp_file("pcb_edit_paper_user_out");
        doc.write(&out).expect("write");
        let reread = PcbFile::read(&out).expect("reread");
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.kind.clone()),
            Some("User".to_string())
        );
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.width),
            Some(100.0)
        );
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.height),
            Some(80.0)
        );
        assert_eq!(
            reread
                .ast()
                .paper
                .as_ref()
                .and_then(|p| p.orientation.clone()),
            Some("landscape".to_string())
        );

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn upsert_property_preserves_existing_extra_children() {
        let path = tmp_file("pcb_property_preserve_extra");
        let src = "(kicad_pcb (version 20260101) (generator pcbnew)\n  (property \"Owner\" \"Old\" (at 1 2 0))\n)\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = PcbFile::read(&path).expect("read");
        doc.upsert_property("Owner", "New");

        let out = tmp_file("pcb_property_preserve_extra_out");
        doc.write(&out).expect("write");
        let written = fs::read_to_string(&out).expect("read out");
        assert!(written.contains("(property \"Owner\" \"New\" (at 1 2 0))"));

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    const MINIMAL_PCB: &str = "(kicad_pcb (version 20260101) (generator pcbnew) (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal)) (net 0 \"\") (net 1 \"GND\") (net 2 \"VCC\") (footprint \"Device:R\" (layer \"F.Cu\") (uuid \"fp-1\") (at 100 100) (property \"Reference\" \"R1\" (at 0 -1.5 0) (layer \"F.SilkS\") (uuid \"fp-ref-1\")) (property \"Value\" \"10k\" (at 0 1.5 0) (layer \"F.Fab\") (uuid \"fp-val-1\"))) (segment (start 100 100) (end 110 100) (width 0.25) (layer \"F.Cu\") (net 1) (uuid \"seg-1\")))\n";

    // ─── Trace (segment) deep tests ───────────────────────────────────────────

    #[test]
    fn add_trace_fields_correct() {
        let path = tmp_file("pcb_trace_fields");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_trace(10.5, 20.25, 30.5, 20.25, 0.127, "B.Cu", 2);

        let segs = &doc.ast().segments;
        let new_seg = segs
            .iter()
            .find(|s| s.start == Some([10.5, 20.25]) && s.end == Some([30.5, 20.25]))
            .expect("added segment not found");

        assert_eq!(new_seg.start, Some([10.5, 20.25]));
        assert_eq!(new_seg.end, Some([30.5, 20.25]));
        assert_eq!(new_seg.width, Some(0.127));
        assert_eq!(new_seg.layer.as_deref(), Some("B.Cu"));
        assert_eq!(new_seg.net, Some(2));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_trace_fractional_coords_preserved() {
        let path = tmp_file("pcb_trace_frac");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_trace(0.5, 0.25, 10.75, 0.25, 0.127, "F.Cu", 1);

        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("0.5"), "x1 0.5 not in output: {raw}");
        assert!(raw.contains("0.25"), "y1 0.25 not in output: {raw}");
        assert!(raw.contains("10.75"), "x2 10.75 not in output: {raw}");
        assert!(raw.contains("0.127"), "width 0.127 not in output: {raw}");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_trace_noop_wrong_coords() {
        let path = tmp_file("pcb_trace_noop");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        assert_eq!(doc.ast().trace_segment_count, 1);
        doc.remove_trace_at(999.0, 999.0, 888.0, 888.0);
        assert_eq!(
            doc.ast().trace_segment_count,
            1,
            "count should not change on no match"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_trace_specificity() {
        let path = tmp_file("pcb_trace_specific");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_trace(0.0, 0.0, 10.0, 0.0, 0.25, "F.Cu", 1);
        doc.add_trace(20.0, 0.0, 30.0, 0.0, 0.25, "F.Cu", 1);
        assert_eq!(doc.ast().trace_segment_count, 3);

        doc.remove_trace_at(0.0, 0.0, 10.0, 0.0);
        assert_eq!(doc.ast().trace_segment_count, 2);

        // the second added trace must still be there
        let segs = &doc.ast().segments;
        assert!(
            segs.iter()
                .any(|s| s.start == Some([20.0, 0.0]) && s.end == Some([30.0, 0.0])),
            "remaining trace was incorrectly removed"
        );
        // the removed trace must be gone
        assert!(
            !segs
                .iter()
                .any(|s| s.start == Some([0.0, 0.0]) && s.end == Some([10.0, 0.0])),
            "removed trace is still present"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_remove_trace_restores_count() {
        let path = tmp_file("pcb_trace_symmetric");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        let original_count = doc.ast().trace_segment_count;
        doc.add_trace(5.0, 5.0, 15.0, 5.0, 0.25, "F.Cu", 1);
        assert_eq!(doc.ast().trace_segment_count, original_count + 1);
        doc.remove_trace_at(5.0, 5.0, 15.0, 5.0);
        assert_eq!(doc.ast().trace_segment_count, original_count);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_trace_survives_write_reread() {
        let path = tmp_file("pcb_trace_reread");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_trace(1.5, 2.5, 11.5, 2.5, 0.3, "B.Cu", 2);
        doc.write(&path).expect("write");

        let reread = PcbFile::read(&path).expect("reread");
        let seg = reread
            .ast()
            .segments
            .iter()
            .find(|s| s.start == Some([1.5, 2.5]))
            .expect("segment lost after write+read");
        assert_eq!(seg.end, Some([11.5, 2.5]));
        assert_eq!(seg.width, Some(0.3));
        assert_eq!(seg.layer.as_deref(), Some("B.Cu"));
        assert_eq!(seg.net, Some(2));

        let _ = fs::remove_file(path);
    }

    // ─── Via deep tests ───────────────────────────────────────────────────────

    #[test]
    fn add_via_fields_correct() {
        let path = tmp_file("pcb_via_fields");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_via(50.5, 60.5, 0.8, 0.4, 1);

        let via = doc
            .ast()
            .vias
            .iter()
            .find(|v| v.at == Some([50.5, 60.5]))
            .expect("via not found");

        assert_eq!(via.at, Some([50.5, 60.5]));
        assert_eq!(via.size, Some(0.8));
        assert_eq!(via.drill, Some(0.4));
        assert_eq!(via.net, Some(1));
        assert!(
            via.layers.contains(&"F.Cu".to_string()) && via.layers.contains(&"B.Cu".to_string()),
            "expected F.Cu and B.Cu layers, got {:?}",
            via.layers
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_via_survives_write_reread() {
        let path = tmp_file("pcb_via_reread");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_via(5.0, 10.0, 1.0, 0.5, 2);
        doc.write(&path).expect("write");

        let reread = PcbFile::read(&path).expect("reread");
        let via = reread
            .ast()
            .vias
            .iter()
            .find(|v| v.at == Some([5.0, 10.0]))
            .expect("via lost after write+read");
        assert_eq!(via.size, Some(1.0));
        assert_eq!(via.drill, Some(0.5));
        assert_eq!(via.net, Some(2));

        let _ = fs::remove_file(path);
    }

    // ─── Footprint deep tests ─────────────────────────────────────────────────

    #[test]
    fn add_footprint_fields_correct() {
        let path = tmp_file("pcb_fp_fields");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:C", 120.5, 100.5, "F.Cu", "C1", "100nF");

        let fp = doc
            .ast()
            .footprints
            .iter()
            .find(|f| f.reference.as_deref() == Some("C1"))
            .expect("footprint not found");

        assert_eq!(fp.lib_id.as_deref(), Some("Device:C"));
        assert_eq!(fp.at, Some([120.5, 100.5]));
        assert_eq!(fp.layer.as_deref(), Some("F.Cu"));
        assert_eq!(fp.reference.as_deref(), Some("C1"));
        assert_eq!(fp.value.as_deref(), Some("100nF"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_footprint_back_copper_uses_back_layers() {
        let path = tmp_file("pcb_fp_back");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:C", 120.0, 100.0, "B.Cu", "C1", "100nF");

        let raw = doc.cst().to_canonical_string();
        assert!(
            raw.contains("B.SilkS"),
            "expected B.SilkS for back-copper Reference layer"
        );
        assert!(
            raw.contains("B.Fab"),
            "expected B.Fab for back-copper Value layer"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_footprint_front_copper_uses_front_layers() {
        let path = tmp_file("pcb_fp_front");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:R", 130.0, 100.0, "F.Cu", "R2", "4.7k");

        let raw = doc.cst().to_canonical_string();
        assert!(
            raw.contains("F.SilkS"),
            "expected F.SilkS for front-copper Reference layer"
        );
        assert!(
            raw.contains("F.Fab"),
            "expected F.Fab for front-copper Value layer"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_footprint_noop_wrong_ref() {
        let path = tmp_file("pcb_fp_noop");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        assert_eq!(doc.ast().footprint_count, 1);
        doc.remove_footprint("Z99");
        assert_eq!(
            doc.ast().footprint_count,
            1,
            "count should not change on no match"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_footprint_specificity() {
        let path = tmp_file("pcb_fp_specific");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:C", 120.0, 100.0, "F.Cu", "C1", "100nF");
        doc.add_footprint("Device:C", 140.0, 100.0, "F.Cu", "C2", "10uF");
        assert_eq!(doc.ast().footprint_count, 3);

        doc.remove_footprint("C1");
        assert_eq!(doc.ast().footprint_count, 2);

        let fps = &doc.ast().footprints;
        assert!(
            fps.iter().any(|f| f.reference.as_deref() == Some("C2")),
            "C2 was incorrectly removed"
        );
        assert!(
            !fps.iter().any(|f| f.reference.as_deref() == Some("C1")),
            "C1 was not removed"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_remove_footprint_restores_count() {
        let path = tmp_file("pcb_fp_symmetric");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        let original = doc.ast().footprint_count;
        doc.add_footprint("Device:L", 150.0, 100.0, "F.Cu", "L1", "10uH");
        assert_eq!(doc.ast().footprint_count, original + 1);
        doc.remove_footprint("L1");
        assert_eq!(doc.ast().footprint_count, original);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_footprint_survives_write_reread() {
        let path = tmp_file("pcb_fp_reread");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:LED", 200.0, 100.0, "F.Cu", "D1", "LED_Red");
        doc.write(&path).expect("write");

        let reread = PcbFile::read(&path).expect("reread");
        let fp = reread
            .ast()
            .footprints
            .iter()
            .find(|f| f.reference.as_deref() == Some("D1"))
            .expect("footprint lost after write+read");
        assert_eq!(fp.lib_id.as_deref(), Some("Device:LED"));
        assert_eq!(fp.at, Some([200.0, 100.0]));
        assert_eq!(fp.value.as_deref(), Some("LED_Red"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_trace_increases_count() {
        let path = tmp_file("pcb_add_trace");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        assert_eq!(doc.ast().trace_segment_count, 1);
        doc.add_trace(0.0, 0.0, 100.0, 0.0, 0.25, "F.Cu", 1);
        assert_eq!(doc.ast().trace_segment_count, 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_trace_at_decreases_count() {
        let path = tmp_file("pcb_rm_trace");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_trace(0.0, 0.0, 100.0, 0.0, 0.25, "F.Cu", 1);
        assert_eq!(doc.ast().trace_segment_count, 2);

        doc.remove_trace_at(0.0, 0.0, 100.0, 0.0);
        assert_eq!(doc.ast().trace_segment_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_via_increases_count() {
        let path = tmp_file("pcb_add_via");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        assert_eq!(doc.ast().via_count, 0);
        doc.add_via(100.0, 100.0, 0.8, 0.4, 1);
        assert_eq!(doc.ast().via_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_footprint_increases_count() {
        let path = tmp_file("pcb_add_fp");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        assert_eq!(doc.ast().footprint_count, 1);
        doc.add_footprint("Device:C", 120.0, 100.0, "F.Cu", "C1", "100nF");
        assert_eq!(doc.ast().footprint_count, 2);

        let fps = &doc.ast().footprints;
        assert!(fps.iter().any(|f| f.reference.as_deref() == Some("C1")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_footprint_decreases_count() {
        let path = tmp_file("pcb_rm_fp");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:C", 120.0, 100.0, "F.Cu", "C1", "100nF");
        assert_eq!(doc.ast().footprint_count, 2);

        doc.remove_footprint("C1");
        assert_eq!(doc.ast().footprint_count, 1);
        assert!(!doc
            .ast()
            .footprints
            .iter()
            .any(|f| f.reference.as_deref() == Some("C1")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn move_footprint_updates_position() {
        let path = tmp_file("pcb_fp_move");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:R", 10.0, 20.0, "F.Cu", "R2", "100R");
        let before = doc
            .ast()
            .footprints
            .iter()
            .find(|f| f.reference.as_deref() == Some("R2"))
            .unwrap()
            .at;
        assert_eq!(before, Some([10.0, 20.0]));

        doc.move_footprint("R2", 55.5, 66.6, None);
        let fp = doc
            .ast()
            .footprints
            .iter()
            .find(|f| f.reference.as_deref() == Some("R2"))
            .expect("footprint");
        assert_eq!(fp.at, Some([55.5, 66.6]));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn move_footprint_sets_rotation() {
        let path = tmp_file("pcb_fp_move_rot");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:R", 10.0, 20.0, "F.Cu", "R3", "100R");
        doc.move_footprint("R3", 30.0, 40.0, Some(90.0));
        let fp = doc
            .ast()
            .footprints
            .iter()
            .find(|f| f.reference.as_deref() == Some("R3"))
            .expect("footprint");
        assert_eq!(fp.at, Some([30.0, 40.0]));
        assert_eq!(fp.rotation, Some(90.0));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn move_footprint_survives_write_reread() {
        let path = tmp_file("pcb_fp_move_rr");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:R", 10.0, 20.0, "F.Cu", "R4", "100R");
        doc.move_footprint("R4", 77.0, 88.0, Some(180.0));
        doc.write(&path).expect("write");

        let reread = PcbFile::read(&path).expect("reread");
        let fp = reread
            .ast()
            .footprints
            .iter()
            .find(|f| f.reference.as_deref() == Some("R4"))
            .expect("footprint");
        assert_eq!(fp.at, Some([77.0, 88.0]));
        assert_eq!(fp.rotation, Some(180.0));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn move_footprint_noop_wrong_ref() {
        let path = tmp_file("pcb_fp_move_noop");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        let before_count = doc.ast().footprint_count;
        doc.move_footprint("DOESNOTEXIST", 0.0, 0.0, None);
        assert_eq!(doc.ast().footprint_count, before_count);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn pcb_structural_roundtrip() {
        let path = tmp_file("pcb_structural");
        fs::write(&path, MINIMAL_PCB).expect("write");
        let mut doc = PcbFile::read(&path).expect("read");

        doc.add_footprint("Device:C", 120.0, 100.0, "F.Cu", "C1", "100nF")
            .add_trace(100.0, 100.0, 120.0, 100.0, 0.25, "F.Cu", 2)
            .add_via(110.0, 100.0, 0.8, 0.4, 2);

        let out = tmp_file("pcb_structural_out");
        doc.write(&out).expect("write");

        let reread = PcbFile::read(&out).expect("reread");
        assert_eq!(reread.ast().footprint_count, 2);
        assert_eq!(reread.ast().trace_segment_count, 2);
        assert_eq!(reread.ast().via_count, 1);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }
}
