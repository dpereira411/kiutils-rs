# Agent Cookbook

Recipes for AI agents operating on KiCad projects through the Rust API.

---

## Recipe 1: Inspect a project before editing

Always inspect first. Understand what's already there before adding anything.

```rust,no_run
use kiutils_rs::{PcbFile, ProjectFile, SchematicFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = ProjectFile::read("my.kicad_pro")?;
    let schematic = SchematicFile::read("my.kicad_sch")?;
    let pcb = PcbFile::read("my.kicad_pcb")?;

    println!("project version: {:?}", project.version());
    println!("symbol count: {}", schematic.symbol_instances().len());
    println!("footprint count: {}", pcb.footprints().len());
    Ok(())
}
```

The typed APIs expose the same planning inputs directly: references, library
IDs, values, footprints, properties, and diagnostics.

---

## Recipe 2: Add a decoupling capacitor

```rust,no_run
use kiutils_rs::SchematicFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut schematic = SchematicFile::read("my.kicad_sch")?;
    let reference = "C99";
    let x = 145.0;
    let y = 65.0;

    schematic
        .add_symbol_instance("Device:C", reference, "100n", x, y)
        .upsert_symbol_instance_property(
            reference,
            "Footprint",
            "Capacitor_SMD:C_0402_1005Metric",
        )
        .upsert_symbol_instance_property(reference, "Value", "100n")
        .add_wire(x, y, x + 20.0, y)
        .add_label("VCC", x, y, 0.0)
        .add_label("GND", x + 20.0, y, 0.0);

    schematic.write("my.kicad_sch")?;
    Ok(())
}
```

---

## Recipe 3: Replace a component's library reference

When renaming a placed symbol to a new `lib_id` while keeping its current
embedded symbol body:

```rust,no_run
use kiutils_kicad::rename_symbol_in_schematic;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    rename_symbol_in_schematic("my.kicad_sch", "R1", "Device:R")?;
    rename_symbol_in_schematic("my.kicad_sch", "R2", "Device:R")?;
    Ok(())
}
```

This command clones the current embedded `lib_symbols` entry under the new
name if needed. It does not fetch a different symbol body from a `.kicad_sym`
library.

When the target symbol already exists in a library and you want the schematic to
adopt that library body instead:

```rust,no_run
use kiutils_kicad::{
    replace_symbol_from_lib_with_library_name_with_options, UpdateFromLibOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    replace_symbol_from_lib_with_library_name_with_options(
        "my.kicad_sch",
        "J4",
        "MyLib",
        "MyPowerHeader",
        UpdateFromLibOptions::default(),
    )?;

    replace_symbol_from_lib_with_library_name_with_options(
        "my.kicad_sch",
        "J4",
        "MyLib",
        "MyPowerHeader",
        UpdateFromLibOptions {
            override_value: true,
            ..UpdateFromLibOptions::default()
        },
    )?;
    Ok(())
}
```

---

## Recipe 4: Route a trace on the PCB

```rust,no_run
use kiutils_rs::PcbFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pcb = PcbFile::read("my.kicad_pcb")?;
    let net = 3;

    pcb.add_trace(100.0, 50.0, 140.0, 50.0, 0.2, "F.Cu", net)
        .add_via(140.0, 50.0, 0.8, 0.4, net)
        .add_trace(140.0, 50.0, 180.0, 50.0, 0.2, "B.Cu", net);

    pcb.write("my.kicad_pcb")?;
    Ok(())
}
```

---

## Recipe 5: Add a footprint to the PCB

```rust,no_run
use kiutils_rs::PcbFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pcb = PcbFile::read("my.kicad_pcb")?;
    pcb.add_footprint(
        "Resistor_SMD:R_0402_1005Metric",
        100.0,
        50.0,
        "F.Cu",
        "R42",
        "10k",
    );
    pcb.write("my.kicad_pcb")?;
    Ok(())
}
```

For back-copper placement use `B.Cu` as the layer.

---

## Recipe 6: Manage library tables

```rust,no_run
use kiutils_rs::FpLibTableFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut table = FpLibTableFile::read("fp-lib-table")?;
    table.upsert_library("MyParts", "${KIPRJMOD}/lib/MyParts.pretty");
    table.write("fp-lib-table")?;
    Ok(())
}
```

---

## Recipe 7: Read a project file for application-layer validation

```rust,no_run
use kiutils_rs::ProjectFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = ProjectFile::read("my.kicad_pro")?;
    println!("pinned symbol libs: {:?}", project.ast().pinned_symbol_libs);
    Ok(())
}
```

---

## Agent loop pattern

```
while not done:
    1. read      -> understand current state
    2. inspect   -> query typed fields and relationships
    3. mutate    -> call API helpers on the document
    4. write     -> persist the updated file
    5. reread    -> verify the change on a fresh parse
    6. validate  -> inspect diagnostics before the next mutation
```

Never batch mutations without a write+reread verification step when correctness
matters.
