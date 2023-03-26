{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.11";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = inputs@{ self, nixpkgs, utils }:
    utils.lib.eachDefaultSystem
    (system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
    devShells.default = pkgs.mkShell {
      nativeBuildInputs = with pkgs; [ cmake postgresql_14 openssl  pkg-config ];
    };
  });
}
