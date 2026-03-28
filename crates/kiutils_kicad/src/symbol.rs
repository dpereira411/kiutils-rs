use std::fs;
use std::path::Path;

use kiutils_sexpr::{parse_one, CstDocument, Node};

use crate::diagnostic::Diagnostic;
use crate::sexpr_edit::{
    atom_quoted, atom_symbol, ensure_root_head_any, mutate_root_and_refresh, remove_property,
    upsert_property_preserve_tail, upsert_scalar,
};
use crate::sexpr_utils::{
    atom_as_f64, atom_as_string, head_of, second_atom_f64, second_atom_i32, second_atom_string,
};
use crate::version_diag::collect_version_diagnostics;
use crate::{Error, UnknownNode, VersionPolicy, WriteMode};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PinSummary {
    /// KiCad electrical type: `"input"`, `"output"`, `"passive"`, `"power_in"`, etc.
    pub electrical_type: Option<String>,
    /// KiCad graphical style: `"line"`, `"inverted"`, `"clock"`, etc.
    pub graphical_style: Option<String>,
    /// Pin function name (e.g. `"~"`, `"VCC"`, `"CLK"`).
    pub name: Option<String>,
    /// Pin number as a string (e.g. `"1"`, `"A3"`, `"GND"`).
    pub number: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub angle: Option<f64>,
    pub length: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolSummary {
    pub name: Option<String>,
    /// All properties as `(key, value)` pairs in document order.
    pub properties: Vec<(String, String)>,
    /// Always equals `properties.len()`.
    pub property_count: usize,
    /// Total pins across all units.
    pub pin_count: usize,
    pub unit_count: usize,
    pub has_embedded_fonts: bool,
    /// Individual pin details (name, number, type, position).
    pub pins: Vec<PinSummary>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolLibAst {
    pub version: Option<i32>,
    pub generator: Option<String>,
    pub generator_version: Option<String>,
    pub symbol_count: usize,
    pub total_property_count: usize,
    pub total_pin_count: usize,
    pub symbols: Vec<SymbolSummary>,
    pub unknown_nodes: Vec<UnknownNode>,
}

#[derive(Debug, Clone)]
pub struct SymbolLibDocument {
    ast: SymbolLibAst,
    cst: CstDocument,
    diagnostics: Vec<Diagnostic>,
    ast_dirty: bool,
    policy: VersionPolicy,
}

impl SymbolLibDocument {
    pub fn ast(&self) -> &SymbolLibAst {
        &self.ast
    }

    pub fn ast_mut(&mut self) -> &mut SymbolLibAst {
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

    pub fn rename_symbol<S: Into<String>>(&mut self, from: &str, to: S) -> &mut Self {
        let from = from.to_string();
        let to = to.into();
        self.mutate_root_items(|items| {
            if let Some(idx) = find_symbol_index(items, &from) {
                if let Some(Node::List {
                    items: symbol_items,
                    ..
                }) = items.get_mut(idx)
                {
                    if symbol_items.len() > 1 {
                        let next = atom_quoted(to);
                        if symbol_items[1] == next {
                            false
                        } else {
                            symbol_items[1] = next;
                            true
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        })
    }

    pub fn rename_first_symbol<S: Into<String>>(&mut self, to: S) -> &mut Self {
        let to = to.into();
        self.mutate_root_items(|items| {
            let Some(idx) = items
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, n)| head_of(n) == Some("symbol"))
                .map(|(idx, _)| idx)
            else {
                return false;
            };
            let Some(Node::List {
                items: symbol_items,
                ..
            }) = items.get_mut(idx)
            else {
                return false;
            };
            if symbol_items.len() > 1 {
                let next = atom_quoted(to);
                if symbol_items[1] == next {
                    false
                } else {
                    symbol_items[1] = next;
                    true
                }
            } else {
                false
            }
        })
    }

    pub fn upsert_symbol_property(
        &mut self,
        symbol_name: &str,
        key: &str,
        value: &str,
    ) -> &mut Self {
        let symbol_name = symbol_name.to_string();
        let key = key.to_string();
        let value = value.to_string();
        self.mutate_root_items(|items| {
            if let Some(idx) = find_symbol_index(items, &symbol_name) {
                if let Some(Node::List {
                    items: symbol_items,
                    ..
                }) = items.get_mut(idx)
                {
                    upsert_property_preserve_tail(symbol_items, &key, &value, 2)
                } else {
                    false
                }
            } else {
                false
            }
        })
    }

    pub fn remove_symbol_property(&mut self, symbol_name: &str, key: &str) -> &mut Self {
        let symbol_name = symbol_name.to_string();
        let key = key.to_string();
        self.mutate_root_items(|items| {
            if let Some(idx) = find_symbol_index(items, &symbol_name) {
                if let Some(Node::List {
                    items: symbol_items,
                    ..
                }) = items.get_mut(idx)
                {
                    remove_property(symbol_items, &key, 2)
                } else {
                    false
                }
            } else {
                false
            }
        })
    }

    /// Insert or replace a symbol node in this library.
    ///
    /// The node's name (second atom) is used as the lookup key. If a symbol with
    /// that name already exists it is replaced in-place; otherwise the node is
    /// appended at the end of the library.
    pub(crate) fn upsert_lib_symbol(&mut self, node: Node) -> &mut Self {
        self.mutate_root_items(|items| {
            let name = match &node {
                Node::List { items: si, .. } => si.get(1).and_then(atom_as_string),
                _ => None,
            };
            let Some(name) = name else {
                return false;
            };
            if let Some(idx) = find_symbol_index(items, &name) {
                items[idx] = node;
            } else {
                items.push(node);
            }
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
        match mode {
            WriteMode::Lossless => fs::write(path, self.cst.to_lossless_string())?,
            WriteMode::Canonical => fs::write(path, self.cst.to_canonical_string())?,
        }
        Ok(())
    }

    pub(crate) fn extract_lib_symbol(&self, symbol_name: &str) -> Option<Node> {
        let items = match self.cst.nodes.first() {
            Some(Node::List { items, .. }) => items,
            _ => return None,
        };
        items
            .iter()
            .skip(1)
            .find(|n| {
                head_of(n) == Some("symbol")
                    && match n {
                        Node::List { items: si, .. } => {
                            si.get(1).and_then(atom_as_string).as_deref() == Some(symbol_name)
                        }
                        _ => false,
                    }
            })
            .cloned()
    }

    pub(crate) fn has_symbol(&self, symbol_name: &str) -> bool {
        self.ast
            .symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some(symbol_name))
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
}

pub struct SymbolLibFile;

impl SymbolLibFile {
    pub fn read<P: AsRef<Path>>(path: P) -> Result<SymbolLibDocument, Error> {
        Self::read_with_policy(path, VersionPolicy::default())
    }

    pub fn read_with_policy<P: AsRef<Path>>(
        path: P,
        policy: VersionPolicy,
    ) -> Result<SymbolLibDocument, Error> {
        let raw = fs::read_to_string(path)?;
        let cst = parse_one(&raw)?;
        ensure_root_head_any(&cst, &["kicad_symbol_lib"])?;
        let ast = parse_ast(&cst);
        let diagnostics = collect_version_diagnostics(ast.version, &policy);
        Ok(SymbolLibDocument {
            ast,
            cst,
            diagnostics,
            ast_dirty: false,
            policy,
        })
    }
}

fn parse_ast(cst: &CstDocument) -> SymbolLibAst {
    let mut version = None;
    let mut generator = None;
    let mut generator_version = None;
    let mut symbols = Vec::new();
    let mut unknown_nodes = Vec::new();

    if let Some(Node::List { items, .. }) = cst.nodes.first() {
        for item in items.iter().skip(1) {
            match head_of(item) {
                Some("version") => version = second_atom_i32(item),
                Some("generator") => generator = second_atom_string(item),
                Some("generator_version") => generator_version = second_atom_string(item),
                Some("symbol") => symbols.push(parse_symbol_summary(item)),
                _ => {
                    if let Some(unknown) = UnknownNode::from_node(item) {
                        unknown_nodes.push(unknown);
                    }
                }
            }
        }
    }

    let symbol_count = symbols.len();
    let total_property_count = symbols.iter().map(|s| s.property_count).sum();
    let total_pin_count = symbols.iter().map(|s| s.pin_count).sum();

    SymbolLibAst {
        version,
        generator,
        generator_version,
        symbol_count,
        total_property_count,
        total_pin_count,
        symbols,
        unknown_nodes,
    }
}

fn parse_pin_summary(node: &Node) -> Option<PinSummary> {
    let Node::List { items, .. } = node else {
        return None;
    };
    // (pin <electrical_type> <graphical_style> (at x y angle) (length l) (name "..." ...) (number "..." ...))
    let electrical_type = items.get(1).and_then(atom_as_string);
    let graphical_style = items.get(2).and_then(atom_as_string);

    let mut x = None;
    let mut y = None;
    let mut angle = None;
    let mut length = None;
    let mut name = None;
    let mut number = None;

    for child in items.iter().skip(3) {
        match head_of(child) {
            Some("at") => {
                let Node::List {
                    items: at_items, ..
                } = child
                else {
                    continue;
                };
                x = at_items.get(1).and_then(atom_as_f64);
                y = at_items.get(2).and_then(atom_as_f64);
                angle = at_items.get(3).and_then(atom_as_f64);
            }
            Some("length") => {
                length = second_atom_f64(child);
            }
            Some("name") => {
                let Node::List {
                    items: name_items, ..
                } = child
                else {
                    continue;
                };
                name = name_items.get(1).and_then(atom_as_string);
            }
            Some("number") => {
                let Node::List {
                    items: num_items, ..
                } = child
                else {
                    continue;
                };
                number = num_items.get(1).and_then(atom_as_string);
            }
            _ => {}
        }
    }

    Some(PinSummary {
        electrical_type,
        graphical_style,
        name,
        number,
        x,
        y,
        angle,
        length,
    })
}

fn collect_pins_recursive(node: &Node, pins: &mut Vec<PinSummary>) {
    let Node::List { items, .. } = node else {
        return;
    };
    for child in items.iter().skip(1) {
        if head_of(child) == Some("pin") {
            if let Some(p) = parse_pin_summary(child) {
                pins.push(p);
            }
        } else {
            collect_pins_recursive(child, pins);
        }
    }
}

fn is_general_symbol_property(key: &str) -> bool {
    !matches!(key, "ki_keywords" | "ki_fp_filters")
}

fn parse_symbol_summary(node: &Node) -> SymbolSummary {
    let Node::List { items, .. } = node else {
        return SymbolSummary {
            name: None,
            properties: Vec::new(),
            property_count: 0,
            pin_count: 0,
            unit_count: 0,
            has_embedded_fonts: false,
            pins: Vec::new(),
        };
    };

    let name = items.get(1).and_then(atom_as_string);
    let properties: Vec<(String, String)> = items
        .iter()
        .skip(2)
        .filter(|child| head_of(child) == Some("property"))
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
            if is_general_symbol_property(&key) {
                Some((key, val))
            } else {
                None
            }
        })
        .collect();
    let property_count = properties.len();
    let unit_count = items
        .iter()
        .skip(2)
        .filter(|child| head_of(child) == Some("symbol"))
        .count();
    let mut pins = Vec::new();
    collect_pins_recursive(node, &mut pins);
    let pin_count = pins.len();
    let has_embedded_fonts = items
        .iter()
        .skip(2)
        .any(|child| head_of(child) == Some("embedded_fonts"));

    SymbolSummary {
        name,
        properties,
        property_count,
        pin_count,
        unit_count,
        has_embedded_fonts,
        pins,
    }
}

fn find_symbol_index(items: &[Node], name: &str) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, node)| {
            if head_of(node) != Some("symbol") {
                return false;
            }
            match node {
                Node::List {
                    items: symbol_items,
                    ..
                } => symbol_items.get(1).and_then(atom_as_string).as_deref() == Some(name),
                _ => false,
            }
        })
        .map(|(idx, _)| idx)
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
        std::env::temp_dir().join(format!("{name}_{nanos}.kicad_sym"))
    }

    #[test]
    fn read_symbol_lib_and_preserve_lossless() {
        let path = tmp_file("sym_read_ok");
        let src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor) (symbol \"R\" (property \"Reference\" \"R\") (symbol \"R_0_0\" (pin passive line (at 0 0 0) (length 1)))))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = SymbolLibFile::read(&path).expect("read");
        assert_eq!(doc.ast().version, Some(20260101));
        assert_eq!(doc.ast().generator.as_deref(), Some("kicad_symbol_editor"));
        assert_eq!(doc.ast().symbol_count, 1);
        assert_eq!(doc.ast().total_property_count, 1);
        assert_eq!(doc.ast().total_pin_count, 1);
        assert_eq!(doc.cst().to_lossless_string(), src);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn captures_unknown_nodes_roundtrip() {
        let path = tmp_file("sym_unknown");
        let src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor) (future_block 1 2) (symbol \"R\"))\n";
        fs::write(&path, src).expect("write fixture");

        let doc = SymbolLibFile::read(&path).expect("read");
        assert_eq!(doc.ast().unknown_nodes.len(), 1);

        let out = tmp_file("sym_unknown_out");
        doc.write(&out).expect("write");
        let got = fs::read_to_string(&out).expect("read out");
        assert_eq!(got, src);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn edit_roundtrip_updates_symbol_metadata() {
        let path = tmp_file("sym_edit");
        let src = "(kicad_symbol_lib (version 20241209) (generator kicad_symbol_editor)\n  (symbol \"Old\" (property \"Reference\" \"U\") (property \"Value\" \"Old\") (symbol \"Old_0_0\" (pin input line (at 0 0 0) (length 2))))\n)\n";
        fs::write(&path, src).expect("write fixture");

        let mut doc = SymbolLibFile::read(&path).expect("read");
        doc.set_version(20260101)
            .set_generator("kiutils")
            .set_generator_version("dev")
            .rename_symbol("Old", "New")
            .upsert_symbol_property("New", "Value", "NewValue")
            .remove_symbol_property("New", "Reference");

        let out = tmp_file("sym_edit_out");
        doc.write(&out).expect("write");
        let reread = SymbolLibFile::read(&out).expect("reread");

        assert_eq!(reread.ast().version, Some(20260101));
        assert_eq!(reread.ast().generator.as_deref(), Some("kiutils"));
        assert_eq!(reread.ast().generator_version.as_deref(), Some("dev"));
        assert_eq!(
            reread.ast().symbols.first().and_then(|s| s.name.clone()),
            Some("New".to_string())
        );
        assert_eq!(
            reread.ast().symbols.first().map(|s| s.property_count),
            Some(1)
        );
        assert_eq!(reread.ast().total_pin_count, 1);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(out);
    }

    #[test]
    fn excludes_kicad_symbol_metadata_from_general_property_summary() {
        let path = tmp_file("sym_special_props");
        let src = "(kicad_symbol_lib (version 20260101) (generator kicad_symbol_editor)\n  (symbol \"Demo\"\n    (property \"Reference\" \"U\")\n    (property \"Value\" \"Demo\")\n    (property \"ki_keywords\" \"opamp amplifier\")\n    (property \"ki_fp_filters\" \"SOIC* DIP*\")\n  )\n)\n";
        fs::write(&path, src).expect("write fixture");

        let doc = SymbolLibFile::read(&path).expect("read");
        let sym = doc.ast().symbols.first().expect("symbol");

        assert_eq!(
            sym.properties,
            vec![
                ("Reference".to_string(), "U".to_string()),
                ("Value".to_string(), "Demo".to_string()),
            ]
        );
        assert_eq!(sym.property_count, 2);
        assert_eq!(doc.cst().to_lossless_string(), src);

        let _ = fs::remove_file(path);
    }

    // ── pin extraction tests ──────────────────────────────────────────────

    const SINGLE_PIN_SYM: &str = concat!(
        "(kicad_symbol_lib (version 20231120) (generator test)\n",
        "  (symbol \"MyLib:Inv\"\n",
        "    (property \"Reference\" \"U\")\n",
        "    (symbol \"MyLib:Inv_1_1\"\n",
        "      (pin input line (at -2.54 0 0) (length 2.54)\n",
        "        (name \"A\" (effects (font (size 1.27 1.27))))\n",
        "        (number \"1\" (effects (font (size 1.27 1.27)))))\n",
        "      (pin output line (at 2.54 0 180) (length 2.54)\n",
        "        (name \"Y\" (effects (font (size 1.27 1.27))))\n",
        "        (number \"2\" (effects (font (size 1.27 1.27))))))))\n",
    );

    #[test]
    fn pin_count_matches_extracted_pins() {
        let path = tmp_file("pin_count");
        fs::write(&path, SINGLE_PIN_SYM).expect("write");
        let doc = SymbolLibFile::read(&path).expect("read");
        let sym = doc.ast().symbols.first().expect("symbol");
        assert_eq!(sym.pin_count, 2);
        assert_eq!(sym.pins.len(), 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn pin_fields_input_pin() {
        let path = tmp_file("pin_fields_input");
        fs::write(&path, SINGLE_PIN_SYM).expect("write");
        let doc = SymbolLibFile::read(&path).expect("read");
        let sym = doc.ast().symbols.first().expect("symbol");
        let pin1 = sym
            .pins
            .iter()
            .find(|p| p.number.as_deref() == Some("1"))
            .expect("pin 1");
        assert_eq!(pin1.electrical_type.as_deref(), Some("input"));
        assert_eq!(pin1.graphical_style.as_deref(), Some("line"));
        assert_eq!(pin1.name.as_deref(), Some("A"));
        assert_eq!(pin1.x, Some(-2.54));
        assert_eq!(pin1.y, Some(0.0));
        assert_eq!(pin1.angle, Some(0.0));
        assert_eq!(pin1.length, Some(2.54));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn pin_fields_output_pin() {
        let path = tmp_file("pin_fields_output");
        fs::write(&path, SINGLE_PIN_SYM).expect("write");
        let doc = SymbolLibFile::read(&path).expect("read");
        let sym = doc.ast().symbols.first().expect("symbol");
        let pin2 = sym
            .pins
            .iter()
            .find(|p| p.number.as_deref() == Some("2"))
            .expect("pin 2");
        assert_eq!(pin2.electrical_type.as_deref(), Some("output"));
        assert_eq!(pin2.name.as_deref(), Some("Y"));
        assert_eq!(pin2.x, Some(2.54));
        assert_eq!(pin2.angle, Some(180.0));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn multi_unit_pin_extraction_fixture() {
        // Uses the corpus fixture: Device:R (2 pins), Op:LM358 (8 pins across 3 sub-symbols)
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/multi_unit_sym.kicad_sym");
        let doc = SymbolLibFile::read(&fixture).expect("read fixture");
        let syms = &doc.ast().symbols;

        let r = syms
            .iter()
            .find(|s| s.name.as_deref() == Some("Device:R"))
            .expect("Device:R");
        assert_eq!(r.pin_count, 2);
        assert_eq!(r.pins.len(), 2);
        assert!(r
            .pins
            .iter()
            .all(|p| p.electrical_type.as_deref() == Some("passive")));
        // pin numbers are "1" and "2"
        assert!(r.pins.iter().any(|p| p.number.as_deref() == Some("1")));
        assert!(r.pins.iter().any(|p| p.number.as_deref() == Some("2")));

        let lm = syms
            .iter()
            .find(|s| s.name.as_deref() == Some("Op:LM358"))
            .expect("Op:LM358");
        assert_eq!(lm.pin_count, 8);
        assert_eq!(lm.pins.len(), 8);
        // power pins
        assert!(lm
            .pins
            .iter()
            .any(|p| p.number.as_deref() == Some("4") && p.name.as_deref() == Some("V-")));
        assert!(lm
            .pins
            .iter()
            .any(|p| p.number.as_deref() == Some("8") && p.name.as_deref() == Some("V+")));
        // op-amp inputs on unit 1
        assert!(lm
            .pins
            .iter()
            .any(|p| p.number.as_deref() == Some("3")
                && p.electrical_type.as_deref() == Some("input")));
        assert!(lm
            .pins
            .iter()
            .any(|p| p.number.as_deref() == Some("1")
                && p.electrical_type.as_deref() == Some("output")));
    }

    #[test]
    fn total_pin_count_matches_pin_vec_sum() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/multi_unit_sym.kicad_sym");
        let doc = SymbolLibFile::read(&fixture).expect("read");
        let sum: usize = doc.ast().symbols.iter().map(|s| s.pins.len()).sum();
        assert_eq!(doc.ast().total_pin_count, sum);
    }
}
