with import <nixpkgs> {};

mkShell {
  buildInputs = [
    openssl
    pkg-config
  ];
}
