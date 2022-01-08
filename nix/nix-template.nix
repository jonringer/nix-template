{ lib, rustPlatform, nix-gitignore, clippy, makeWrapper, nix, openssl, pkg-config }:

rustPlatform.buildRustPackage rec {
   name = "nix-template";

   src = nix-gitignore.gitignoreSource [] ../.;

   cargoLock.lockFile = ../Cargo.lock;

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
