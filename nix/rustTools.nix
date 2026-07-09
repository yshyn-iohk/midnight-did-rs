{ rust-bin, rust-overlay }:

let
  nightlyVersion = "2026-03-18";
  rustOverrideArgs = {
    extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
    targets    = [ "wasm32-unknown-unknown" ];
  };
in
{
  rust =
    rust-bin.nightly.${nightlyVersion}.default.override rustOverrideArgs;
}
