with import <nixpkgs> { };

rustPlatform.buildRustPackage rec {
   name = "nix-template";

   src = builtins.path { path = ./.; name = "nix-template"; };

   # this will need to be updated anytime Cargo.lock gets changed
   cargoSha256 = "1dnq79fafbv3w7kf9b1q36zsv69xrhnk42973vzf2sdjvds5qpx7";

   doCheck = true;
   checkInputs = [ clippy ];
   postCheck = ''
     cargo clippy
   '';

}
