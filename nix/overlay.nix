final: prev: {
  nix-template = prev.callPackage ./nix-template.nix { };

  devShell = prev.callPackage ./shell.nix { };
}
