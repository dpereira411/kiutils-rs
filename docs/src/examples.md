# Examples

## PCB round-trip

```rust,no_run
use kiutils_rs::PcbFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PcbFile::read("input.kicad_pcb")?;
    doc.write("output.kicad_pcb")?;  // byte-for-byte identical
    Ok(())
}
```

Run the included example:

```bash
cargo run -p kiutils_kicad --example pcb_roundtrip -- input.kicad_pcb output.kicad_pcb
```

## Schematic round-trip

```rust,no_run
use kiutils_rs::SchematicFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = SchematicFile::read("input.kicad_sch")?;
    doc.write("output.kicad_sch")?;
    Ok(())
}
```

## Add resistor + wire + label

```rust,no_run
use kiutils_rs::SchematicFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = SchematicFile::read("my.kicad_sch")?;

    doc.add_symbol_instance("Device:R", "R99", "10k", 120.0, 80.0)
       .add_wire(120.0, 80.0, 160.0, 80.0)
       .add_label("VCC", 120.0, 80.0, 0.0)
       .upsert_symbol_instance_property(
           "R99", "Footprint", "Resistor_SMD:R_0402_1005Metric");

    doc.write("my.kicad_sch")?;
    Ok(())
}
```

## Corpus round-trips (batch validation)

```bash
# Validate all PCBs in a directory
cargo run -p kiutils_kicad --example pcb_corpus_roundtrip -- \
  /path/to/pcbs crates/kiutils_kicad/examples/generated/pcbs

# Schematics
cargo run -p kiutils_kicad --example schematic_corpus_roundtrip -- \
  /path/to/schematics crates/kiutils_kicad/examples/generated/schematics

# Symbol libraries
cargo run -p kiutils_kicad --example symlib_corpus_roundtrip -- \
  /path/to/sym-libs crates/kiutils_kicad/examples/generated/symlib
```

## Read all symbols from a symbol library

```rust,no_run
use kiutils_kicad::SymbolLibDocument;

let doc = SymbolLibDocument::read("Device.kicad_sym")?;
for sym in &doc.ast().symbols {
    println!("{}", sym.name.as_deref().unwrap_or("?"));
}
```

## Load a hierarchical schematic

```rust,no_run
use kiutils_kicad::load_schematic_tree;

for result in load_schematic_tree("top.kicad_sch") {
    let sheet = result?;
    println!("{} symbols in sheet", sheet.ast().symbol_count);
}
```
