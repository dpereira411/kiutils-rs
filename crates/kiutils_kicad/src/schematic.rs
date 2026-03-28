use std::collections::HashMap;
use std::fs;
use std::path::Path;

use kiutils_sexpr::{parse_one, Atom, CstDocument, Node};

use crate::diagnostic::Diagnostic;
use crate::sections::{parse_paper, parse_title_block, ParsedPaper, ParsedTitleBlock};
use crate::sexpr_edit::{
    atom_quoted, atom_symbol, ensure_root_head_any, find_property_index, list_node,
    mutate_root_and_refresh, paper_standard_node, paper_user_node, remove_property, upsert_node,
    upsert_property_preserve_tail, upsert_scalar, upsert_section_child_scalar,
};
use crate::sexpr_utils::{
    atom_as_f64, atom_as_string, head_of, list_child_head_count, second_atom_bool, second_atom_i32,
    second_atom_string,
};
use crate::symbol::{SymbolLibDocument, SymbolLibFile};
use crate::version_diag::collect_version_diagnostics;
use crate::{Error, UnknownNode, VersionPolicy, WriteMode};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicPaperSummary {
    pub kind: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub orientation: Option<String>,
}

impl From<ParsedPaper> for SchematicPaperSummary {
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
pub struct SchematicTitleBlockSummary {
    pub title: Option<String>,
    pub date: Option<String>,
    pub revision: Option<String>,
    pub company: Option<String>,
    pub comments: Vec<String>,
}

impl From<ParsedTitleBlock> for SchematicTitleBlockSummary {
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

/// Summary of a symbol instance embedded in a schematic.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicSymbolInfo {
    pub reference: Option<String>,
    pub lib_id: Option<String>,
    pub value: Option<String>,
    pub footprint: Option<String>,
    /// All properties as (key, value) pairs.
    pub properties: Vec<(String, String)>,
    /// Schematic placement coordinates (mm).
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub angle: Option<f64>,
    /// Which unit of a multi-unit symbol this instance represents.
    pub unit: Option<i32>,
}

/// Endpoints of a wire segment in a schematic.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicWireSummary {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// A net label placed on a schematic.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicLabelSummary {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    /// One of `"label"`, `"global_label"`, or `"hierarchical_label"`.
    pub label_type: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicAst {
    pub version: Option<i32>,
    pub generator: Option<String>,
    pub generator_version: Option<String>,
    pub uuid: Option<String>,
    pub has_paper: bool,
    pub paper: Option<SchematicPaperSummary>,
    pub has_title_block: bool,
    pub title_block: Option<SchematicTitleBlockSummary>,
    pub has_lib_symbols: bool,
    pub embedded_fonts: Option<bool>,
    pub lib_symbol_count: usize,
    pub symbol_count: usize,
    pub sheet_count: usize,
    pub junction_count: usize,
    pub no_connect_count: usize,
    pub bus_entry_count: usize,
    pub bus_alias_count: usize,
    pub wire_count: usize,
    pub bus_count: usize,
    pub image_count: usize,
    pub text_count: usize,
    pub text_box_count: usize,
    pub label_count: usize,
    pub global_label_count: usize,
    pub hierarchical_label_count: usize,
    pub netclass_flag_count: usize,
    pub polyline_count: usize,
    pub rectangle_count: usize,
    pub circle_count: usize,
    pub arc_count: usize,
    pub rule_area_count: usize,
    pub sheet_instance_count: usize,
    pub symbol_instance_count: usize,
    pub unknown_nodes: Vec<UnknownNode>,
    /// All wire segments with their endpoints.
    pub wires: Vec<SchematicWireSummary>,
    /// All net labels (local, global, hierarchical).
    pub labels: Vec<SchematicLabelSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UpdateFromLibReport {
    pub library_prefix: String,
    pub reference: Option<String>,
    pub updated_symbols: Vec<String>,
    pub skipped_missing_symbols: Vec<String>,
}

/// Controls how [`update_symbols_from_lib_with_options`] syncs instance properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UpdateFromLibOptions {
    /// Replace schematic instance `Value` fields from the library when `true`.
    pub overwrite_value: bool,
}

/// Controls how [`fork_symbol_to_lib`] handles the target library symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ForkSymbolToLibOptions {
    /// Replace an existing target symbol when `true`; otherwise fail on conflict.
    pub overwrite: bool,
}

/// A pin resolved to a net (populated when a symbol library is available).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NetPin {
    pub reference: String,
    pub pin_number: String,
}

/// One electrical net derived from wire connectivity and label placement.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicNet {
    /// Net name from label, or `None` for unnamed wire segments.
    pub name: Option<String>,
    /// Labels that name this net.
    pub labels: Vec<SchematicLabelSummary>,
    /// Pins on this net (empty unless resolved with a symbol lib).
    pub pins: Vec<NetPin>,
}

/// Extracted netlist from a schematic.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SchematicNetlist {
    pub nets: Vec<SchematicNet>,
}

#[derive(Debug, Clone)]
pub struct SchematicDocument {
    ast: SchematicAst,
    cst: CstDocument,
    diagnostics: Vec<Diagnostic>,
    ast_dirty: bool,
    policy: VersionPolicy,
}

impl SchematicDocument {
    pub fn ast(&self) -> &SchematicAst {
        &self.ast
    }

    pub fn ast_mut(&mut self) -> &mut SchematicAst {
        self.ast_dirty = true;
        &mut self.ast
    }

    pub fn cst(&self) -> &CstDocument {
        &self.cst
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn set_version(&mut self, version: i32) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_scalar(items, "version", atom_symbol(version.to_string()), 1)
        })
    }

    pub fn set_generator<S: Into<String>>(&mut self, generator: S) -> &mut Self {
        self.mutate_root_items(|items| {
            upsert_scalar(items, "generator", atom_quoted(generator.into()), 1)
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

    pub fn set_uuid<S: Into<String>>(&mut self, uuid: S) -> &mut Self {
        self.mutate_root_items(|items| upsert_scalar(items, "uuid", atom_quoted(uuid.into()), 1))
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

    pub fn set_embedded_fonts(&mut self, enabled: bool) -> &mut Self {
        let value = if enabled { "yes" } else { "no" };
        self.mutate_root_items(|items| {
            upsert_scalar(items, "embedded_fonts", atom_symbol(value.to_string()), 1)
        })
    }

    /// Return filenames of sub-sheets referenced by `(sheet ...)` nodes.
    ///
    /// The filenames come from the `Sheetfile` property on each sheet node
    /// and are relative to the directory containing this schematic.
    pub fn sheet_filenames(&self) -> Vec<String> {
        let items = match self.cst.nodes.first() {
            Some(Node::List { items, .. }) => items,
            _ => return Vec::new(),
        };
        items
            .iter()
            .skip(1)
            .filter(|node| head_of(node) == Some("sheet"))
            .filter_map(|node| {
                let Node::List {
                    items: sheet_items, ..
                } = node
                else {
                    return None;
                };
                // Look for (property "Sheetfile" "filename.kicad_sch" ...)
                find_property_index(sheet_items, "Sheetfile", 1).and_then(|idx| {
                    if let Some(Node::List {
                        items: prop_items, ..
                    }) = sheet_items.get(idx)
                    {
                        prop_items.get(2).and_then(atom_as_string)
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Return info for all symbol instances in the schematic.
    pub fn symbol_instances(&self) -> Vec<SchematicSymbolInfo> {
        let items = match self.cst.nodes.first() {
            Some(Node::List { items, .. }) => items,
            _ => return Vec::new(),
        };
        items
            .iter()
            .skip(1)
            .filter(|node| head_of(node) == Some("symbol"))
            .map(parse_schematic_symbol_info)
            .collect()
    }

    /// Return `true` if at least one symbol instance has `reference` as its
    /// `Reference` property value (case-sensitive).
    pub fn has_symbol_instance(&self, reference: &str) -> bool {
        self.symbol_instances()
            .into_iter()
            .any(|s| s.reference.as_deref() == Some(reference))
    }

    /// Build a netlist by tracing wire connectivity and matching labels.
    ///
    /// Each [`SchematicNet`] has a `name` (from the first label on that net, with
    /// `global_label` preferred over `hierarchical_label` over `label`) and a list
    /// of the labels that name it.  Pins are left empty; use `netlist_with_lib` when
    /// you have the symbol library available.
    pub fn netlist(&self) -> SchematicNetlist {
        build_netlist(&self.ast.wires, &self.ast.labels)
    }

    /// Upsert a property on every symbol instance matching `reference`.
    pub fn upsert_symbol_instance_property<R: Into<String>, K: Into<String>, V: Into<String>>(
        &mut self,
        reference: R,
        key: K,
        value: V,
    ) -> &mut Self {
        let reference = reference.into();
        let key = key.into();
        let value = value.into();
        self.mutate_root_items(|items| {
            let indices = find_schematic_symbol_indices_by_reference(items, &reference);
            let mut changed = false;
            for idx in indices {
                if let Some(Node::List {
                    items: sym_items, ..
                }) = items.get_mut(idx)
                {
                    if upsert_property_preserve_tail(sym_items, &key, &value, 1) {
                        changed = true;
                    }
                }
            }
            changed
        })
    }

    /// Remove a property from every symbol instance matching `reference`.
    pub fn remove_symbol_instance_property<R: Into<String>, K: Into<String>>(
        &mut self,
        reference: R,
        key: K,
    ) -> &mut Self {
        let reference = reference.into();
        let key = key.into();
        self.mutate_root_items(|items| {
            let indices = find_schematic_symbol_indices_by_reference(items, &reference);
            let mut changed = false;
            for idx in indices {
                if let Some(Node::List {
                    items: sym_items, ..
                }) = items.get_mut(idx)
                {
                    if remove_property(sym_items, &key, 1) {
                        changed = true;
                    }
                }
            }
            changed
        })
    }

    /// Add a new symbol instance to the schematic.
    pub fn add_symbol_instance<L, R, V>(
        &mut self,
        lib_id: L,
        reference: R,
        value: V,
        x: f64,
        y: f64,
    ) -> &mut Self
    where
        L: Into<String>,
        R: Into<String>,
        V: Into<String>,
    {
        let node = symbol_instance_node(&lib_id.into(), &reference.into(), &value.into(), x, y);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Remove all symbol instances whose "Reference" property matches `reference`.
    pub fn remove_symbol_instance<R: Into<String>>(&mut self, reference: R) -> &mut Self {
        let reference = reference.into();
        self.mutate_root_items(|items| {
            let indices = find_schematic_symbol_indices_by_reference(items, &reference);
            if indices.is_empty() {
                return false;
            }
            for idx in indices.into_iter().rev() {
                items.remove(idx);
            }
            true
        })
    }

    /// Replace the `lib_id` of every symbol instance matching `reference`.
    pub fn set_symbol_lib_id<R, L>(&mut self, reference: R, new_lib_id: L) -> &mut Self
    where
        R: Into<String>,
        L: Into<String>,
    {
        let reference = reference.into();
        let new_lib_id = new_lib_id.into();
        self.mutate_root_items(|items| {
            let indices = find_schematic_symbol_indices_by_reference(items, &reference);
            let mut changed = false;
            for idx in indices {
                if let Some(Node::List {
                    items: sym_items, ..
                }) = items.get_mut(idx)
                {
                    if upsert_scalar(sym_items, "lib_id", atom_quoted(new_lib_id.clone()), 1) {
                        changed = true;
                    }
                }
            }
            changed
        })
    }

    /// Append a wire segment to the schematic.
    pub fn add_wire(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> &mut Self {
        let node = wire_node(x1, y1, x2, y2);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Remove all wire segments whose endpoints exactly match the given coordinates.
    pub fn remove_wire_at(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> &mut Self {
        self.mutate_root_items(|items| {
            let before = items.len();
            items.retain(|node| !wire_pts_match(node, x1, y1, x2, y2));
            items.len() != before
        })
    }

    /// Remove all net labels (label/global_label/hierarchical_label) with the given text.
    pub fn remove_label_by_name(&mut self, name: &str) -> &mut Self {
        self.mutate_root_items(|items| {
            let before = items.len();
            items.retain(|node| !label_name_matches(node, name));
            items.len() != before
        })
    }

    /// Rename all net labels with `old_name` text to `new_name`.
    pub fn rename_label(&mut self, old_name: &str, new_name: &str) -> &mut Self {
        self.mutate_root_items(|items| {
            let mut changed = false;
            for node in items.iter_mut() {
                if label_name_matches(node, old_name) {
                    if let Node::List {
                        items: label_items, ..
                    } = node
                    {
                        if label_items.len() > 1 {
                            label_items[1] = atom_quoted(new_name);
                            changed = true;
                        }
                    }
                }
            }
            changed
        })
    }

    /// Add a net label at the given position.
    pub fn add_label<T: Into<String>>(&mut self, text: T, x: f64, y: f64, angle: f64) -> &mut Self {
        let node = label_node(&text.into(), x, y, angle);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Add a global label at the given position.
    ///
    /// `shape` is one of: `input`, `output`, `bidirectional`, `tri_state`, `passive`,
    /// `power_in`, `power_out`.
    pub fn add_global_label<T, S>(
        &mut self,
        text: T,
        shape: S,
        x: f64,
        y: f64,
        angle: f64,
    ) -> &mut Self
    where
        T: Into<String>,
        S: Into<String>,
    {
        let node = global_label_node(&text.into(), &shape.into(), x, y, angle);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Add a junction dot at the given position (marks an intentional wire crossing).
    pub fn add_junction(&mut self, x: f64, y: f64) -> &mut Self {
        let node = junction_node(x, y);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
    }

    /// Add a no-connect marker at the given position (marks an intentionally unconnected pin).
    pub fn add_no_connect(&mut self, x: f64, y: f64) -> &mut Self {
        let node = no_connect_node(x, y);
        self.mutate_root_items(|items| {
            items.push(node);
            true
        })
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
        self.validate_embedded_symbol_cache()?;
        match mode {
            WriteMode::Lossless => fs::write(path, self.cst.to_lossless_string())?,
            WriteMode::Canonical => fs::write(path, self.cst.to_canonical_string())?,
        }
        Ok(())
    }

    /// Return a clone of the `lib_symbols` entry whose name matches `lib_id`.
    pub(crate) fn extract_embedded_lib_symbol(&self, lib_id: &str) -> Option<Node> {
        let items = match self.cst.nodes.first() {
            Some(Node::List { items, .. }) => items,
            _ => return None,
        };
        let lib_symbols = items
            .iter()
            .skip(1)
            .find(|n| head_of(n) == Some("lib_symbols"))?;
        let Node::List {
            items: ls_items, ..
        } = lib_symbols
        else {
            return None;
        };
        ls_items
            .iter()
            .find(|n| {
                head_of(n) == Some("symbol")
                    && match n {
                        Node::List { items: si, .. } => {
                            si.get(1).and_then(atom_as_string).as_deref() == Some(lib_id)
                        }
                        _ => false,
                    }
            })
            .cloned()
    }

    /// Add or replace a symbol in the schematic's embedded `lib_symbols` section.
    pub(crate) fn upsert_embedded_lib_symbol(&mut self, node: Node) -> &mut Self {
        self.mutate_root_items(|items| {
            let name = match &node {
                Node::List { items: si, .. } => si.get(1).and_then(atom_as_string),
                _ => None,
            };
            let Some(name) = name else {
                return false;
            };
            let Some(ls_idx) = items
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, n)| head_of(n) == Some("lib_symbols"))
                .map(|(i, _)| i)
            else {
                return false;
            };
            let Node::List {
                items: ls_items, ..
            } = &mut items[ls_idx]
            else {
                return false;
            };
            let existing = ls_items
                .iter()
                .enumerate()
                .find(|(_, n)| {
                    head_of(n) == Some("symbol")
                        && match n {
                            Node::List { items: si, .. } => {
                                si.get(1).and_then(atom_as_string).as_deref() == Some(&name)
                            }
                            _ => false,
                        }
                })
                .map(|(i, _)| i);
            if let Some(idx) = existing {
                ls_items[idx] = node;
            } else {
                ls_items.push(node);
            }
            true
        })
    }

    /// Remove the `lib_symbols` entry whose name matches `lib_id`.
    pub(crate) fn remove_embedded_lib_symbol(&mut self, lib_id: &str) -> &mut Self {
        let lib_id = lib_id.to_string();
        self.mutate_root_items(|items| {
            let Some(ls_idx) = items
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, n)| head_of(n) == Some("lib_symbols"))
                .map(|(i, _)| i)
            else {
                return false;
            };
            let Node::List {
                items: ls_items, ..
            } = &mut items[ls_idx]
            else {
                return false;
            };
            let before = ls_items.len();
            ls_items.retain(|n| {
                !(head_of(n) == Some("symbol")
                    && match n {
                        Node::List { items: si, .. } => {
                            si.get(1).and_then(atom_as_string).as_deref() == Some(lib_id.as_str())
                        }
                        _ => false,
                    })
            });
            ls_items.len() != before
        })
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
            move |_cst, ast| collect_version_diagnostics(ast.version, &policy),
        );
        self.ast_dirty = false;
        self
    }

    /// Return the distinct `lib_id` values referenced by placed symbols that do not
    /// have a matching embedded entry in the schematic's `lib_symbols` cache.
    pub fn missing_embedded_lib_symbol_lib_ids(&self) -> Vec<String> {
        let mut missing = Vec::new();
        for lib_id in self.symbol_instances().into_iter().filter_map(|s| s.lib_id) {
            if !missing.contains(&lib_id) && self.extract_embedded_lib_symbol(&lib_id).is_none() {
                missing.push(lib_id);
            }
        }
        missing
    }

    /// Validate that every placed symbol's `lib_id` resolves to an embedded
    /// `lib_symbols` cache entry, which KiCad expects when opening a schematic.
    pub fn validate_embedded_symbol_cache(&self) -> Result<(), Error> {
        let missing = self.missing_embedded_lib_symbol_lib_ids();
        if missing.is_empty() {
            return Ok(());
        }
        Err(Error::Validation(format!(
            "schematic references lib_id(s) without embedded lib_symbols entries: {}",
            missing.join(", ")
        )))
    }
}

pub struct SchematicFile;

impl SchematicFile {
    pub fn read<P: AsRef<Path>>(path: P) -> Result<SchematicDocument, Error> {
        Self::read_with_policy(path, VersionPolicy::default())
    }

    pub fn read_with_policy<P: AsRef<Path>>(
        path: P,
        policy: VersionPolicy,
    ) -> Result<SchematicDocument, Error> {
        let raw = fs::read_to_string(path)?;
        let cst = parse_one(&raw)?;
        ensure_root_head_any(&cst, &["kicad_sch"])?;
        let ast = parse_ast(&cst);
        let diagnostics = collect_version_diagnostics(ast.version, &policy);
        Ok(SchematicDocument {
            ast,
            cst,
            diagnostics,
            ast_dirty: false,
            policy,
        })
    }
}

/// Find indices of root-level `(symbol ...)` nodes whose "Reference" property matches.
fn find_schematic_symbol_indices_by_reference(items: &[Node], reference: &str) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, node)| {
            if head_of(node) != Some("symbol") {
                return false;
            }
            let Node::List {
                items: sym_items, ..
            } = node
            else {
                return false;
            };
            if let Some(prop_idx) = find_property_index(sym_items, "Reference", 1) {
                if let Some(Node::List {
                    items: prop_items, ..
                }) = sym_items.get(prop_idx)
                {
                    return prop_items.get(2).and_then(atom_as_string).as_deref()
                        == Some(reference);
                }
            }
            false
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Extract property value from a symbol node's items.
fn get_property_value(sym_items: &[Node], key: &str) -> Option<String> {
    find_property_index(sym_items, key, 1).and_then(|idx| {
        if let Some(Node::List {
            items: prop_items, ..
        }) = sym_items.get(idx)
        {
            prop_items.get(2).and_then(atom_as_string)
        } else {
            None
        }
    })
}

fn property_key(node: &Node) -> Option<String> {
    let Node::List { items, .. } = node else {
        return None;
    };
    if head_of(node) != Some("property") {
        return None;
    }
    items.get(1).and_then(atom_as_string)
}

fn is_preserved_instance_property(key: &str) -> bool {
    matches!(key, "Reference" | "Value")
}

fn is_symbol_metadata_property(key: &str) -> bool {
    matches!(key, "ki_keywords" | "ki_fp_filters")
}

fn should_preserve_instance_property(key: &str, options: UpdateFromLibOptions) -> bool {
    if key == "Value" && options.overwrite_value {
        false
    } else {
        is_preserved_instance_property(key)
    }
}

fn find_schematic_symbol_indices_by_lib_id(items: &[Node], lib_id: &str) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, node)| {
            if head_of(node) != Some("symbol") {
                return false;
            }
            let Node::List {
                items: sym_items, ..
            } = node
            else {
                return false;
            };
            sym_items
                .iter()
                .skip(1)
                .find(|n| head_of(n) == Some("lib_id"))
                .and_then(second_atom_string)
                .as_deref()
                == Some(lib_id)
        })
        .map(|(idx, _)| idx)
        .collect()
}

fn property_insertion_index(sym_items: &[Node]) -> usize {
    sym_items
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, child)| head_of(child) == Some("pin"))
        .map(|(idx, _)| idx)
        .unwrap_or(sym_items.len())
}

fn sync_symbol_instance_properties_from_lib(
    sym_items: &mut Vec<Node>,
    lib_symbol: &Node,
    options: UpdateFromLibOptions,
) -> bool {
    let Node::List {
        items: lib_items, ..
    } = lib_symbol
    else {
        return false;
    };
    let lib_props: Vec<Node> = lib_items
        .iter()
        .skip(2)
        .filter(|child| head_of(child) == Some("property"))
        .filter(|child| {
            property_key(child)
                .map(|key| {
                    !should_preserve_instance_property(&key, options)
                        && !is_symbol_metadata_property(&key)
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let first_prop = sym_items
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, child)| head_of(child) == Some("property"))
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| property_insertion_index(sym_items));
    let after_prop = sym_items
        .iter()
        .enumerate()
        .skip(1)
        .rev()
        .find(|(_, child)| head_of(child) == Some("property"))
        .map(|(idx, _)| idx + 1)
        .unwrap_or(first_prop);

    let mut next_items = Vec::with_capacity(sym_items.len() + lib_props.len());
    next_items.push(sym_items[0].clone());
    next_items.extend(
        sym_items
            .iter()
            .skip(1)
            .take(first_prop.saturating_sub(1))
            .cloned(),
    );
    next_items.extend(
        sym_items[first_prop..after_prop]
            .iter()
            .filter(|child| {
                property_key(child)
                    .map(|key| should_preserve_instance_property(&key, options))
                    .unwrap_or(false)
            })
            .cloned(),
    );
    next_items.extend(lib_props);
    next_items.extend(sym_items.iter().skip(after_prop).cloned());

    if *sym_items == next_items {
        false
    } else {
        *sym_items = next_items;
        true
    }
}

fn parse_schematic_symbol_info(node: &Node) -> SchematicSymbolInfo {
    let Node::List { items, .. } = node else {
        return SchematicSymbolInfo {
            reference: None,
            lib_id: None,
            value: None,
            footprint: None,
            properties: Vec::new(),
            x: None,
            y: None,
            angle: None,
            unit: None,
        };
    };

    let lib_id = items
        .iter()
        .skip(1)
        .find(|n| head_of(n) == Some("lib_id"))
        .and_then(second_atom_string);

    let (x, y, angle) = items
        .iter()
        .skip(1)
        .find(|n| head_of(n) == Some("at"))
        .and_then(|at| {
            let Node::List {
                items: at_items, ..
            } = at
            else {
                return None;
            };
            let x = at_items.get(1).and_then(atom_as_f64)?;
            let y = at_items.get(2).and_then(atom_as_f64)?;
            let angle = at_items.get(3).and_then(atom_as_f64).unwrap_or(0.0);
            Some((x, y, angle))
        })
        .map(|(x, y, a)| (Some(x), Some(y), Some(a)))
        .unwrap_or((None, None, None));

    let unit = items
        .iter()
        .skip(1)
        .find(|n| head_of(n) == Some("unit"))
        .and_then(second_atom_i32);

    let reference = get_property_value(items, "Reference");
    let value = get_property_value(items, "Value");
    let footprint = get_property_value(items, "Footprint");

    let properties: Vec<(String, String)> = items
        .iter()
        .skip(1)
        .filter(|n| head_of(n) == Some("property"))
        .filter_map(|n| {
            let Node::List {
                items: prop_items, ..
            } = n
            else {
                return None;
            };
            let key = prop_items.get(1).and_then(atom_as_string)?;
            let val = prop_items
                .get(2)
                .and_then(atom_as_string)
                .unwrap_or_default();
            Some((key, val))
        })
        .collect();

    SchematicSymbolInfo {
        reference,
        lib_id,
        value,
        footprint,
        properties,
        x,
        y,
        angle,
        unit,
    }
}

fn symbol_local_name(lib_id: &str) -> &str {
    lib_id
        .split_once(':')
        .map(|(_, local)| local)
        .unwrap_or(lib_id)
}

/// Update the library prefix of a symbol node and all its sub-symbol descendants.
///
/// The symbol content (pins, graphics, property values) is preserved unchanged.
/// KiCad stores the top-level embedded symbol name as the full lib_id, but unit
/// sub-symbols usually use the local symbol name only, such as `"R_0_1"` for
/// `"Device:R"` or `"LED_0_1"` for `"User Library:LED"`.
fn reprefix_lib_symbol_node(node: Node, old_lib_id: &str, new_lib_id: &str) -> Node {
    rename_lib_symbol_node(node, old_lib_id, new_lib_id)
}

/// Rename a symbol node and all nested unit/sub-symbol names rooted at `old_lib_id`.
fn rename_lib_symbol_node(node: Node, old_lib_id: &str, new_lib_id: &str) -> Node {
    let Node::List { items, span } = node else {
        return node;
    };
    let old_prefix_with_sep = format!("{old_lib_id}_");
    let old_local_name = symbol_local_name(old_lib_id);
    let new_local_name = symbol_local_name(new_lib_id);
    let old_local_prefix_with_sep = format!("{old_local_name}_");
    let items = items
        .into_iter()
        .map(|child| match &child {
            Node::Atom {
                atom: Atom::Quoted(s),
                span: s_span,
            } => {
                if s == old_lib_id {
                    Node::Atom {
                        atom: Atom::Quoted(new_lib_id.to_string()),
                        span: *s_span,
                    }
                } else if s.starts_with(&old_prefix_with_sep) {
                    let suffix = &s[old_lib_id.len()..]; // includes leading "_"
                    Node::Atom {
                        atom: Atom::Quoted(format!("{new_lib_id}{suffix}")),
                        span: *s_span,
                    }
                } else if s.starts_with(&old_local_prefix_with_sep) {
                    let suffix = &s[old_local_name.len()..]; // includes leading "_"
                    Node::Atom {
                        atom: Atom::Quoted(format!("{new_local_name}{suffix}")),
                        span: *s_span,
                    }
                } else {
                    child
                }
            }
            Node::List { .. } if head_of(&child) == Some("symbol") => {
                rename_lib_symbol_node(child, old_lib_id, new_lib_id)
            }
            _ => child,
        })
        .collect();
    Node::List { items, span }
}

fn fork_target_lib_id(
    lib_path: &Path,
    target_symbol_name: &str,
) -> Result<(String, String), Error> {
    let lib_stem = lib_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Validation("invalid lib_path filename".to_string()))?;
    let target_symbol_name = target_symbol_name.trim();
    if target_symbol_name.is_empty() {
        return Err(Error::Validation(
            "target symbol name must not be empty".to_string(),
        ));
    }
    Ok((
        lib_stem.to_string(),
        format!("{lib_stem}:{target_symbol_name}"),
    ))
}

fn target_lib_id_from_library_name(
    library_name: &str,
    target_symbol_name: &str,
) -> Result<String, Error> {
    let library_name = library_name.trim();
    let target_symbol_name = normalize_symbol_name_input(library_name, target_symbol_name)?;
    if library_name.is_empty() {
        return Err(Error::Validation(
            "library name must not be empty".to_string(),
        ));
    }
    Ok(format!("{library_name}:{target_symbol_name}"))
}

fn normalize_symbol_name_input(
    library_name: &str,
    target_symbol_name: &str,
) -> Result<String, Error> {
    let target_symbol_name = target_symbol_name.trim();
    if target_symbol_name.is_empty() {
        return Err(Error::Validation(
            "target symbol name must not be empty".to_string(),
        ));
    }
    if let Some((prefix, local_name)) = target_symbol_name.split_once(':') {
        if prefix == library_name {
            if local_name.trim().is_empty() {
                return Err(Error::Validation(
                    "target symbol name must not be empty".to_string(),
                ));
            }
            Ok(local_name.trim().to_string())
        } else {
            Err(Error::Validation(format!(
                "symbol name {target_symbol_name:?} does not belong to library {library_name:?}"
            )))
        }
    } else {
        Ok(target_symbol_name.to_string())
    }
}

fn resolve_symbol_from_library(
    lib: &SymbolLibDocument,
    target_lib_id: &str,
    target_symbol_name: &str,
) -> Option<Node> {
    if let Some(node) = lib.extract_lib_symbol(target_lib_id) {
        return Some(node);
    }
    let bare_name = symbol_local_name(target_symbol_name);
    lib.extract_lib_symbol(bare_name)
        .map(|node| normalize_library_symbol_node(node, bare_name, target_lib_id))
}

fn normalize_library_symbol_node(node: Node, old_symbol_name: &str, target_lib_id: &str) -> Node {
    let Node::List { items, span } = node else {
        return node;
    };
    let old_local_prefix_with_sep = format!("{old_symbol_name}_");
    let new_local_name = symbol_local_name(target_lib_id);
    let items = items
        .into_iter()
        .enumerate()
        .map(|(idx, child)| match child {
            Node::Atom {
                atom: Atom::Quoted(s),
                span: s_span,
            } => {
                if idx == 1 && s == old_symbol_name {
                    Node::Atom {
                        atom: Atom::Quoted(target_lib_id.to_string()),
                        span: s_span,
                    }
                } else if s.starts_with(&old_local_prefix_with_sep) {
                    let suffix = &s[old_symbol_name.len()..];
                    Node::Atom {
                        atom: Atom::Quoted(format!("{new_local_name}{suffix}")),
                        span: s_span,
                    }
                } else {
                    Node::Atom {
                        atom: Atom::Quoted(s),
                        span: s_span,
                    }
                }
            }
            Node::List { .. } => {
                normalize_library_symbol_node(child, old_symbol_name, target_lib_id)
            }
            _ => child,
        })
        .collect();
    Node::List { items, span }
}

/// Copy a symbol from a schematic's embedded `lib_symbols` section into a
/// `.kicad_sym` library file.
///
/// # What it does
///
/// 1. Reads the schematic at `sch_path` and finds the symbol instance for
///    `reference` (e.g. `"R1"`).
/// 2. Derives the target lib_id from the library filename stem and the symbol
///    name — e.g. `"Device:R"` pushed to `"MyLib.kicad_sym"` becomes `"MyLib:R"`.
/// 3. Extracts the embedded symbol definition from `lib_symbols` and updates its
///    name prefix to match the target library (content is unchanged).
/// 4. Writes the symbol into `lib_path` (upsert — replaces if already present).
/// 5. If the target lib_id differs from the source, also updates the schematic:
///    - The instance's `lib_id` is changed to the target.
///    - The `lib_symbols` section is updated with the new entry.
///    - The old `lib_symbols` entry is removed if no other instance still uses it.
///
/// Returns the final lib_id written to the library (e.g. `"MyLib:R"`).
pub fn push_symbol_to_lib<S, R, L>(sch_path: S, reference: R, lib_path: L) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<std::path::Path>,
{
    let sch_path = sch_path.as_ref();
    let reference = reference.as_ref();
    let lib_path = lib_path.as_ref();

    // Derive target library name from the filename stem (e.g. "MyLib.kicad_sym" → "MyLib")
    let lib_stem = lib_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Validation("invalid lib_path filename".to_string()))?;

    let mut sch = SchematicFile::read(sch_path)?;

    // Find the lib_id for this reference (e.g. "Device:R")
    let lib_id = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some(reference))
        .and_then(|s| s.lib_id)
        .ok_or_else(|| {
            Error::Validation(format!("reference {reference:?} not found in schematic"))
        })?;

    // Build target lib_id: swap library prefix, keep symbol name
    // e.g. "Device:R" + "MyLib" → "MyLib:R"
    let symbol_name = lib_id.splitn(2, ':').nth(1).unwrap_or(&lib_id);
    let target_lib_id = format!("{lib_stem}:{symbol_name}");

    // Extract the embedded definition from lib_symbols
    let node = sch.extract_embedded_lib_symbol(&lib_id).ok_or_else(|| {
        Error::Validation(format!(
            "no embedded lib_symbol for {lib_id:?} in schematic"
        ))
    })?;

    // Update name prefix in the node if moving to a different library
    let node = if target_lib_id != lib_id {
        reprefix_lib_symbol_node(node, &lib_id, &target_lib_id)
    } else {
        node
    };

    // Write the symbol into the library file
    let mut lib = SymbolLibFile::read(lib_path)?;
    lib.upsert_lib_symbol(node.clone());
    lib.write(lib_path)?;

    // Update the schematic only when moving to a different library
    if target_lib_id != lib_id {
        sch.set_symbol_lib_id(reference, &target_lib_id);
        sch.upsert_embedded_lib_symbol(node);

        // Clean up the old lib_symbols entry if no instance still references it
        let still_used = sch
            .symbol_instances()
            .into_iter()
            .any(|s| s.lib_id.as_deref() == Some(&lib_id));
        if !still_used {
            sch.remove_embedded_lib_symbol(&lib_id);
        }

        sch.write(sch_path)?;
    }

    Ok(target_lib_id)
}

/// Rename a placed schematic symbol's `lib_id` using the schematic's embedded
/// `lib_symbols` cache as the source of truth.
///
/// If `new_lib_id` is not already present in `lib_symbols`, this clones the
/// current embedded symbol definition under the new name, repoints the targeted
/// instance, and removes the old embedded symbol if no instances still use it.
///
/// If `new_lib_id` is already present in `lib_symbols`, the existing embedded
/// target is preserved and only the targeted instance is repointed.
pub fn rename_symbol_in_schematic<S, R, L>(
    sch_path: S,
    reference: R,
    new_lib_id: L,
) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<str>,
{
    let sch_path = sch_path.as_ref();
    let reference = reference.as_ref();
    let new_lib_id = new_lib_id.as_ref();

    let mut sch = SchematicFile::read(sch_path)?;
    let old_lib_id = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some(reference))
        .and_then(|s| s.lib_id)
        .ok_or_else(|| {
            Error::Validation(format!("reference {reference:?} not found in schematic"))
        })?;

    if old_lib_id == new_lib_id {
        sch.write(sch_path)?;
        return Ok(old_lib_id);
    }

    let has_target_embedded = sch.extract_embedded_lib_symbol(new_lib_id).is_some();
    if !has_target_embedded {
        let node = sch
            .extract_embedded_lib_symbol(&old_lib_id)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "no embedded lib_symbol for {old_lib_id:?} in schematic"
                ))
            })?;
        let renamed = rename_lib_symbol_node(node, &old_lib_id, new_lib_id);
        sch.upsert_embedded_lib_symbol(renamed);
    }

    sch.set_symbol_lib_id(reference, new_lib_id);

    let still_used = sch
        .symbol_instances()
        .into_iter()
        .any(|s| s.lib_id.as_deref() == Some(&old_lib_id));
    if !still_used {
        sch.remove_embedded_lib_symbol(&old_lib_id);
    }

    sch.write(sch_path)?;
    Ok(new_lib_id.to_string())
}

/// Replace one placed schematic symbol from a `.kicad_sym` library entry.
///
/// This is the library-authoritative inverse of `push_symbol_to_lib`: it loads
/// the target symbol from `lib_path`, embeds that body into the schematic under
/// the derived `lib_id`, repoints only `reference`, refreshes instance
/// properties from the library according to `options`, and removes the old
/// embedded symbol if no instances still use it.
pub fn replace_symbol_from_lib_with_library_name_with_options<S, R, L, K, N>(
    sch_path: S,
    reference: R,
    lib_path: L,
    library_name: K,
    target_symbol_name: N,
    options: UpdateFromLibOptions,
) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<std::path::Path>,
    K: AsRef<str>,
    N: AsRef<str>,
{
    let sch_path = sch_path.as_ref();
    let reference = reference.as_ref();
    let lib_path = lib_path.as_ref();
    let target_lib_id =
        target_lib_id_from_library_name(library_name.as_ref(), target_symbol_name.as_ref())?;

    let mut sch = SchematicFile::read(sch_path)?;
    let old_lib_id = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some(reference))
        .and_then(|s| s.lib_id)
        .ok_or_else(|| {
            Error::Validation(format!("reference {reference:?} not found in schematic"))
        })?;

    let lib = SymbolLibFile::read(lib_path)?;
    let target_node =
        resolve_symbol_from_library(&lib, &target_lib_id, target_symbol_name.as_ref()).ok_or_else(
            || {
                Error::Validation(format!(
                    "symbol {target_lib_id:?} (or bare name {:?}) not found in library {}",
                    symbol_local_name(target_symbol_name.as_ref()),
                    lib_path.display()
                ))
            },
        )?;

    sch.upsert_embedded_lib_symbol(target_node.clone());
    sch.set_symbol_lib_id(reference, &target_lib_id);
    sch.mutate_root_items(|items| {
        let indices = find_schematic_symbol_indices_by_reference(items, reference);
        let mut changed = false;
        for idx in indices {
            if let Some(Node::List {
                items: sym_items, ..
            }) = items.get_mut(idx)
            {
                if sync_symbol_instance_properties_from_lib(sym_items, &target_node, options) {
                    changed = true;
                }
            }
        }
        changed
    });

    let still_used = sch
        .symbol_instances()
        .into_iter()
        .any(|s| s.lib_id.as_deref() == Some(&old_lib_id));
    if !still_used && old_lib_id != target_lib_id {
        sch.remove_embedded_lib_symbol(&old_lib_id);
    }

    sch.write(sch_path)?;
    Ok(target_lib_id)
}

pub fn replace_symbol_from_lib_with_options<S, R, L, N>(
    sch_path: S,
    reference: R,
    lib_path: L,
    target_symbol_name: N,
    options: UpdateFromLibOptions,
) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<std::path::Path>,
    N: AsRef<str>,
{
    let (library_prefix, _) = fork_target_lib_id(lib_path.as_ref(), target_symbol_name.as_ref())?;
    replace_symbol_from_lib_with_library_name_with_options(
        sch_path,
        reference,
        lib_path,
        library_prefix,
        target_symbol_name,
        options,
    )
}

pub fn replace_symbol_from_lib<S, R, L, N>(
    sch_path: S,
    reference: R,
    lib_path: L,
    target_symbol_name: N,
) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<std::path::Path>,
    N: AsRef<str>,
{
    replace_symbol_from_lib_with_options(
        sch_path,
        reference,
        lib_path,
        target_symbol_name,
        UpdateFromLibOptions::default(),
    )
}

pub fn replace_symbol_from_lib_with_library_name<S, R, L, K, N>(
    sch_path: S,
    reference: R,
    lib_path: L,
    library_name: K,
    target_symbol_name: N,
) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<std::path::Path>,
    K: AsRef<str>,
    N: AsRef<str>,
{
    replace_symbol_from_lib_with_library_name_with_options(
        sch_path,
        reference,
        lib_path,
        library_name,
        target_symbol_name,
        UpdateFromLibOptions::default(),
    )
}

/// Fork a placed schematic symbol into a target library under an explicit symbol name.
///
/// The target library prefix is derived from `lib_path`'s filename stem. Only the
/// provided `reference` is repointed to the new symbol; other instances using the
/// original `lib_id` are left unchanged.
///
/// Returns the final `lib_id` written to the library, such as `"MyLib:FastDiode"`.
pub fn fork_symbol_to_lib<S, R, L, N>(
    sch_path: S,
    reference: R,
    lib_path: L,
    target_symbol_name: N,
    options: ForkSymbolToLibOptions,
) -> Result<String, Error>
where
    S: AsRef<std::path::Path>,
    R: AsRef<str>,
    L: AsRef<std::path::Path>,
    N: AsRef<str>,
{
    let sch_path = sch_path.as_ref();
    let reference = reference.as_ref();
    let lib_path = lib_path.as_ref();
    let (_library_prefix, target_lib_id) =
        fork_target_lib_id(lib_path, target_symbol_name.as_ref())?;

    let mut sch = SchematicFile::read(sch_path)?;
    let lib_id = sch
        .symbol_instances()
        .into_iter()
        .find(|s| s.reference.as_deref() == Some(reference))
        .and_then(|s| s.lib_id)
        .ok_or_else(|| {
            Error::Validation(format!("reference {reference:?} not found in schematic"))
        })?;

    let node = sch.extract_embedded_lib_symbol(&lib_id).ok_or_else(|| {
        Error::Validation(format!(
            "no embedded lib_symbol for {lib_id:?} in schematic"
        ))
    })?;

    let node = if target_lib_id != lib_id {
        rename_lib_symbol_node(node, &lib_id, &target_lib_id)
    } else {
        node
    };

    let mut lib = SymbolLibFile::read(lib_path)?;
    if !options.overwrite && target_lib_id != lib_id && lib.has_symbol(&target_lib_id) {
        return Err(Error::Validation(format!(
            "target symbol {target_lib_id:?} already exists in library"
        )));
    }
    if !options.overwrite && target_lib_id == lib_id && lib.has_symbol(&target_lib_id) {
        return Err(Error::Validation(format!(
            "target symbol {target_lib_id:?} already exists in library; pass overwrite to replace it"
        )));
    }
    lib.upsert_lib_symbol(node.clone());
    lib.write(lib_path)?;

    if target_lib_id != lib_id {
        sch.set_symbol_lib_id(reference, &target_lib_id);
        sch.upsert_embedded_lib_symbol(node);

        let still_used = sch
            .symbol_instances()
            .into_iter()
            .any(|s| s.lib_id.as_deref() == Some(&lib_id));
        if !still_used {
            sch.remove_embedded_lib_symbol(&lib_id);
        }

        sch.write(sch_path)?;
    }

    Ok(target_lib_id)
}

pub fn update_symbols_from_lib<S, L>(
    sch_path: S,
    lib_path: L,
    reference: Option<&str>,
    update_all: bool,
) -> Result<UpdateFromLibReport, Error>
where
    S: AsRef<std::path::Path>,
    L: AsRef<std::path::Path>,
{
    update_symbols_from_lib_with_options(
        sch_path,
        lib_path,
        reference,
        update_all,
        UpdateFromLibOptions::default(),
    )
}

/// Refresh embedded schematic symbols from a `.kicad_sym` library file and optionally
/// overwrite instance `Value` fields from the library definition.
pub fn update_symbols_from_lib_with_options<S, L>(
    sch_path: S,
    lib_path: L,
    reference: Option<&str>,
    update_all: bool,
    options: UpdateFromLibOptions,
) -> Result<UpdateFromLibReport, Error>
where
    S: AsRef<std::path::Path>,
    L: AsRef<std::path::Path>,
{
    let sch_path = sch_path.as_ref();
    let lib_path = lib_path.as_ref();
    let library_prefix = lib_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Validation("invalid lib_path filename".to_string()))?
        .to_string();

    let mut sch = SchematicFile::read(sch_path)?;
    let lib = SymbolLibFile::read(lib_path)?;
    let embedded_names = if update_all {
        let names = embedded_lib_symbol_names_with_prefix(&sch, &library_prefix);
        if names.is_empty() {
            return Err(Error::Validation(format!(
                "no embedded lib_symbols entries found for library {library_prefix:?}"
            )));
        }
        names
    } else {
        let reference = reference.ok_or_else(|| {
            Error::Validation("missing reference; pass a reference or use --all".to_string())
        })?;
        let lib_id = sch
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some(reference))
            .and_then(|s| s.lib_id)
            .ok_or_else(|| {
                Error::Validation(format!("reference {reference:?} not found in schematic"))
            })?;
        if !lib_id.starts_with(&format!("{library_prefix}:")) {
            return Err(Error::Validation(format!(
                "reference {reference:?} uses lib_id {lib_id:?}, not library {library_prefix:?}"
            )));
        }
        vec![lib_id]
    };

    let mut updated_symbols = Vec::new();
    let mut skipped_missing_symbols = Vec::new();

    for symbol_name in embedded_names {
        if let Some(node) = lib.extract_lib_symbol(&symbol_name) {
            sch.upsert_embedded_lib_symbol(node.clone());
            if update_all {
                sch.mutate_root_items(|items| {
                    let indices = find_schematic_symbol_indices_by_lib_id(items, &symbol_name);
                    let mut changed = false;
                    for idx in indices {
                        if let Some(Node::List {
                            items: sym_items, ..
                        }) = items.get_mut(idx)
                        {
                            if sync_symbol_instance_properties_from_lib(sym_items, &node, options) {
                                changed = true;
                            }
                        }
                    }
                    changed
                });
            } else if let Some(target_reference) = reference {
                sch.mutate_root_items(|items| {
                    let indices =
                        find_schematic_symbol_indices_by_reference(items, target_reference);
                    let mut changed = false;
                    for idx in indices {
                        if let Some(Node::List {
                            items: sym_items, ..
                        }) = items.get_mut(idx)
                        {
                            if sync_symbol_instance_properties_from_lib(sym_items, &node, options) {
                                changed = true;
                            }
                        }
                    }
                    changed
                });
            }
            updated_symbols.push(symbol_name);
        } else {
            skipped_missing_symbols.push(symbol_name);
        }
    }

    if !updated_symbols.is_empty() {
        sch.write(sch_path)?;
    }

    Ok(UpdateFromLibReport {
        library_prefix,
        reference: reference.map(str::to_string),
        updated_symbols,
        skipped_missing_symbols,
    })
}

fn embedded_lib_symbol_names_with_prefix(
    doc: &SchematicDocument,
    library_prefix: &str,
) -> Vec<String> {
    let items = match doc.cst.nodes.first() {
        Some(Node::List { items, .. }) => items,
        _ => return Vec::new(),
    };
    let Some(Node::List {
        items: lib_items, ..
    }) = items
        .iter()
        .skip(1)
        .find(|n| head_of(n) == Some("lib_symbols"))
    else {
        return Vec::new();
    };

    let expected_prefix = format!("{library_prefix}:");
    lib_items
        .iter()
        .filter(|n| head_of(n) == Some("symbol"))
        .filter_map(|n| match n {
            Node::List {
                items: symbol_items,
                ..
            } => symbol_items.get(1).and_then(atom_as_string),
            _ => None,
        })
        .filter(|name| name.starts_with(&expected_prefix))
        .collect()
}

fn parse_ast(cst: &CstDocument) -> SchematicAst {
    let mut version = None;
    let mut generator = None;
    let mut generator_version = None;
    let mut uuid = None;
    let mut has_paper = false;
    let mut paper = None;
    let mut has_title_block = false;
    let mut title_block = None;
    let mut has_lib_symbols = false;
    let mut embedded_fonts = None;
    let mut lib_symbol_count = 0usize;
    let mut symbol_count = 0usize;
    let mut sheet_count = 0usize;
    let mut junction_count = 0usize;
    let mut no_connect_count = 0usize;
    let mut bus_entry_count = 0usize;
    let mut bus_alias_count = 0usize;
    let mut wire_count = 0usize;
    let mut bus_count = 0usize;
    let mut image_count = 0usize;
    let mut text_count = 0usize;
    let mut text_box_count = 0usize;
    let mut label_count = 0usize;
    let mut global_label_count = 0usize;
    let mut hierarchical_label_count = 0usize;
    let mut netclass_flag_count = 0usize;
    let mut polyline_count = 0usize;
    let mut rectangle_count = 0usize;
    let mut circle_count = 0usize;
    let mut arc_count = 0usize;
    let mut rule_area_count = 0usize;
    let mut sheet_instance_count = 0usize;
    let mut symbol_instance_count = 0usize;
    let mut unknown_nodes = Vec::new();
    let mut wires: Vec<SchematicWireSummary> = Vec::new();
    let mut labels: Vec<SchematicLabelSummary> = Vec::new();

    if let Some(Node::List { items, .. }) = cst.nodes.first() {
        for item in items.iter().skip(1) {
            match head_of(item) {
                Some("version") => version = second_atom_i32(item),
                Some("generator") => generator = second_atom_string(item),
                Some("generator_version") => generator_version = second_atom_string(item),
                Some("uuid") => uuid = second_atom_string(item),
                Some("paper") => {
                    has_paper = true;
                    paper = Some(parse_paper(item).into());
                }
                Some("title_block") => {
                    has_title_block = true;
                    title_block = Some(parse_title_block(item).into());
                }
                Some("lib_symbols") => {
                    has_lib_symbols = true;
                    lib_symbol_count = list_child_head_count(item, "symbol");
                }
                Some("symbol") => symbol_count += 1,
                Some("sheet") => sheet_count += 1,
                Some("junction") => junction_count += 1,
                Some("no_connect") => no_connect_count += 1,
                Some("bus_entry") => bus_entry_count += 1,
                Some("bus_alias") => bus_alias_count += 1,
                Some("wire") => {
                    wire_count += 1;
                    if let Some(w) = parse_wire_summary(item) {
                        wires.push(w);
                    }
                }
                Some("bus") => bus_count += 1,
                Some("image") => image_count += 1,
                Some("text") => text_count += 1,
                Some("text_box") => text_box_count += 1,
                Some("label") => {
                    label_count += 1;
                    if let Some(l) = parse_label_summary(item, "label") {
                        labels.push(l);
                    }
                }
                Some("global_label") => {
                    global_label_count += 1;
                    if let Some(l) = parse_label_summary(item, "global_label") {
                        labels.push(l);
                    }
                }
                Some("hierarchical_label") => {
                    hierarchical_label_count += 1;
                    if let Some(l) = parse_label_summary(item, "hierarchical_label") {
                        labels.push(l);
                    }
                }
                Some("netclass_flag") => netclass_flag_count += 1,
                Some("polyline") => polyline_count += 1,
                Some("rectangle") => rectangle_count += 1,
                Some("circle") => circle_count += 1,
                Some("arc") => arc_count += 1,
                Some("rule_area") => rule_area_count += 1,
                Some("sheet_instances") => {
                    sheet_instance_count = list_child_head_count(item, "path");
                }
                Some("symbol_instances") => {
                    symbol_instance_count = list_child_head_count(item, "path");
                }
                Some("embedded_fonts") => {
                    embedded_fonts = second_atom_bool(item);
                }
                _ => {
                    if let Some(unknown) = UnknownNode::from_node(item) {
                        unknown_nodes.push(unknown);
                    }
                }
            }
        }
    }

    SchematicAst {
        version,
        generator,
        generator_version,
        uuid,
        has_paper,
        paper,
        has_title_block,
        title_block,
        has_lib_symbols,
        embedded_fonts,
        lib_symbol_count,
        symbol_count,
        sheet_count,
        junction_count,
        no_connect_count,
        bus_entry_count,
        bus_alias_count,
        wire_count,
        bus_count,
        image_count,
        text_count,
        text_box_count,
        label_count,
        global_label_count,
        hierarchical_label_count,
        netclass_flag_count,
        polyline_count,
        rectangle_count,
        circle_count,
        arc_count,
        rule_area_count,
        sheet_instance_count,
        symbol_instance_count,
        unknown_nodes,
        wires,
        labels,
    }
}

// ─── Private wire/label parsers ───────────────────────────────────────────────

fn parse_wire_summary(node: &Node) -> Option<SchematicWireSummary> {
    let Node::List { items, .. } = node else {
        return None;
    };
    let pts = items.iter().skip(1).find(|n| head_of(n) == Some("pts"))?;
    let Node::List {
        items: pts_items, ..
    } = pts
    else {
        return None;
    };
    let (x1, y1) = parse_xy_coord(pts_items.get(1)?)?;
    let (x2, y2) = parse_xy_coord(pts_items.get(2)?)?;
    Some(SchematicWireSummary { x1, y1, x2, y2 })
}

fn parse_label_summary(node: &Node, label_type: &str) -> Option<SchematicLabelSummary> {
    let Node::List { items, .. } = node else {
        return None;
    };
    let text = items.get(1).and_then(atom_as_string)?;
    let at = items.iter().skip(2).find(|n| head_of(n) == Some("at"))?;
    let Node::List {
        items: at_items, ..
    } = at
    else {
        return None;
    };
    let x = at_items.get(1).and_then(atom_as_f64)?;
    let y = at_items.get(2).and_then(atom_as_f64)?;
    let angle = at_items.get(3).and_then(atom_as_f64).unwrap_or(0.0);
    Some(SchematicLabelSummary {
        text,
        x,
        y,
        angle,
        label_type: label_type.to_string(),
    })
}

// ─── Netlist builder (Phase D) ────────────────────────────────────────────────

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

fn intern_point(points: &mut Vec<(f64, f64)>, x: f64, y: f64) -> usize {
    const EPS: f64 = 0.001;
    if let Some(idx) = points
        .iter()
        .position(|(px, py)| (*px - x).abs() < EPS && (*py - y).abs() < EPS)
    {
        idx
    } else {
        points.push((x, y));
        points.len() - 1
    }
}

fn build_netlist(
    wires: &[SchematicWireSummary],
    labels: &[SchematicLabelSummary],
) -> SchematicNetlist {
    if wires.is_empty() {
        // No wires: each label is its own isolated net.
        let nets = labels
            .iter()
            .map(|l| SchematicNet {
                name: Some(l.text.clone()),
                labels: vec![l.clone()],
                pins: Vec::new(),
            })
            .collect();
        return SchematicNetlist { nets };
    }

    // Collect unique endpoints and wire edges.
    let mut points: Vec<(f64, f64)> = Vec::new();
    let wire_edges: Vec<(usize, usize)> = wires
        .iter()
        .map(|w| {
            let a = intern_point(&mut points, w.x1, w.y1);
            let b = intern_point(&mut points, w.x2, w.y2);
            (a, b)
        })
        .collect();

    let mut uf = UnionFind::new(points.len());
    for (a, b) in &wire_edges {
        uf.union(*a, *b);
    }

    // Group point indices by their root component.
    let mut component_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..points.len() {
        let root = uf.find(i);
        component_map.entry(root).or_default().push(i);
    }

    // For each component, gather labels that touch any point in it.
    let mut nets: Vec<SchematicNet> = component_map
        .values()
        .map(|point_indices| {
            let component_labels: Vec<SchematicLabelSummary> = labels
                .iter()
                .filter(|l| {
                    const EPS: f64 = 0.001;
                    point_indices.iter().any(|&i| {
                        let (px, py) = points[i];
                        (px - l.x).abs() < EPS && (py - l.y).abs() < EPS
                    })
                })
                .cloned()
                .collect();

            // Prefer global_label > hierarchical_label > label for naming.
            let name = component_labels
                .iter()
                .find(|l| l.label_type == "global_label")
                .or_else(|| {
                    component_labels
                        .iter()
                        .find(|l| l.label_type == "hierarchical_label")
                })
                .or_else(|| component_labels.first())
                .map(|l| l.text.clone());

            SchematicNet {
                name,
                labels: component_labels,
                pins: Vec::new(),
            }
        })
        .collect();

    // Stable output: named nets first (sorted), then unnamed.
    nets.sort_by(|a, b| match (&a.name, &b.name) {
        (Some(na), Some(nb)) => na.cmp(nb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    SchematicNetlist { nets }
}

/// Merge netlists from multiple schematic sheets into one unified netlist.
///
/// Nets connected by [`global_label`](SchematicLabelSummary) with the same
/// name are merged across sheets. Nets named only by local labels remain
/// sheet-local. Hierarchical labels are not yet resolved (they require
/// parent-sheet pin matching).
pub fn merge_sheet_netlists(docs: &[&SchematicDocument]) -> SchematicNetlist {
    // Build individual netlists and flatten into one list of nets.
    let all_nets: Vec<SchematicNet> = docs
        .iter()
        .flat_map(|doc| build_netlist(&doc.ast.wires, &doc.ast.labels).nets)
        .collect();

    let mut merged: Vec<SchematicNet> = Vec::new();
    // Maps global_label text → index in `merged`.
    let mut global_map: HashMap<String, usize> = HashMap::new();

    for net in all_nets {
        // A net crosses sheet boundaries only if it has a global_label.
        let global_name = net
            .labels
            .iter()
            .find(|l| l.label_type == "global_label")
            .map(|l| l.text.clone());

        if let Some(ref name) = global_name {
            if let Some(&idx) = global_map.get(name) {
                // Merge labels into the already-seen global net.
                merged[idx].labels.extend(net.labels);
            } else {
                let idx = merged.len();
                global_map.insert(name.clone(), idx);
                merged.push(net);
            }
        } else {
            // Local net — keep as-is.
            merged.push(net);
        }
    }

    merged.sort_by(|a, b| match (&a.name, &b.name) {
        (Some(na), Some(nb)) => na.cmp(nb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    SchematicNetlist { nets: merged }
}

// ─── Private structural-editing helpers ──────────────────────────────────────

fn generate_uuid() -> String {
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

/// Format a coordinate value without spurious trailing `.0`.
fn fmt_coord(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn effects_node(hide: bool) -> Node {
    let mut items = vec![
        atom_symbol("effects"),
        list_node(vec![
            atom_symbol("font"),
            list_node(vec![
                atom_symbol("size"),
                atom_symbol("1.27"),
                atom_symbol("1.27"),
            ]),
        ]),
    ];
    if hide {
        items.push(list_node(vec![atom_symbol("hide"), atom_symbol("yes")]));
    }
    list_node(items)
}

fn property_with_at_node(key: &str, value: &str, x: f64, y: f64, hide: bool) -> Node {
    list_node(vec![
        atom_symbol("property"),
        atom_quoted(key),
        atom_quoted(value),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(fmt_coord(x)),
            atom_symbol(fmt_coord(y)),
            atom_symbol("0"),
        ]),
        effects_node(hide),
    ])
}

fn symbol_instance_node(lib_id: &str, reference: &str, value: &str, x: f64, y: f64) -> Node {
    list_node(vec![
        atom_symbol("symbol"),
        list_node(vec![atom_symbol("lib_id"), atom_quoted(lib_id)]),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(fmt_coord(x)),
            atom_symbol(fmt_coord(y)),
            atom_symbol("0"),
        ]),
        list_node(vec![atom_symbol("unit"), atom_symbol("1")]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(generate_uuid())]),
        property_with_at_node("Reference", reference, x, y - 2.0, false),
        property_with_at_node("Value", value, x, y + 2.0, false),
        property_with_at_node("Footprint", "", x, y + 4.0, true),
    ])
}

fn junction_node(x: f64, y: f64) -> Node {
    list_node(vec![
        atom_symbol("junction"),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(fmt_coord(x)),
            atom_symbol(fmt_coord(y)),
        ]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(generate_uuid())]),
    ])
}

fn no_connect_node(x: f64, y: f64) -> Node {
    list_node(vec![
        atom_symbol("no_connect"),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(fmt_coord(x)),
            atom_symbol(fmt_coord(y)),
        ]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(generate_uuid())]),
    ])
}

fn xy_node(x: f64, y: f64) -> Node {
    list_node(vec![
        atom_symbol("xy"),
        atom_symbol(fmt_coord(x)),
        atom_symbol(fmt_coord(y)),
    ])
}

fn wire_node(x1: f64, y1: f64, x2: f64, y2: f64) -> Node {
    list_node(vec![
        atom_symbol("wire"),
        list_node(vec![atom_symbol("pts"), xy_node(x1, y1), xy_node(x2, y2)]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(generate_uuid())]),
    ])
}

fn label_node(text: &str, x: f64, y: f64, angle: f64) -> Node {
    list_node(vec![
        atom_symbol("label"),
        atom_quoted(text),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(fmt_coord(x)),
            atom_symbol(fmt_coord(y)),
            atom_symbol(fmt_coord(angle)),
        ]),
        list_node(vec![atom_symbol("fields_autoplaced"), atom_symbol("yes")]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(generate_uuid())]),
    ])
}

fn global_label_node(text: &str, shape: &str, x: f64, y: f64, angle: f64) -> Node {
    list_node(vec![
        atom_symbol("global_label"),
        atom_quoted(text),
        list_node(vec![atom_symbol("shape"), atom_symbol(shape)]),
        list_node(vec![
            atom_symbol("at"),
            atom_symbol(fmt_coord(x)),
            atom_symbol(fmt_coord(y)),
            atom_symbol(fmt_coord(angle)),
        ]),
        list_node(vec![atom_symbol("fields_autoplaced"), atom_symbol("yes")]),
        list_node(vec![atom_symbol("uuid"), atom_quoted(generate_uuid())]),
    ])
}

fn parse_xy_coord(node: &Node) -> Option<(f64, f64)> {
    if head_of(node) != Some("xy") {
        return None;
    }
    let Node::List { items, .. } = node else {
        return None;
    };
    let x = items.get(1).and_then(atom_as_string)?.parse::<f64>().ok()?;
    let y = items.get(2).and_then(atom_as_string)?.parse::<f64>().ok()?;
    Some((x, y))
}

fn wire_pts_match(node: &Node, x1: f64, y1: f64, x2: f64, y2: f64) -> bool {
    (|| -> Option<bool> {
        if head_of(node) != Some("wire") {
            return Some(false);
        }
        let Node::List { items, .. } = node else {
            return Some(false);
        };
        let pts = items.iter().skip(1).find(|n| head_of(n) == Some("pts"))?;
        let Node::List {
            items: pts_items, ..
        } = pts
        else {
            return Some(false);
        };
        let pt1 = pts_items.get(1).and_then(parse_xy_coord)?;
        let pt2 = pts_items.get(2).and_then(parse_xy_coord)?;
        Some(pt1 == (x1, y1) && pt2 == (x2, y2))
    })()
    .unwrap_or(false)
}

fn label_name_matches(node: &Node, name: &str) -> bool {
    match head_of(node) {
        Some("label") | Some("global_label") | Some("hierarchical_label") => {}
        _ => return false,
    }
    let Node::List { items, .. } = node else {
        return false;
    };
    items.get(1).and_then(atom_as_string).as_deref() == Some(name)
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
        std::env::temp_dir().join(format!("{name}_{nanos}.kicad_sch"))
    }

    #[test]
    fn read_schematic_and_preserve_lossless() {
        let path = tmp_file("sch_read_ok");
        let src = "(kicad_sch (version 20250114) (generator \"eeschema\") (generator_version \"9.0\") (uuid \"u-1\") (paper \"A4\") (title_block (title \"Demo\") (date \"2026-02-25\") (comment 2 \"c2\") (comment 1 \"c1\")) (lib_symbols (symbol \"Lib:R\")) (symbol (lib_id \"Lib:R\")) (wire (pts (xy 0 0) (xy 1 1))) (sheet_instances (path \"/\" (page \"1\"))) (embedded_fonts no))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = SchematicFile::read(&path).expect("read");
        assert_eq!(doc.ast().version, Some(20250114));
        assert_eq!(doc.ast().generator.as_deref(), Some("eeschema"));
        assert_eq!(doc.ast().generator_version.as_deref(), Some("9.0"));
        assert_eq!(doc.ast().uuid.as_deref(), Some("u-1"));
        assert_eq!(
            doc.ast().paper.as_ref().and_then(|p| p.kind.clone()),
            Some("A4".to_string())
        );
        assert_eq!(doc.ast().lib_symbol_count, 1);
        assert_eq!(doc.ast().symbol_count, 1);
        assert_eq!(doc.ast().wire_count, 1);
        assert_eq!(doc.ast().sheet_instance_count, 1);
        assert_eq!(doc.ast().embedded_fonts, Some(false));
        assert_eq!(doc.cst().to_lossless_string(), src);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn captures_unknown_nodes_roundtrip() {
        let path = tmp_file("sch_unknown");
        let src = "(kicad_sch (version 20250114) (generator \"eeschema\") (future_block 1 2) (lib_symbols (symbol \"Device:R\")) (symbol (lib_id \"Device:R\")))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = SchematicFile::read(&path).expect("read");
        assert_eq!(doc.ast().unknown_nodes.len(), 1);

        let out = tmp_file("sch_unknown_out");
        doc.write(&out).expect("write");
        let got = fs::read_to_string(&out).expect("read out");
        assert_eq!(got, src);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn edit_roundtrip_updates_core_fields() {
        let path = tmp_file("sch_edit");
        let src = "(kicad_sch (version 20241229) (generator \"eeschema\") (paper \"A4\") (title_block (title \"Old\") (date \"2025-01-01\") (rev \"A\") (company \"OldCo\")) (future_token 1 2))\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = SchematicFile::read(&path).expect("read");
        doc.set_version(20260101)
            .set_generator("kiutils")
            .set_generator_version("dev")
            .set_uuid("uuid-new")
            .set_paper_user(297.0, 210.0, Some("landscape"))
            .set_title("New")
            .set_date("2026-02-25")
            .set_revision("B")
            .set_company("Lords")
            .set_embedded_fonts(true);

        let out = tmp_file("sch_edit_out");
        doc.write(&out).expect("write");
        let reread = SchematicFile::read(&out).expect("reread");

        assert_eq!(reread.ast().version, Some(20260101));
        assert_eq!(reread.ast().generator.as_deref(), Some("kiutils"));
        assert_eq!(reread.ast().generator_version.as_deref(), Some("dev"));
        assert_eq!(reread.ast().uuid.as_deref(), Some("uuid-new"));
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.kind.clone()),
            Some("User".to_string())
        );
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.width),
            Some(297.0)
        );
        assert_eq!(
            reread.ast().paper.as_ref().and_then(|p| p.height),
            Some(210.0)
        );
        assert_eq!(reread.ast().embedded_fonts, Some(true));
        assert_eq!(reread.ast().unknown_nodes.len(), 1);
        assert_eq!(
            reread
                .ast()
                .title_block
                .as_ref()
                .and_then(|t| t.title.clone()),
            Some("New".to_string())
        );

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    const MINIMAL_SCH: &str = "(kicad_sch (version 20260101) (generator \"eeschema\") (uuid \"u1\") (paper \"A4\") (lib_symbols (symbol \"Device:R\")) (symbol (lib_id \"Device:R\") (at 100 50 0) (unit 1) (uuid \"abc-123\") (property \"Reference\" \"R1\" (at 100 40 0) (effects (font (size 1.27 1.27)))) (property \"Value\" \"10k\" (at 100 60 0) (effects (font (size 1.27 1.27)))) (property \"Footprint\" \"Resistor_SMD:R_0603\" (at 100 70 0) (effects (font (size 1.27 1.27)) (hide yes)))) (wire (pts (xy 0 0) (xy 1 1))))\n";

    #[test]
    fn add_symbol_instance_increases_count() {
        let path = tmp_file("sch_add_sym");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().symbol_count, 1);
        doc.add_symbol_instance("Device:C", "C1", "100nF", 50.0, 50.0);
        assert_eq!(doc.ast().symbol_count, 2);

        let syms = doc.symbol_instances();
        assert!(syms.iter().any(|s| s.reference.as_deref() == Some("C1")));
        assert!(syms.iter().any(|s| s.lib_id.as_deref() == Some("Device:C")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_symbol_instance_decreases_count() {
        let path = tmp_file("sch_rm_sym");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_symbol_instance("Device:C", "C1", "100nF", 50.0, 50.0);
        assert_eq!(doc.ast().symbol_count, 2);

        doc.remove_symbol_instance("C1");
        assert_eq!(doc.ast().symbol_count, 1);
        assert!(!doc
            .symbol_instances()
            .iter()
            .any(|s| s.reference.as_deref() == Some("C1")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_symbol_lib_id_updates_lib_id() {
        let path = tmp_file("sch_set_lib");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.set_symbol_lib_id("R1", "Device:R_Small");
        let syms = doc.symbol_instances();
        let r1 = syms
            .iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .unwrap();
        assert_eq!(r1.lib_id.as_deref(), Some("Device:R_Small"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_wire_increases_count() {
        let path = tmp_file("sch_add_wire");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().wire_count, 1);
        doc.add_wire(10.0, 20.0, 30.0, 20.0);
        assert_eq!(doc.ast().wire_count, 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_wire_at_decreases_count() {
        let path = tmp_file("sch_rm_wire");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_wire(10.0, 20.0, 30.0, 20.0);
        assert_eq!(doc.ast().wire_count, 2);

        doc.remove_wire_at(10.0, 20.0, 30.0, 20.0);
        assert_eq!(doc.ast().wire_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_label_by_name_decreases_count() {
        let path = tmp_file("sch_rm_label");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_label("CLK", 10.0, 20.0, 0.0);
        assert_eq!(doc.ast().label_count, 1);

        doc.remove_label_by_name("CLK");
        assert_eq!(doc.ast().label_count, 0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn rename_label_changes_text() {
        let path = tmp_file("sch_rename_label");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_label("OLD", 10.0, 20.0, 0.0);
        doc.rename_label("OLD", "NEW");
        doc.write(&path).expect("write");

        let doc2 = SchematicFile::read(&path).expect("re-read");
        let labels = doc2.ast().labels.clone();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "NEW");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_label_increases_count() {
        let path = tmp_file("sch_add_label");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().label_count, 0);
        doc.add_label("VCC", 100.0, 0.0, 0.0);
        assert_eq!(doc.ast().label_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_global_label_increases_count() {
        let path = tmp_file("sch_add_glabel");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().global_label_count, 0);
        doc.add_global_label("VCC", "power_in", 100.0, 0.0, 0.0);
        assert_eq!(doc.ast().global_label_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_junction_increases_count() {
        let path = tmp_file("sch_add_junction");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().junction_count, 0);
        doc.add_junction(100.0, 50.0);
        assert_eq!(doc.ast().junction_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_no_connect_increases_count() {
        let path = tmp_file("sch_add_nc");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().no_connect_count, 0);
        doc.add_no_connect(200.0, 50.0);
        assert_eq!(doc.ast().no_connect_count, 1);

        let _ = fs::remove_file(path);
    }

    // ─── Symbol deep tests ────────────────────────────────────────────────────

    #[test]
    fn add_symbol_fields_correct() {
        let path = tmp_file("sch_sym_fields");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_symbol_instance("Device:LED", "D1", "LED_Red", 50.5, 75.25);

        let syms = doc.symbol_instances();
        let d1 = syms
            .iter()
            .find(|s| s.reference.as_deref() == Some("D1"))
            .expect("D1 not found");
        assert_eq!(d1.lib_id.as_deref(), Some("Device:LED"));
        assert_eq!(d1.value.as_deref(), Some("LED_Red"));

        // Verify coordinates appear in raw CST
        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("50.5"), "x coord not in output");
        assert!(raw.contains("75.25"), "y coord not in output");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_symbol_survives_write_reread() {
        let path = tmp_file("sch_sym_reread");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_symbol_instance("Device:C", "C1", "100nF", 200.0, 50.0);
        doc.upsert_embedded_lib_symbol(list_node(vec![
            atom_symbol("symbol"),
            atom_quoted("Device:C"),
        ]));
        doc.write(&path).expect("write");

        let reread = SchematicFile::read(&path).expect("reread");
        let c1 = reread
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("C1"))
            .expect("C1 lost after write+read");
        assert_eq!(c1.lib_id.as_deref(), Some("Device:C"));
        assert_eq!(c1.value.as_deref(), Some("100nF"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_symbol_noop_wrong_ref() {
        let path = tmp_file("sch_sym_noop");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().symbol_count, 1);
        doc.remove_symbol_instance("Z99");
        assert_eq!(
            doc.ast().symbol_count,
            1,
            "count must not change on no match"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_remove_symbol_restores_count() {
        let path = tmp_file("sch_sym_symmetric");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        let orig = doc.ast().symbol_count;
        doc.add_symbol_instance("Device:L", "L1", "10uH", 300.0, 50.0);
        assert_eq!(doc.ast().symbol_count, orig + 1);
        doc.remove_symbol_instance("L1");
        assert_eq!(doc.ast().symbol_count, orig);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_lib_id_content_correct() {
        let path = tmp_file("sch_libid_content");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.set_symbol_lib_id("R1", "Device:R_Small");

        let r1 = doc
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .expect("R1 not found");
        assert_eq!(
            r1.lib_id.as_deref(),
            Some("Device:R_Small"),
            "lib_id not updated in AST"
        );

        // Also verify old value is gone from raw CST
        let raw = doc.cst().to_canonical_string();
        assert!(
            !raw.contains("\"Device:R\"") || raw.contains("\"Device:R_Small\""),
            "old lib_id still present"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_lib_id_noop_wrong_ref() {
        let path = tmp_file("sch_libid_noop");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        let before = doc
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .unwrap();
        doc.set_symbol_lib_id("Z99", "Device:Something");
        let after = doc
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .unwrap();
        assert_eq!(
            before.lib_id, after.lib_id,
            "lib_id should not change when ref doesn't match"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_lib_id_survives_write_reread() {
        let path = tmp_file("sch_libid_reread");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.set_symbol_lib_id("R1", "Device:R_Small");
        doc.upsert_embedded_lib_symbol(list_node(vec![
            atom_symbol("symbol"),
            atom_quoted("Device:R_Small"),
        ]));
        doc.write(&path).expect("write");

        let reread = SchematicFile::read(&path).expect("reread");
        let r1 = reread
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .expect("R1 lost");
        assert_eq!(r1.lib_id.as_deref(), Some("Device:R_Small"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_lib_id_write_fails_when_embedded_symbol_cache_is_missing() {
        let path = tmp_file("sch_libid_missing_cache");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.set_symbol_lib_id("R1", "Device:R_Small");
        let err = doc.write(&path).expect_err("write must fail");
        assert!(matches!(err, Error::Validation(_)));
        assert!(
            err.to_string()
                .contains("without embedded lib_symbols entries: Device:R_Small"),
            "unexpected error: {err}"
        );

        let reread = SchematicFile::read(&path).expect("reread");
        let r1 = reread
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .expect("R1 lost");
        assert_eq!(r1.lib_id.as_deref(), Some("Device:R"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn validate_embedded_symbol_cache_accepts_consistent_schematic() {
        let path = tmp_file("sch_valid_cache");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let doc = SchematicFile::read(&path).expect("read");

        doc.validate_embedded_symbol_cache()
            .expect("cache should be valid");
        assert!(doc.missing_embedded_lib_symbol_lib_ids().is_empty());

        let _ = fs::remove_file(path);
    }

    // ─── Wire deep tests ──────────────────────────────────────────────────────

    #[test]
    fn add_wire_content_correct() {
        let path = tmp_file("sch_wire_content");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_wire(1.5, 2.5, 10.75, 2.5);

        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("1.5"), "x1 not in output");
        assert!(raw.contains("2.5"), "y not in output");
        assert!(raw.contains("10.75"), "x2 not in output");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_wire_noop_wrong_coords() {
        let path = tmp_file("sch_wire_noop");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        assert_eq!(doc.ast().wire_count, 1);
        doc.remove_wire_at(999.0, 999.0, 888.0, 888.0);
        assert_eq!(doc.ast().wire_count, 1, "count must not change on no match");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_wire_specificity() {
        let path = tmp_file("sch_wire_specific");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_wire(10.0, 0.0, 20.0, 0.0);
        doc.add_wire(30.0, 0.0, 40.0, 0.0);
        assert_eq!(doc.ast().wire_count, 3);

        doc.remove_wire_at(10.0, 0.0, 20.0, 0.0);
        assert_eq!(doc.ast().wire_count, 2);

        // The second wire must remain; verified via raw CST
        let raw = doc.cst().to_canonical_string();
        assert!(
            raw.contains("30") && raw.contains("40"),
            "second wire should still be present"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_remove_wire_restores_count() {
        let path = tmp_file("sch_wire_symmetric");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        let orig = doc.ast().wire_count;
        doc.add_wire(5.0, 5.0, 15.0, 5.0);
        assert_eq!(doc.ast().wire_count, orig + 1);
        doc.remove_wire_at(5.0, 5.0, 15.0, 5.0);
        assert_eq!(doc.ast().wire_count, orig);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_wire_survives_write_reread() {
        let path = tmp_file("sch_wire_reread");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_wire(1.27, 2.54, 10.16, 2.54);
        doc.write(&path).expect("write");

        let reread = SchematicFile::read(&path).expect("reread");
        assert_eq!(reread.ast().wire_count, 2);

        let _ = fs::remove_file(path);
    }

    // ─── Label / global-label deep tests ─────────────────────────────────────

    #[test]
    fn add_label_content_correct() {
        let path = tmp_file("sch_label_content");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_label("NET_POWER", 50.5, 25.25, 90.0);

        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("NET_POWER"), "label text not in output");
        assert!(raw.contains("50.5"), "x coord not in output");
        assert!(raw.contains("25.25"), "y coord not in output");
        assert!(raw.contains("90"), "angle not in output");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_label_survives_write_reread() {
        let path = tmp_file("sch_label_reread");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_label("SDA", 100.0, 0.0, 0.0);
        doc.write(&path).expect("write");

        let reread = SchematicFile::read(&path).expect("reread");
        assert_eq!(reread.ast().label_count, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_global_label_content_correct() {
        let path = tmp_file("sch_glabel_content");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_global_label("VCC_3V3", "power_in", 100.5, 50.5, 0.0);

        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("VCC_3V3"), "label text not in output");
        assert!(raw.contains("power_in"), "shape not in output");
        assert!(raw.contains("100.5"), "x coord not in output");
        assert!(raw.contains("50.5"), "y coord not in output");

        let _ = fs::remove_file(path);
    }

    // ─── Junction / no-connect deep tests ────────────────────────────────────

    #[test]
    fn add_junction_content_correct() {
        let path = tmp_file("sch_junc_content");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_junction(33.5, 44.25);

        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("33.5"), "x not in output");
        assert!(raw.contains("44.25"), "y not in output");
        assert!(raw.contains("junction"), "junction node not in output");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_no_connect_content_correct() {
        let path = tmp_file("sch_nc_content");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_no_connect(77.5, 88.25);

        let raw = doc.cst().to_canonical_string();
        assert!(raw.contains("77.5"), "x not in output");
        assert!(raw.contains("88.25"), "y not in output");
        assert!(raw.contains("no_connect"), "no_connect node not in output");

        let _ = fs::remove_file(path);
    }

    // ─── Property after structural edit preserved ─────────────────────────────

    #[test]
    fn property_preserved_after_structural_edit() {
        let path = tmp_file("sch_prop_after_edit");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        // Set a custom property on R1, then do a structural edit, then verify property still there
        doc.upsert_symbol_instance_property("R1", "MPN", "RC0603FR-0710KL");
        doc.add_wire(200.0, 0.0, 300.0, 0.0);

        doc.write(&path).expect("write");
        let reread = SchematicFile::read(&path).expect("reread");

        let r1 = reread
            .symbol_instances()
            .into_iter()
            .find(|s| s.reference.as_deref() == Some("R1"))
            .expect("R1 lost");
        assert!(
            r1.properties
                .iter()
                .any(|(k, v)| k == "MPN" && v == "RC0603FR-0710KL"),
            "MPN property lost after structural edit"
        );
        assert_eq!(reread.ast().wire_count, 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn structural_roundtrip() {
        let path = tmp_file("sch_structural");
        fs::write(&path, MINIMAL_SCH).expect("write");
        let mut doc = SchematicFile::read(&path).expect("read");

        doc.add_symbol_instance("Device:LED", "D1", "LED", 200.0, 50.0)
            .add_wire(200.0, 0.0, 200.0, 50.0)
            .add_label("NET_A", 200.0, 0.0, 0.0)
            .add_global_label("VCC", "power_in", 200.0, 100.0, 180.0);
        doc.upsert_embedded_lib_symbol(list_node(vec![
            atom_symbol("symbol"),
            atom_quoted("Device:LED"),
        ]));

        let out = tmp_file("sch_structural_out");
        doc.write(&out).expect("write");

        let reread = SchematicFile::read(&out).expect("reread");
        assert_eq!(reread.ast().symbol_count, 2);
        assert_eq!(reread.ast().wire_count, 2);
        assert_eq!(reread.ast().label_count, 1);
        assert_eq!(reread.ast().global_label_count, 1);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    // ── Phase C: symbol position, wire objects, label objects ─────────────

    const SCH_WITH_WIRES_AND_LABELS: &str = concat!(
        "(kicad_sch (version 20260101) (generator test) (uuid \"u1\") (paper \"A4\")\n",
        "  (lib_symbols)\n",
        "  (symbol (lib_id \"Device:R\") (at 120 80 90) (unit 2) (uuid \"s1\")\n",
        "    (property \"Reference\" \"R1\" (at 120 70 0) (effects (font (size 1.27 1.27))))\n",
        "    (property \"Value\" \"4k7\" (at 120 90 0) (effects (font (size 1.27 1.27))))\n",
        "    (property \"Footprint\" \"Resistor_SMD:R_0402\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes))))\n",
        "  (wire (pts (xy 100 80) (xy 120 80)) (uuid \"w1\"))\n",
        "  (wire (pts (xy 120 80) (xy 140 80)) (uuid \"w2\"))\n",
        "  (label \"VCC\" (at 100 80 0) (fields_autoplaced yes) (uuid \"l1\"))\n",
        "  (global_label \"GND\" (shape power_in) (at 140 80 180) (fields_autoplaced yes) (uuid \"gl1\"))\n",
        ")\n",
    );

    #[test]
    fn symbol_position_extracted() {
        let path = tmp_file("sym_pos");
        fs::write(&path, SCH_WITH_WIRES_AND_LABELS).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        let instances = doc.symbol_instances();
        assert_eq!(instances.len(), 1);
        let r1 = &instances[0];
        assert_eq!(r1.x, Some(120.0));
        assert_eq!(r1.y, Some(80.0));
        assert_eq!(r1.angle, Some(90.0));
        assert_eq!(r1.unit, Some(2));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn wire_objects_collected() {
        let path = tmp_file("wire_obj");
        fs::write(&path, SCH_WITH_WIRES_AND_LABELS).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        assert_eq!(doc.ast().wire_count, 2);
        assert_eq!(doc.ast().wires.len(), 2);
        let w = &doc.ast().wires[0];
        assert_eq!(w.x1, 100.0);
        assert_eq!(w.y1, 80.0);
        assert_eq!(w.x2, 120.0);
        assert_eq!(w.y2, 80.0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn label_objects_collected() {
        let path = tmp_file("label_obj");
        fs::write(&path, SCH_WITH_WIRES_AND_LABELS).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        assert_eq!(doc.ast().labels.len(), 2);
        let local = doc
            .ast()
            .labels
            .iter()
            .find(|l| l.label_type == "label")
            .unwrap();
        assert_eq!(local.text, "VCC");
        assert_eq!(local.x, 100.0);
        assert_eq!(local.y, 80.0);
        let global = doc
            .ast()
            .labels
            .iter()
            .find(|l| l.label_type == "global_label")
            .unwrap();
        assert_eq!(global.text, "GND");
        assert_eq!(global.x, 140.0);
        let _ = fs::remove_file(path);
    }

    // ── Phase D: netlist / connectivity ───────────────────────────────────

    #[test]
    fn netlist_names_connected_wire_segment() {
        // Two wires sharing endpoint at (120,80), label "VCC" at (100,80) → one net "VCC"
        let path = tmp_file("netlist_named");
        fs::write(&path, SCH_WITH_WIRES_AND_LABELS).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        let netlist = doc.netlist();
        // All wires connect to form one component, which gets label "GND" (global preferred)
        // and "VCC" (local). The component covers (100,80)-(120,80)-(140,80).
        // Two labels → one net with both labels.
        assert_eq!(netlist.nets.len(), 1);
        let net = &netlist.nets[0];
        assert_eq!(net.labels.len(), 2);
        // Global label preferred for name
        assert_eq!(net.name.as_deref(), Some("GND"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn netlist_unnamed_wire_has_none_name() {
        // A wire with no labels → unnamed net
        let src = concat!(
            "(kicad_sch (version 20260101) (generator test) (uuid \"u2\") (paper \"A4\")\n",
            "  (lib_symbols)\n",
            "  (wire (pts (xy 0 0) (xy 10 0)) (uuid \"w1\"))\n",
            ")\n",
        );
        let path = tmp_file("netlist_unnamed");
        fs::write(&path, src).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        let netlist = doc.netlist();
        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.nets[0].name, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn netlist_two_isolated_wires_become_two_nets() {
        let src = concat!(
            "(kicad_sch (version 20260101) (generator test) (uuid \"u3\") (paper \"A4\")\n",
            "  (lib_symbols)\n",
            "  (wire (pts (xy 0 0) (xy 10 0)) (uuid \"w1\"))\n",
            "  (wire (pts (xy 100 0) (xy 110 0)) (uuid \"w2\"))\n",
            "  (label \"A\" (at 0 0 0) (uuid \"l1\"))\n",
            "  (label \"B\" (at 100 0 0) (uuid \"l2\"))\n",
            ")\n",
        );
        let path = tmp_file("netlist_two");
        fs::write(&path, src).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        let netlist = doc.netlist();
        assert_eq!(netlist.nets.len(), 2);
        let names: Vec<_> = netlist
            .nets
            .iter()
            .filter_map(|n| n.name.as_deref())
            .collect();
        assert!(names.contains(&"A"));
        assert!(names.contains(&"B"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn netlist_empty_schematic_has_no_nets() {
        let src = "(kicad_sch (version 20260101) (generator test) (uuid \"u4\") (paper \"A4\") (lib_symbols))\n";
        let path = tmp_file("netlist_empty");
        fs::write(&path, src).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        let netlist = doc.netlist();
        assert_eq!(netlist.nets.len(), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn netlist_chained_wires_form_one_net() {
        // Three wires in a chain: (0,0)-(10,0), (10,0)-(20,0), (20,0)-(30,0)
        // One label at (0,0) → all one net
        let src = concat!(
            "(kicad_sch (version 20260101) (generator test) (uuid \"u5\") (paper \"A4\")\n",
            "  (lib_symbols)\n",
            "  (wire (pts (xy 0 0) (xy 10 0)) (uuid \"w1\"))\n",
            "  (wire (pts (xy 10 0) (xy 20 0)) (uuid \"w2\"))\n",
            "  (wire (pts (xy 20 0) (xy 30 0)) (uuid \"w3\"))\n",
            "  (label \"VCC\" (at 0 0 0) (uuid \"l1\"))\n",
            ")\n",
        );
        let path = tmp_file("netlist_chain");
        fs::write(&path, src).expect("write");
        let doc = SchematicFile::read(&path).expect("read");
        let netlist = doc.netlist();
        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.nets[0].name.as_deref(), Some("VCC"));
        let _ = fs::remove_file(path);
    }
}
