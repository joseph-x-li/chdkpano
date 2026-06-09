# Nix derivation that builds:
#   $out/bin/chdkpano-server           — the axum HTTP server (Rust → aarch64-linux)
#   $out/share/chdkpano/dist/          — the leptos wasm client (trunk build --release)
#
# The chdkpano workspace has a path dep on `../../../chdkptp_rs` (a sibling
# git repo, not inside chdkpano). Nix's hermetic build sandbox can't see
# anything outside the declared source closure, so we explicitly pull
# chdkptp_rs in as `chdkptpSrc` (a flake input) and stitch the two source
# trees together inside a tiny `runCommand` derivation that mirrors the
# original on-disk layout exactly. That way the path `../../../chdkptp_rs`
# inside server/Cargo.toml resolves correctly inside the sandbox.
#
# The deep `outer/chdkpano/chdkpano/` nesting is artificial — it exists only
# to make `../../../chdkptp_rs` from the workspace's server/ subdirectory
# land on the right place. If we ever bring chdkptp_rs into the chdkpano
# monorepo as a sibling, this layout dance goes away.
#
# Still likely to need iteration on first build: trunk's `--offline` mode
# needs every shelled-out binary (wasm-bindgen, wasm-opt, npx tailwindcss)
# present in `nativeBuildInputs`.
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
, tailwindcss_4
, runCommand
, fetchurl
, makeWrapper
, repoRoot      # the chdkpano Rust workspace (Cargo.toml lives here)
, chdkptpSrc    # the sibling chdkptp_rs source tree (flake input)
}:

let
  # Pin a Rust nightly that works with leptos 0.8.
  # Bump this when leptos requires a newer one.
  rustToolchain = rust-bin.nightly."2026-04-01".default.override {
    extensions = [ "rust-src" ];
    targets = [ "wasm32-unknown-unknown" ];
  };

  # `utoipa-swagger-ui`'s build script downloads the Swagger UI bundle from
  # GitHub at build time — which the hermetic Nix sandbox (no network) blocks,
  # so the build panics. Vendor the exact zip it wants via a fixed-output
  # fetch and point the crate at it with a `file:` URL (SWAGGER_UI_DOWNLOAD_URL),
  # which makes the build script skip the download. Bump the version + hash
  # together if utoipa-swagger-ui is upgraded (see its build log for the URL).
  swaggerUi = fetchurl {
    url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.12.zip";
    hash = "sha256-HK4z/JI+1yq8BTBJveYXv9bpN/sXru7bn/8g5mf2B/I=";
  };

  # Reconstruct the on-disk layout that server/Cargo.toml's path dep expects:
  #
  #   mergedSrc/
  #   └── outer/                ← fake parent that makes ../../../resolve right
  #       ├── chdkpano/         ← was the chdkpano repo on disk
  #       │   └── chdkpano/     ← the Rust workspace
  #       │       ├── Cargo.toml
  #       │       ├── server/   ← server/Cargo.toml says path = "../../../chdkptp_rs"
  #       │       └── client/
  #       └── chdkptp_rs/       ← reached via 3 levels up from server/
  #
  # `sourceRoot` below tells buildRustPackage to cd into the workspace dir.
  mergedSrc = runCommand "chdkpano-with-chdkptp" { } ''
    mkdir -p $out/outer/chdkpano
    cp -r ${repoRoot}/. $out/outer/chdkpano/chdkpano
    cp -r ${chdkptpSrc}/. $out/outer/chdkptp_rs
    # Strip any leftover write-protection so cargo can update timestamps etc.
    chmod -R u+w $out
  '';
in

rustPlatform.buildRustPackage {
  pname = "chdkpano";
  version = "0.1.0";

  src = mergedSrc;
  # cd into the actual workspace after the source is unpacked.
  sourceRoot = "chdkpano-with-chdkptp/outer/chdkpano/chdkpano";

  cargoLock = {
    lockFile = "${repoRoot}/Cargo.lock";
    # Path deps don't need outputHashes — they're rebuilt from source every
    # time. Only git deps (`{ git = "..."; }`) need a hash here.
  };

  nativeBuildInputs = [
    rustToolchain
    pkg-config
    trunk
    wasm-bindgen-cli
    binaryen          # provides wasm-opt (trunk uses it in --release builds)
    nodejs            # general node runtime (kept for any node-based tooling)
    tailwindcss_4     # standalone Tailwind v4 CLI (v4.3.0); trunk finds it on
                      # PATH the same way it finds wasm-bindgen/wasm-opt. Without
                      # it, `trunk build --offline` fails: "couldn't find
                      # application tailwindcss ... unable to download offline".
    makeWrapper
  ];

  buildInputs = [ udev ];

  # Hand the vendored Swagger UI zip to utoipa-swagger-ui's build script so it
  # doesn't try to hit the network. The `file:` URL makes it copy locally.
  SWAGGER_UI_DOWNLOAD_URL = "file://${swaggerUi}";

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
