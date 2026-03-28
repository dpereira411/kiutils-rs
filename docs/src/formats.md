# Supported Formats

## File types

| Extension | Type | Read | Write | Structural edit |
|---|---|---|---|---|
| `.kicad_pro` | Project (JSON) | ✓ | ✓ | ✓ |
| `.kicad_sch` | Schematic | ✓ | ✓ | ✓ |
| `.kicad_sym` | Symbol library | ✓ | ✓ | property/rename |
| `.kicad_pcb` | PCB layout | ✓ | ✓ | ✓ |
| `.kicad_mod` | Footprint | ✓ | ✓ | — |
| `fp-lib-table` | Footprint library table | ✓ | ✓ | ✓ |
| `sym-lib-table` | Symbol library table | ✓ | ✓ | ✓ |
| `.kicad_dru` | Design rules | ✓ | ✓ | — |
| `.kicad_wks` | Worksheet | ✓ | ✓ | — |

## Write modes

| Mode | Behavior |
|---|---|
| `WriteMode::Lossless` | Preserves all whitespace, comments, and unknown tokens |
| `WriteMode::Canonical` | Emits a normalized, consistently-indented representation |

Lossless mode is the default. Use Canonical when producing a clean baseline or
when diffing output in CI.

## Schematic editing surface

| Operation | CLI command | Rust method |
|---|---|---|
| Inspect all symbols, wires, labels | `schematic inspect` | `doc.symbol_instances()` |
| Set a symbol property | `schematic set-property` | `doc.upsert_symbol_instance_property()` |
| Remove a symbol property | `schematic remove-property` | `doc.remove_symbol_instance_property()` |
| Add a symbol instance | `schematic add-symbol` | `doc.add_symbol_instance()` |
| Remove a symbol instance | `schematic remove-symbol` | `doc.remove_symbol_instance()` |
| Change a symbol's library reference | `schematic rename` | `doc.set_symbol_lib_id()` |
| Add a wire | `schematic add-wire` | `doc.add_wire()` |
| Remove a wire by exact coordinates | `schematic remove-wire` | `doc.remove_wire_at()` |
| Add a net label | `schematic add-label` | `doc.add_label()` |
| Add a global label | `schematic add-global-label` | `doc.add_global_label()` |
| Add a junction | `schematic add-junction` | `doc.add_junction()` |
| Add a no-connect marker | `schematic add-no-connect` | `doc.add_no_connect()` |

## PCB editing surface

| Operation | CLI command | Rust method |
|---|---|---|
| Inspect footprints, nets, traces | `pcb inspect` | `doc.ast()` |
| Set a board property | `pcb set-property` | `doc.upsert_property()` |
| Add a trace segment | `pcb add-trace` | `doc.add_trace()` |
| Remove a trace by coordinates | `pcb remove-trace` | `doc.remove_trace_at()` |
| Add a via | `pcb add-via` | `doc.add_via()` |
| Add a footprint | `pcb add-footprint` | `doc.add_footprint()` |
| Remove a footprint | `pcb remove-footprint` | `doc.remove_footprint()` |
