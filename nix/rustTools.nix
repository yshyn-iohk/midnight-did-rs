{ rust-bin, rust-overlay }:

let
  nightlyVersion = "2026-03-18";
  rustOverrideArgs = {
    extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
    targets    = [ ];
  };
in
{
  rust =
    rust-bin.nightly.${nightlyVersion}.default.override rustOverrideArgs;
}
