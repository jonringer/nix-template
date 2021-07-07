{ mkShell, cargo, clippy, openssl, pkg-config, rustfmt }:

mkShell {
  buildInputs = [
    cargo
    clippy
    openssl
    pkg-config
    rustfmt
  ];
}
