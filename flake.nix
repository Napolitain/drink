{
  description = "Drink water-break reminders for KDE Plasma Wayland";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      # A flake may be pinned to any source revision between weekly releases.
      version = builtins.concatStringsSep "." (
        builtins.match "([0-9]{4})([0-9]{2})([0-9]{2}).*" self.lastModifiedDate
      );
      drink = pkgs.callPackage ./nix/package.nix {
        src = self;
        inherit version;
      };
    in
    {
      packages.${system} = {
        default = drink;
        drink-kde = drink;
      };
      checks.${system}.drink-kde = drink;
    };
}
