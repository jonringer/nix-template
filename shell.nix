with import <nixpkgs> {};

mkShell {
  buildInputs = [
    cargo
    clippy
    openssl
    pkg-config
    rustfmt
  ];
}
