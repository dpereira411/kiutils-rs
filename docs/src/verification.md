# Verification

## Test suite overview

```bash
cargo test         # all crates
cargo test -p kiutils_kicad   # library + integration tests
cargo test -p kiutils_kicad --test inspect_cli   # inspect binary contract tests
```

As of the current release:
- **120+** unit and integration tests in `kiutils_kicad`
- **18** inspect-binary JSON/text smoke tests in `kiutils_kicad`
- Deep mutation tests covering field correctness, write+reread round-trips,
  no-op safety, and symmetric add/remove for all structural edit operations

## What is tested

| Area | Coverage |
|---|---|
| Lossless round-trips | PCB, schematic, symbol-lib, footprint, design rules, worksheet |
| Structural schematic edits | add/remove symbol, wire, label, global-label, junction, no-connect, rename |
| Structural PCB edits | add/remove trace, via, footprint |
| Write + reread verification | All structural ops: field values verified after `write()` + fresh `read()` |
| No-op safety | Remove ops on non-matching coordinates/references leave document unchanged |
| Specificity | Remove ops target only the exact element, not adjacent ones |
| Symmetric add/remove | `count_after_add_then_remove == count_before` |
| Inspect binary contracts | `kiutils-inspect` JSON/text output shape |
| Validation diagnostics | Missing libs, duplicate entries, unresolved references |
| Corpus fixtures | Multi-unit symbols, hierarchical schematics, complex PCBs with zones/groups |

## Local gate

```bash
cargo fmt --all
cargo test
cargo clippy --all-targets --all-features -- -D warnings
mdbook build docs
```

## Integration test examples

Round-trip (output must equal input byte-for-byte in Lossless mode):

```rust
{{#include ../../crates/kiutils_kicad/tests/integration.rs:pcb_roundtrip_test}}
```

Structural edit verified after write+reread (from `schematic.rs` test suite):

```rust,no_run
#[test]
fn add_symbol_survives_write_reread() {
    let mut doc = SchematicDocument::from_str(MINIMAL_SCH).unwrap();
    doc.add_symbol_instance("Device:R", "R55", "1k", 10.0, 20.0);
    let tmp = tmp_file("add_sym_reread", "kicad_sch");
    doc.write(&tmp).unwrap();
    let doc2 = SchematicDocument::read(&tmp).unwrap();
    let syms = doc2.symbol_instances();
    let r55 = syms.iter().find(|s| s.reference.as_deref() == Some("R55")).unwrap();
    assert_eq!(r55.lib_id.as_deref(), Some("Device:R"));
    assert_eq!(r55.value.as_deref(), Some("1k"));
}
```
