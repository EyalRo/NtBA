{
  description = "NtBA — Not the Basketball Association: development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        # Single source of truth for the toolchain: rust-toolchain.toml
        # (the same file rustup users get automatically).
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          name = "ntba";
          packages = [
            rustToolchain
            pkgs.binaryen   # wasm-opt, used by scripts/build-web.sh when present
            pkgs.python3    # scripts/serve.sh
            pkgs.gnumake
          ];
          shellHook = ''
            echo "NtBA dev shell — try: make serve   (then open http://localhost:8080)"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
