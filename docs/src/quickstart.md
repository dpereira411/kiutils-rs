# Quickstart

`kiutils-rs` is a library workspace. The quickest way to get value is to read a
KiCad file, make a typed edit, and write it back out in lossless mode.

## Rust API — parse, edit, write

```rust,no_run
use kiutils_rs::SchematicFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = SchematicFile::read("my.kicad_sch")?;

    // Add a symbol instance and wire it
    doc.add_symbol_instance("Device:R", "R42", "10k", 120.0, 80.0)
       .add_wire(120.0, 80.0, 160.0, 80.0)
       .upsert_symbol_instance_property("R42", "Footprint",
           "Resistor_SMD:R_0402_1005Metric");

    doc.write("my.kicad_sch")?;
    Ok(())
}
```

## Rust API — inspect without mutating

```rust,no_run
use kiutils_rs::ProjectFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = ProjectFile::read("my.kicad_pro")?;
    println!("schema version: {:?}", project.version());
    println!("pinned symbol libs: {}", project.pinned_symbol_libs().len());
    Ok(())
}
```

## Rust API — PCB editing

```rust,no_run
use kiutils_rs::PcbFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PcbFile::read("my.kicad_pcb")?;

    doc.add_footprint("Resistor_SMD:R_0402_1005Metric",
                      120.0, 80.0, "F.Cu", "R42", "10k")
       .add_trace(120.0, 80.0, 160.0, 80.0, 0.2, "F.Cu", 1)
       .add_via(160.0, 80.0, 0.8, 0.4, 1);

    doc.write("my.kicad_pcb")?;
    Ok(())
}
```

## Internal inspect tool

For parser/CST debugging during library development:

```bash
cargo run -p kiutils_kicad --bin kiutils-inspect -- my.kicad_sch --show-diagnostics
```

## Build and verify

```bash
cargo build
cargo test
mdbook build docs
```
