{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, ... }:
    let
      inherit (midnightDidRsLib.rustTools) rust;
    in
    {
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rust
          just
          taplo
          cargo-nextest
          git
          jq
        ];

        shellHook = ''
          export ROOT_DIR=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
          cd "$ROOT_DIR"
          echo "Entered midnight-did-rs devshell. Run 'just --list' for available commands."
        '';

        env = {
          RUST_LOG = "info";
        };
      };
    };
}
