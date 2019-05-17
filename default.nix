{ nixpkgs ? <nixpkgs> }:
let
  pkgs = import nixpkgs {};
in
pkgs.callPackage (
{ stdenv, cargo, openssl, pkgconfig }:
stdenv.mkDerivation {
  name = "github-data-fetch";
  src = ./.;

  nativeBuildInputs = [ pkgconfig cargo ];
  buildInputs = [ openssl ];

  shellHook = ''
    echo Use cargo build to compile
    echo To run use cargo run -- -O repoowner -r reponame -t githubtoken -o outpath
  '';
}
) {}
