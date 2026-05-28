# Default target lists available recipes.
default:
    @just --list

# Re-generate Rust code from did.compact via the patched compact compiler.
#
# Per compactc --help: `compactc <flag> ... <source-pathname> <target-directory-pathname>`.
# With --rust + --skip-ts the compiler emits:
#   <target>/contract/lib.rs              -- Rust source (consumed)
#   <target>/Cargo.toml                   -- emitted Cargo manifest (NOT consumed; we maintain our own)
#   <target>/compiler/contract-info.json  -- analyzer metadata (not consumed in cycle 1)
#   <target>/zkir/<circuit>.zkir          -- ZKIR per exported circuit
#   <target>/keys/<circuit>.{prover,verifier}  -- proving + verifier keys (emitted by zkir tool)
# Source: third_party compact compiler/passes.ss (generate-everything pass).
codegen:
    @mkdir -p target-gen
    @rm -rf target-gen/contract-out
    git submodule update --init third_party/midnight-did
    compactc --rust --skip-ts \
        third_party/midnight-did/packages/contract/src/did.compact \
        target-gen/contract-out
    @mkdir -p crates/midnight-did/src/contract
    cp target-gen/contract-out/contract/lib.rs crates/midnight-did/src/contract/generated.rs
    @mkdir -p crates/midnight-did/assets/keys
    # Recursive find picks up keys whether they live in keys/, zkir/, or contract/.
    find target-gen/contract-out -name "*.zkir"     -exec cp {} crates/midnight-did/assets/keys/ \;
    find target-gen/contract-out -name "*.prover"   -exec cp {} crates/midnight-did/assets/keys/ \;
    find target-gen/contract-out -name "*.verifier" -exec cp {} crates/midnight-did/assets/keys/ \;
    cargo fmt -p midnight-did

# Verify re-running codegen produces no diff (regression signal for CI).
codegen-check: codegen
    git diff --exit-code -- crates/midnight-did/src/contract crates/midnight-did/assets/keys

build:
    cargo build --all-targets

test:
    cargo nextest run

fmt:
    cargo fmt
    taplo fmt

fmt-check:
    cargo fmt -- --check
    taplo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

ci: fmt-check lint build test
