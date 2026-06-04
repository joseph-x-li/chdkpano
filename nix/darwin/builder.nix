# nix-darwin configuration that turns your Mac into a Linux builder.
#
# This sets up `linux-builder`: a tiny NixOS VM (~256 MB RAM, ~20 GB disk)
# that runs on the Mac via the macOS Virtualization framework. When you run
# `nix build .#sdImage` on the Mac, the build is automatically offloaded
# into the VM (the Mac doesn't natively know how to compile aarch64-linux
# binaries; the VM does).
#
# Apply with:
#   darwin-rebuild switch --flake .#mac-builder
#
# After that, every `nix build` you run on the Mac that needs aarch64-linux
# will transparently use the builder. Image builds drop from "~30 min via
# QEMU emulation" to "~3–10 min via the VM".
#
# Tear down with:
#   sudo launchctl bootout system/org.nixos.linux-builder
#   sudo rm -rf /var/lib/nix-builder/
{ pkgs, ... }:

{
  nix = {
    # Use determinate-nix or nix-daemon; both work.
    package = pkgs.nixVersions.latest;

    # Trust users in @admin (you) to use the daemon's builders and the
    # extra trusted substituters that linux-builder needs.
    settings = {
      experimental-features = [ "nix-command" "flakes" ];
      trusted-users = [ "@admin" ];
      # Cache hits from the official binary cache (huge speedup)
      substituters = [
        "https://cache.nixos.org"
        "https://nix-community.cachix.org"
      ];
      trusted-public-keys = [
        "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
        "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
      ];
    };

    # Spawn the Linux builder VM
    linux-builder = {
      enable = true;
      # System the VM runs (must match what you're building for).
      systems = [ "aarch64-linux" "x86_64-linux" ];
      # Give the VM enough resources to build chdkpano comfortably.
      # The defaults (1 CPU, 3 GB) are tight when cargo builds in parallel.
      maxJobs = 4;
      config = {
        virtualisation = {
          cores = 4;
          # Memory in MB.
          darwin-builder.memorySize = 6 * 1024;
          darwin-builder.diskSize = 40 * 1024;
        };
      };
    };
  };

  # Things that are nice to have on the dev Mac
  environment.systemPackages = with pkgs; [
    nixos-rebuild   # for `nixos-rebuild switch --target-host` to the Pi
    deploy-rs       # nicer alternative for fleet deploys
    nix-output-monitor
    alejandra       # nix formatter
  ];

  # nix-darwin housekeeping. Recent nix-darwin manages nix-daemon
  # unconditionally when `nix.enable` is on — `services.nix-daemon.enable`
  # is now a hard assertion failure.
  programs.zsh.enable = true;

  system.stateVersion = 5;     # TODO: bump if nix-darwin's docs say so
}
