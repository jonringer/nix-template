{ rustPlatform, nix-gitignore, clippy }:

rustPlatform.buildRustPackage rec {
   name = "nix-template";

   src = nix-gitignore.gitignoreSource [] ../.;

   # this will need to be updated anytime Cargo.lock gets changed
   cargoSha256 = "sha256-/DT8avWBj1zx4SbsAOHSZlJlUql7VEeCLUlYvXon5Gg=";

   doCheck = true;
   checkInputs = [ clippy ];
   postCheck = ''
     cargo clippy
   '';

}
