{
  description = "Nix-template flake";

  inputs = {
    utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs, utils }:
    let
      # put devShell and any other required packages into local overlay
      localOverlay = import ./nix/overlay.nix;

      pkgsForSystem = system: import nixpkgs {
        # if you have additional overlays, you may add them here
        overlays = [
          localOverlay # this should expose devShell
        ];
        inherit system;
      };
    # https://github.com/numtide/flake-utils#usage for more examples
    in utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ] (system: rec {
      legacyPackages = pkgsForSystem system;
      packages = utils.lib.flattenTree {
        inherit (legacyPackages) devShell nix-template;
        default = legacyPackages.nix-template;
      };
      apps.nix-template = utils.lib.mkApp { drv = packages.nix-template; };  # use as `nix run <mypkg>`
      hydraJobs = { inherit (legacyPackages) nix-template; };
      checks = { inherit (legacyPackages) nix-template; };              # items to be ran as part of `nix flake check`
      devShells.default = with legacyPackages; mkShell {
        nativeBuildInputs = [ rustc cargo clippy pkg-config ];
        buildInputs = [ openssl ];
      };
  }) // {
    # non-system suffixed items should go here
    overlays.default = localOverlay;
    nixosModule = { config }: { options = {}; config = {};}; # export single module
    nixosModules = {}; # attr set or list
    nixosConfigurations.hostname = { config, pkgs }: {};
  };
}
