# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-03-21

### Added

- Promote `.kicad_sch`, `.kicad_sym`, and `.kicad_wks` to public API (`SchematicFile`, `SymbolLibFile`, `WorksheetFile` and associated types)
- `SchematicDocument::symbol_instances()`, `sheet_filenames()`, `upsert_symbol_instance_property()`, `remove_symbol_instance_property()`
- `SymbolLibDocument::rename_symbol()`, `upsert_symbol_property()`, `remove_symbol_property()`
- `WorksheetDocument::set_setup_line_width()`, `set_setup_text_size()`
- `KiCadProject::open(path)` — loads `.kicad_pro`, root schematic tree, PCB, and lib tables in one call
- `load_schematic_tree(root)` — recursive BFS sub-sheet loader with cycle detection
- `KiCadProject::root_schematic()`, `primary_pcb()`, `is_clean()`

## [0.1.2](https://github.com/Milind220/kiutils-rs/compare/kiutils-rs-v0.1.1...kiutils-rs-v0.1.2) - 2026-03-02

### Fixed

- add safe project pinned-lib setters and lib-table URI upsert ([#20](https://github.com/Milind220/kiutils-rs/pull/20))

## [0.1.1](https://github.com/Milind220/kiutils-rs/compare/kiutils-rs-v0.1.0...kiutils-rs-v0.1.1) - 2026-02-28

### Fixed

- export SymLibTable facade types and refresh docs ([#16](https://github.com/Milind220/kiutils-rs/pull/16))

### Other

- Merge pull request #13 from Milind220/codex/cargo-discovery-metadata

## [0.1.0](https://github.com/Milind220/kiutils-rs/releases/tag/kiutils-rs-v0.1.0) - 2026-02-26

### Added

- *(api)* add kiutils-rs public facade crate
