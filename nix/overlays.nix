{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, ... }:
    {
      # Nothing to declare at flake-module level for now. The symlink to
      # third_party/midnight-ledger is materialised by the devShells shellHook
      # using midnightDidRsLib.sources.midnight-ledger.
      _module.args.midnightLedgerSrc = midnightDidRsLib.sources.midnight-ledger;
    };
}
