# Nix derivation that builds:
#   $out/bin/chdkpano-server           — the axum HTTP server (Rust → aarch64-linux)
#   $out/share/chdkpano/dist/          — the leptos wasm client (trunk build --release)
#
# Building Rust + wasm under Nix is the spiciest part of this whole setup.
# Two specific things will likely need iteration on first build:
#
#   1. `cargoLock` needs `outputHashes` for any git/path deps. The chdkpano
#      workspace has a path dep on `../../../chdkptp_rs` which Nix can't
#      reach unless you either:
#        a. Vendor chdkptp_rs into the source closure (cleanest)
#        b. Publish chdkptp to crates.io and bump to a registry dep
#        c. Use `fetchFromGitHub` for chdkptp_rs and pin a commit
#      The scaffold below assumes (a): we depend on a sibling
#      `chdkptp_rs/` directory at the same level as `chdkpano/`.
#
#   2. The wasm client uses leptos which requires nightly Rust. We use
#      rust-overlay to pin a known-good nightly. tailwindcss v4 also runs
#      under the hood — trunk's hooks invoke it.
{ lib
, stdenv
, rustPlatform
, rust-bin
, pkg-config
, udev
, trunk
, wasm-bindgen-cli
, binaryen
, nodejs
, callPackage
, makeWrapper
, repoRoot     # passed from flake.nix
}:

let
  # Pin a Rust nightly that works with leptos 0.8.
  # Bump this when leptos requires a newer one.
  rustToolchain = rust-bin.nightly."2026-04-01".default.override {
    extensions = [ "rust-src" ];
    targets = [ "wasm32-unknown-unknown" ];
  };
in

rustPlatform.buildRustPackage {
  pname = "chdkpano";
  version = "0.1.0";

  # Source = the chdkpano repo root. Includes server/, client/, Cargo.lock.
  # If chdkptp_rs is a sibling directory, also pull it into the closure
  # via a small wrapper derivation — see explainer.html for the pattern.
  src = repoRoot;

  cargoLock = {
    lockFile = "${repoRoot}/Cargo.lock";
    # Path deps don't have hashes. Git deps need one — uncomment and run
    # `nix build` to see the expected hash, then paste it in.
    # outputHashes = {
    #   "chdkptp-0.1.0" = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    # };
  };

  nativeBuildInputs = [
    rustToolchain
    pkg-config
    trunk
    wasm-bindgen-cli
    binaryen          # provides wasm-opt (trunk uses it in --release builds)
    nodejs            # tailwind v4 runs via node
    makeWrapper
  ];

  buildInputs = [ udev ];

  # Server compiles via cargo's default phase; we also need the wasm client.
  # `trunk build --offline` so it doesn't try to download anything during
  # the Nix build sandbox (which has no network).
  postBuild = ''
    pushd client
    HOME=$TMPDIR trunk build --release --offline
    popd
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin $out/share/chdkpano

    # Server binary (cargo's default install phase would do this, but we
    # disabled it by overriding installPhase to also handle the client).
    install -m 755 target/${stdenv.hostPlatform.config}/release/chdkpano-server $out/bin/

    # Wasm client bundle (index.html + hashed .js/.wasm/.css)
    cp -r client/dist $out/share/chdkpano/dist

    runHook postInstall
  '';

  # Don't try to run tests in the Nix sandbox — many require a USB camera.
  doCheck = false;

  meta = with lib; {
    description = "Web UI for Canon CHDK panorama rigs (chdkpano)";
    homepage = "https://github.com/joseph-x-li/chdkpano";
    license = licenses.mit;
    mainProgram = "chdkpano-server";
    platforms = platforms.linux;
  };
}
