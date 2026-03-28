# kiutils-rs justfile
# https://github.com/casey/just

# Default: list recipes
default:
    @just --list

# ── Build ─────────────────────────────────────────────────────────────────────

# Build all crates (debug)
build:
    cargo build --workspace

# Build workspace artifacts in release mode
release:
    cargo build --workspace --release

# ── Test ──────────────────────────────────────────────────────────────────────

# Run all tests
test:
    cargo test --workspace

# Run tests for a specific crate  (e.g. `just test-crate kiutils_kicad`)
test-crate crate:
    cargo test -p {{crate}}

# Run tests and show output for failing tests
test-verbose:
    cargo test --workspace -- --nocapture

# ── Lint / Format ─────────────────────────────────────────────────────────────

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Apply formatting
fmt:
    cargo fmt --all

# Run clippy
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Full local gate: fmt-check + lint + test
gate: fmt-check lint test

# ── Docs ──────────────────────────────────────────────────────────────────────

# Build the mdbook
docs:
    mdbook build docs

# Serve the mdbook with live-reload on http://localhost:3000
docs-serve:
    mdbook serve docs --open

# Build rustdoc for all crates
rustdoc:
    cargo doc --workspace --no-deps

# Build and open rustdoc
rustdoc-open:
    cargo doc --workspace --no-deps --open

# ── Maintenance ───────────────────────────────────────────────────────────────

# Remove build artifacts
clean:
    cargo clean

# Remove build artifacts and mdbook output
clean-all:
    cargo clean
    rm -rf docs/book
