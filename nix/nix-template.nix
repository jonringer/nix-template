{ lib, rustPlatform, nix-gitignore, clippy, makeWrapper, nix, openssl, pkg-config }:

rustPlatform.buildRustPackage rec {
   name = "nix-template";

   src = nix-gitignore.gitignoreSource [] ../.;

   # this will need to be updated anytime Cargo.lock gets changed
   cargoSha256 = "sha256-N0ZA43PDYhJmT5U1SxtX56kIivxoaYcJ+Kaf5WusAyg=";

   nativeBuildInputs = [ pkg-config makeWrapper ];
   buildInputs = [ openssl ];

   doCheck = true;
   checkInputs = [ clippy ];
   postCheck = ''
     cargo clippy
   '';

   # needed for `nix-prefetch-url`
   postInstall = ''
     wrapProgram $out/bin/nix-template \
       --prefix PATH : ${lib.makeBinPath [ nix ]}
   '';
}
