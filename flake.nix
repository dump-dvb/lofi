{
  inputs = {
    nixpkgs.url = github:NixOS/nixpkgs/nixos-unstable;
    utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem
    (system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
      package = pkgs.callPackage ./derivation.nix { naersk = naersk.lib.${system}; };
    in
    rec {
    checks = packages;
    packages = {
      lofi = package;
      default = package;
    };
    devShells.default = pkgs.mkShell {
      nativeBuildInputs = (with packages.lofi; nativeBuildInputs ++ buildInputs);
    };
    apps = {
      lofi = utils.lib.mkApp { drv = packages.lofi; };
      default = apps.lofi;
    };
  }) // {
    overlays.default = final: prev: {
      inherit (self.packages.${prev.system})
      lofi;
    };
  };
}
