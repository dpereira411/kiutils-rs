# Usage Patterns (Rust API)

This chapter is for consumers of the `kiutils-rs` library crate.
CLI users should see [Agent Cookbook](agent-cookbook.md) instead.

---

## Pattern: safe typed mutation

```rust,no_run
use kiutils_rs::{fork_symbol_to_lib, ForkSymbolToLibOptions, SchematicFile};

let mut doc = SchematicFile::read("my.kicad_sch")?;
doc.upsert_symbol_instance_property("R1", "Footprint",
    "Resistor_SMD:R_0402_1005Metric");
doc.write("my.kicad_sch")?;

fork_symbol_to_lib(
    "my.kicad_sch",
    "D15",
    "Custom.kicad_sym",
    "FastDiode",
    ForkSymbolToLibOptions { overwrite: false },
)?;
```

- Parse with `*File::read(...)`
- Update through document setters (`set_*`, `upsert_*`, `remove_*`, `add_*`)
- Write with `write(...)` or `write_mode(..., WriteMode::Canonical)`

**Why**: setter APIs reconcile the typed AST and the underlying CST together.
Mutating the raw AST via `ast_mut()` bypasses this reconciliation and will
produce inconsistent state on write.

---

## Schematic mutations

```rust,no_run
use kiutils_rs::SchematicFile;

let mut doc = SchematicFile::read("my.kicad_sch")?;

// Structural
doc.add_symbol_instance("Device:R", "R99", "10k", 120.0, 80.0);
doc.remove_symbol_instance("R99");
doc.set_symbol_lib_id("R1", "Device:R_Small");

// Wiring
doc.add_wire(100.0, 80.0, 140.0, 80.0);
doc.remove_wire_at(100.0, 80.0, 140.0, 80.0);

// Labels
doc.add_label("VCC", 140.0, 80.0, 0.0);
doc.add_global_label("VBUS", "power_in", 100.0, 60.0, 0.0);

// Markers
doc.add_junction(140.0, 80.0);
doc.add_no_connect(200.0, 80.0);

// Properties on existing symbols
doc.upsert_symbol_instance_property("R1", "Value", "4k7");
doc.remove_symbol_instance_property("R1", "ki_description");

doc.write("my.kicad_sch")?;
```

Methods return `&mut Self`, so you can chain them:

```rust,no_run
doc.add_symbol_instance("Device:C", "C10", "100n", 50.0, 80.0)
   .add_wire(50.0, 80.0, 80.0, 80.0)
   .add_label("VCC", 80.0, 80.0, 0.0);
```

---

## PCB mutations

```rust,no_run
use kiutils_rs::PcbFile;

let mut doc = PcbFile::read("my.kicad_pcb")?;

// Traces and vias
doc.add_trace(100.0, 50.0, 140.0, 50.0, 0.2, "F.Cu", 3);
doc.remove_trace_at(100.0, 50.0, 140.0, 50.0);
doc.add_via(140.0, 50.0, 0.8, 0.4, 3);

// Footprints
doc.add_footprint(
    "Resistor_SMD:R_0402_1005Metric",
    120.0, 80.0, "F.Cu", "R42", "10k",
);
doc.remove_footprint("R42");

// Board properties
doc.upsert_property("Rev", "B");
doc.remove_property("Obsolete");

doc.write("my.kicad_pcb")?;
```

---

## Inspecting schematic symbols

```rust,no_run
use kiutils_rs::SchematicFile;

let doc = SchematicFile::read("my.kicad_sch")?;
for sym in doc.symbol_instances() {
    println!(
        "{}: {} ({})",
        sym.reference.as_deref().unwrap_or("?"),
        sym.value.as_deref().unwrap_or("?"),
        sym.lib_id.as_deref().unwrap_or("?"),
    );
}
```

---

## Loading a hierarchical schematic tree

```rust,no_run
use kiutils_kicad::load_schematic_tree;

let results = load_schematic_tree("root.kicad_sch");
for result in results {
    let doc = result?;
    println!("{} symbols", doc.ast().symbol_count);
}
```

`load_schematic_tree` resolves sub-sheet references recursively and returns
one `Result<SchematicDocument, Error>` per file.

---

## Common pitfalls

| Pitfall | What happens | Correct pattern |
|---|---|---|
| Using `ast_mut()` then `write()` | Non-reconciled state; may panic or corrupt | Use setter/upsert helpers |
| Assuming unknown tokens are dropped | Other tools lose future KiCad syntax | `kiutils-rs` round-trips unknowns automatically |
| Always using Canonical mode | Noisy VCS diffs on unedited files | Default to Lossless; Canonical only for baselines |
| Removing wires by approximate coords | No match → silent no-op | Coordinates must match exactly as written in the file |
