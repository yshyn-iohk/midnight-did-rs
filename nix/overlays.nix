{ inputs, ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, system, ... }:
    {
      # Symlinks to third_party/midnight-ledger and third_party/compact-runtime-rs are
      # materialised by the devShells shellHook using these source paths.
      _module.args = {
        midnightLedgerSrc    = midnightDidRsLib.sources.midnight-ledger;
        midnightZkSrc        = midnightDidRsLib.sources.midnight-zk;
        compactRuntimeRsSrc  = "${inputs.compact}/runtime-rs";
      };
    };
}
