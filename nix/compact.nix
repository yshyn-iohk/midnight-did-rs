{ inputs, ... }:
{
  perSystem =
    { system, ... }:
    {
      _module.args.compactPkg = inputs.compact.packages.${system}.compactc;
    };
}
