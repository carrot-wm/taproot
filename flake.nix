{
  description = "taproot - a pure-Rust libc.so.6 (maintained fork of c-ward)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      # match the fork's rust-toolchain.toml (nightly-2026-06-11).
      toolchain =
        (fenix.packages.${system}.toolchainOf {
          channel = "nightly";
          date = "2026-06-11";
          sha256 = "sha256-L59udwZx36niu4S6j9huMpLBWL4m/Flt61nbXfXk/wk=";
        }).withComponents [ "cargo" "rustc" "rust-src" ];
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [ toolchain pkgs.binutils ];
        shellHook = ''echo "taproot shell: $(rustc --version)"'';
      };
    };
}
