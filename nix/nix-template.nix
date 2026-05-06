{
  lib,
  stdenv,
  rustPlatform,
  fetchFromGitHub,
  installShellFiles,
  makeWrapper,
  nix,
  nix-gitignore,
  openssl,
  pkg-config,
}:

rustPlatform.buildRustPackage rec {
  name = "nix-template";

  src = nix-gitignore.gitignoreSource [ ] ../.;

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    installShellFiles
    makeWrapper
    pkg-config
  ];

  buildInputs = [ openssl ];

  # needed for nix-prefetch-url
  postInstall =
    ''
      wrapProgram $out/bin/nix-template \
        --prefix PATH : ${lib.makeBinPath [ nix ]}

    ''
    + lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
      installShellCompletion --cmd nix-template \
        --bash <($out/bin/nix-template completions bash) \
        --fish <($out/bin/nix-template completions fish) \
        --zsh <($out/bin/nix-template completions zsh)
    '';

}
