# kiutils-rs

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/Milind220/kiutils-rs)

Rust-native KiCad parser/formatter focused on lossless round-trips, typed APIs, and agent-driven workflows.

## Crates

| Crate | Import | Purpose |
|---|---|---|
| `kiutils_sexpr` | `kiutils_sexpr` | Lossless S-expression CST parser/printer |
| `kiutils_kicad` | `kiutils_kicad` | Typed KiCad document APIs (implementation layer) |
| `kiutils-rs` | `kiutils_rs` | Stable public facade |

## Supported File Types

`.kicad_pcb` · `.kicad_mod` · `.kicad_sch` · `.kicad_sym` · `fp-lib-table` · `sym-lib-table` · `.kicad_dru` · `.kicad_pro` · `.kicad_wks`

Compatibility: KiCad v10 primary, v9 secondary.

## Key Behavior

- Lossless output by default (`WriteMode::Lossless`) — unrelated formatting is never touched.
- Optional canonical normalized output (`WriteMode::Canonical`).
- Unknown token/field capture for forward compatibility.
- Typed mutation helpers (`set_*`, `upsert_*`, `remove_*`) for common edits.

## Quick API Example

```rust
use kiutils_rs::{SchematicFile, PcbFile};

// Read → edit → write (lossless)
let mut sch = SchematicFile::read("board.kicad_sch")?;
sch.upsert_symbol_instance_property("R1", "Value", "10k");
sch.write("board.kicad_sch")?;

let mut pcb = PcbFile::read("board.kicad_pcb")?;
pcb.set_generator("kiutils-rs-agent")
   .upsert_property("Owner", "EDA-Agent");
pcb.write("board.kicad_pcb")?;
```

## Development

Prerequisites: Rust toolchain + `mdbook` (via `nix develop` or `cargo install mdbook`).

```bash
just          # list all recipes
just gate     # fmt-check + lint + test (full local gate)
just test     # run all tests
just docs     # build mdbook → docs/book/
just docs-serve  # live-reload at http://localhost:3000
```

## Internal Inspect Tool

For parser/CST debugging inside this workspace:

```bash
cargo run -p kiutils_kicad --bin kiutils-inspect -- <path> --show-unknown --show-diagnostics
```

## License

MIT
