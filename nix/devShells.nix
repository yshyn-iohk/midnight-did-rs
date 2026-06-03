{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, midnightLedgerSrc, midnightZkSrc, compactRuntimeRsSrc, compactRuntimeRsMacrosSrc, compactPkg, ... }:
    let
      inherit (midnightDidRsLib.rustTools) rust;
    in
    {
      devShells.default = pkgs.mkShell {
        packages = [ compactPkg ] ++ (with pkgs; [
          rust
          just
          taplo
          cargo-nextest
          git
          jq
        ]);

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

          # Materialize third_party/midnight-zk as a symlink to the nix-store path.
          # Provides the patched `midnight-proofs` crate referenced by
          # [patch.crates-io] in the root Cargo.toml. See ADR 0006.
          TARGET="${midnightZkSrc}"
          LINK="$ROOT_DIR/third_party/midnight-zk"
          if [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$TARGET" ]; then
            :
          else
            rm -rf "$LINK"
            ln -s "$TARGET" "$LINK"
            echo "Linked $LINK -> $TARGET"
          fi

          # Materialise compact's runtime-rs + runtime-rs-macros subtrees inside
          # third_party/compact/, mirroring the in-repo layout so that the
          # relative path `../runtime-rs-macros` (in compact-runtime's Cargo.toml)
          # and `../runtime-rs` (in runtime-rs-macros' dev-deps) both resolve
          # correctly without aliasing.
          mkdir -p "$ROOT_DIR/third_party/compact"

          TARGET="${compactRuntimeRsSrc}"
          LINK="$ROOT_DIR/third_party/compact/runtime-rs"
          if [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$TARGET" ]; then
            :
          else
            rm -rf "$LINK"
            ln -s "$TARGET" "$LINK"
            echo "Linked $LINK -> $TARGET"
          fi

          TARGET="${compactRuntimeRsMacrosSrc}"
          LINK="$ROOT_DIR/third_party/compact/runtime-rs-macros"
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
