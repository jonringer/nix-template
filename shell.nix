with import <nixpkgs> {};

mkShell {
  buildInputs = [
    cargo
    openssl
    pkg-config
    rustfmt
  ];
}
