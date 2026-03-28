//! # kiutils-kicad
//!
//! Typed KiCad file document layer built on top of `kiutils-sexpr`.
//!
//! If you want stable end-user imports, use [`kiutils-rs`](https://docs.rs/kiutils-rs).
//! This crate exposes the implementation-layer API and additional file families.
//!
//! ## Scope
//! - `.kicad_pcb`
//! - `.kicad_mod`
//! - `.kicad_sch`
//! - `.kicad_sym`
//! - `fp-lib-table`
//! - `sym-lib-table`
//! - `.kicad_dru`
//! - `.kicad_pro`
//! - `.kicad_wks`
//!
//! ## Core behavior
//! - Default write mode is lossless (`WriteMode::Lossless`)
//! - Unknown tokens are captured on typed ASTs (`unknown_nodes`, `unknown_fields`)
//! - `write_mode(..., WriteMode::Canonical)` available for normalized output
//! - Version diagnostics produced post-parse for forward-compat signaling
//!
//! Evidence:
//! - Round-trip + unknown preservation tests:
//!   <https://github.com/Milind220/kiutils-rs/blob/main/crates/kiutils_kicad/tests/integration.rs>
//! - Inspect binary contract tests (`kiutils-inspect`):
//!   <https://github.com/Milind220/kiutils-rs/blob/main/crates/kiutils_kicad/tests/inspect_cli.rs>
//!
//! ## Quickstart
//! ```rust,no_run
//! use kiutils_kicad::{SchematicFile, WriteMode};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let doc = SchematicFile::read("input.kicad_sch")?;
//!     doc.write_mode("output.kicad_sch", WriteMode::Lossless)?;
//!     Ok(())
//! }
//! ```
//!
//! Policy notes:
//! - AST `*_count` fields are convenience counters, not strict stability promises.
//! - Unknown-token diagnostics are developer-facing; summarize before showing end users.
//! - `.kicad_dru` rule conditions are preserved as strings in v1.

mod batch;
mod diagnostic;
mod dru;
mod error;
mod footprint;
mod lib_table;
mod pcb;
mod project;
mod schematic;
mod sections;
mod sexpr_edit;
mod sexpr_utils;
mod symbol;
mod unknown;
mod version;
mod version_diag;
mod worksheet;
mod write_mode;

pub use batch::{
    load_schematic_tree, read_pcbs, read_pcbs_from_refs, read_schematics,
    read_schematics_from_refs, read_symbol_libs, read_symbol_libs_from_refs,
};
pub use diagnostic::{Diagnostic, Severity, Span};
pub use dru::{DesignRuleSummary, DesignRulesAst, DesignRulesDocument, DesignRulesFile};
pub use error::Error;
pub use footprint::{FootprintAst, FootprintDocument, FootprintFile};
pub use lib_table::{
    FpLibTableAst, FpLibTableDocument, FpLibTableFile, LibTableKind, SymLibTableAst,
    SymLibTableDocument, SymLibTableFile,
};
pub use pcb::{
    PcbArcSummary, PcbAst, PcbDimensionSummary, PcbDocument, PcbFile, PcbFootprintSummary,
    PcbGeneratedSummary, PcbGraphicSummary, PcbGroupSummary, PcbLayer, PcbNet, PcbProperty,
    PcbSegmentSummary, PcbSetupSummary, PcbTargetSummary, PcbViaSummary, PcbZoneSummary,
};
pub use project::{ProjectAst, ProjectDocument, ProjectExtra, ProjectFile};
pub use schematic::{
    fork_symbol_to_lib, merge_sheet_netlists, push_symbol_to_lib, rename_symbol_in_schematic,
    replace_symbol_from_lib, replace_symbol_from_lib_with_library_name,
    replace_symbol_from_lib_with_library_name_with_options, replace_symbol_from_lib_with_options,
    update_symbols_from_lib, update_symbols_from_lib_with_options, ForkSymbolToLibOptions, NetPin,
    SchematicAst, SchematicDocument, SchematicFile, SchematicLabelSummary, SchematicNet,
    SchematicNetlist, SchematicPaperSummary, SchematicSymbolInfo, SchematicTitleBlockSummary,
    SchematicWireSummary, UpdateFromLibOptions, UpdateFromLibReport,
};
pub use symbol::{PinSummary, SymbolLibAst, SymbolLibDocument, SymbolLibFile, SymbolSummary};
pub use unknown::{UnknownField, UnknownNode};
pub use version::{KiCadSeries, VersionPolicy};
pub use worksheet::{WorksheetAst, WorksheetDocument, WorksheetFile, WorksheetSetupSummary};
pub use write_mode::WriteMode;
