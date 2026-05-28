{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, midnightLedgerSrc, ... }:
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

          # Materialize third_party/midnight-ledger as a symlink to the nix-store path.
          TARGET="${midnightLedgerSrc}"
          LINK="$ROOT_DIR/third_party/midnight-ledger"
          mkdir -p "$ROOT_DIR/third_party"
          if [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$TARGET" ]; then
            :
          else
            rm -rf "$LINK"
            ln -s "$TARGET" "$LINK"
            echo "Linked $LINK -> $TARGET"
          fi

          echo "Entered midnight-did-rs devshell. Run 'just --list' for available commands."
        '';

        env = {
          RUST_LOG = "info";
        };
      };
    };
}
