{
  description = "Midnight DID — native Rust implementation";

  nixConfig = {
    extra-substituters     = [ "https://cache.iog.io" ];
    extra-trusted-public-keys = [ "hydra.iohk.io:f/Ea+s+dFdN+3Y/G+FDgSq+a5NEWhJGzdjvKNGv0/EQ=" ];
  };

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-parts.url  = "github:hercules-ci/flake-parts";
    midnight-ledger = {
      url = "github:yshyn-iohk/midnight-ledger/dioxus-vc-demo";
      flake = false;
    };
    # midnight-zk fork providing the patched `midnight-proofs` crate that
    # `midnight-ledger`'s `transient-crypto` references via [patch.crates-io].
    # Without this, `cargo build -p midnight-did-runtime` fails on the
    # ParamsKZG::{read_mmap_arc, write_mmap_companion, read_custom_lazy}
    # methods that only exist on the patched fork. See ADR 0006.
    midnight-zk = {
      url = "github:yshyn-iohk/midnight-zk/feat/v0.7-h-poly-streaming";
      flake = false;
    };
    compact = {
      url = "github:yshyn-iohk/compact/codegen-rust";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, flake-parts, ... }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      imports = [
        ./nix/devShells.nix
        ./nix/overlays.nix
        ./nix/compact.nix
      ];

      perSystem =
        { system, ... }:
        {
          _module.args = {
            inherit rust-overlay;
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };
            midnightDidRsLib = {
              rustTools = import ./nix/rustTools.nix {
                rust-bin     = (import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; }).rust-bin;
                rust-overlay = rust-overlay;
              };
              sources = {
                midnight-ledger = inputs.midnight-ledger;
                midnight-zk     = inputs.midnight-zk;
              };
            };
          };
        };
    };
}
