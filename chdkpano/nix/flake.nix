{
  description = "chdkpano — Canon CHDK panorama rig on Raspberry Pi Zero 2 W";

  inputs = {
    # nixos-unstable tracks newer kernels (needed for fresh hostapd/dwc2/etc.
    # on the Pi). Pin a release if you'd rather chase stability over recency.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    nix-darwin = {
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # rust-overlay so we can build with a specific Rust toolchain (the wasm
    # client uses nightly features via leptos).
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, nix-darwin, rust-overlay, ... }:
    let
      # Repo root is one level up from this flake (this file lives at nix/flake.nix).
      repoRoot = ../.;

      # The Pi's NixOS config. Apply specialisations after build to flip
      # between "field" (AP) and "desk" (client) modes.
      mkPi = system: nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          # NixOS SD-card image generator for aarch64 (Pi Zero 2 / Pi 3 / Pi 4 / Pi 5)
          "${nixpkgs}/nixos/modules/installer/sd-card/sd-image-aarch64.nix"

          # rust-overlay so we can pin Rust if we ever build chdkpano in Nix
          ({ pkgs, ... }: {
            nixpkgs.overlays = [ rust-overlay.overlays.default ];
          })

          # The reusable modules
          ./modules/chdkpano.nix
          ./modules/network-base.nix

          # This host's specific config (hostname, keys, WiFi creds, etc.)
          ./hosts/chdk-pi.nix

          # Bake in chdkpano + field/desk specialisations.
          ({ config, lib, pkgs, ... }: {
            services.chdkpano = {
              enable = true;
              package = pkgs.callPackage ./pkgs/chdkpano.nix {
                repoRoot = repoRoot;
              };
            };

            # Two atomically-switchable network configurations.
            # Default boot = field mode (AP). Activate the other with:
            #   sudo /run/booted-system/specialisation/desk/bin/switch-to-configuration switch
            specialisation.desk.configuration = {
              imports = [ ./modules/network-desk.nix ];
            };

            # Make field mode the default by importing it at the top level
            # (vs as a specialisation). The "desk" specialisation can be
            # switched into without a reboot; switching back is symmetric.
            imports = [ ./modules/network-field.nix ];
          })
        ];
      };
    in
    {
      nixosConfigurations.chdk-pi = mkPi "aarch64-linux";

      # `nix build .#sdImage` produces result/sd-image/*.img.zst
      packages.aarch64-linux.sdImage =
        self.nixosConfigurations.chdk-pi.config.system.build.sdImage;

      # `nix build .#chdkpano` builds just the server package (useful for
      # quick iteration without rebuilding the whole image).
      packages.aarch64-linux.chdkpano =
        let pkgs = import nixpkgs {
          system = "aarch64-linux";
          overlays = [ rust-overlay.overlays.default ];
        };
        in pkgs.callPackage ./pkgs/chdkpano.nix { repoRoot = repoRoot; };

      # nix-darwin config for the Mac that builds aarch64-linux images.
      # See nix/darwin/builder.nix for what this enables. After applying:
      #   darwin-rebuild switch --flake .#mac-builder
      darwinConfigurations.mac-builder = nix-darwin.lib.darwinSystem {
        # Change to "x86_64-darwin" on Intel Macs.
        system = "aarch64-darwin";
        modules = [ ./darwin/builder.nix ];
      };

      # Convenience devShell for local Nix work
      devShells.aarch64-darwin.default =
        let pkgs = import nixpkgs { system = "aarch64-darwin"; };
        in pkgs.mkShell {
          buildInputs = with pkgs; [ nixos-rebuild deploy-rs alejandra ];
        };
    };
}
