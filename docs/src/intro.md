# Introduction

`kiutils-rs` is a Rust workspace for **lossless KiCad file parsing and editing**.
It is designed as a backend for AI agents and code generators that need to read,
inspect, and structurally modify KiCad project files without corrupting formatting
or discarding unknown tokens.

## Workspace layers

| Crate | Role |
|---|---|
| `kiutils_sexpr` | Lossless S-expression CST parser/printer |
| `kiutils_kicad` | Typed KiCad document layer + mutation helpers |
| `kiutils` (`kiutils-rs`) | Stable public API for library consumers |

## Design goals

- **Lossless by default** — `WriteMode::Lossless` preserves unrelated formatting and unknown tokens
- **Canonical output on demand** — `WriteMode::Canonical` emits normalized output for diffs
- **Forward-compatible** — unknown S-expression tokens are captured and round-tripped
- **Agent-friendly library APIs** — typed read/edit/write helpers make structured mutations straightforward

## Version scope

- Primary target: KiCad v9 / v10
- File format version numbers are preserved as-read unless explicitly overwritten

## Install (library)

```toml
[dependencies]
kiutils-rs = "0.1"
```

The end-user CLI now lives outside this repository. `kiutils-rs` remains the
library source of truth.
