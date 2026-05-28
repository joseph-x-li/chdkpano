# Per-host config for "chdk-pi" — the Raspberry Pi Zero 2 W in your rig.
# Reusable behaviour lives in ../modules/*. This file should only contain
# values that are specific to THIS PI (hostname, your ssh keys, the WiFi
# networks you actually connect to, etc.).
{ config, lib, pkgs, ... }:

{
  ###############################################################
  # Hostname and identity
  ###############################################################

  networking.hostName = "chdk-pi";
  time.timeZone = "America/New_York";        # TODO: your zone

  ###############################################################
  # Users
  ###############################################################

  # Pi user — used for SSH and deploys. Add additional pubkeys for
  # collaborators by appending to the list.
  users.users.pi = {
    isNormalUser = true;
    description = "Pi operator";
    extraGroups = [ "wheel" "plugdev" "dialout" "networkmanager" ];
    openssh.authorizedKeys.keys = [
      # TODO: paste your `~/.ssh/id_ed25519.pub` here
      "ssh-ed25519 AAAA…REPLACE_ME mac@example"
    ];
  };
  # If `users.mutableUsers = false`, the system rejects any `passwd`-style
  # changes — declarative password (or lack of one) is authoritative. Pair
  # with `hashedPassword = ...` if you want a console fallback.
  users.mutableUsers = false;

  # Passwordless sudo for the wheel group — the Pi is a personal device,
  # there's no admin separation to enforce.
  security.sudo.wheelNeedsPassword = false;

  ###############################################################
  # WiFi credentials (used in DESK / "client" mode)
  ###############################################################
  # Pre-load every SSID the Pi might roam between. Higher `priority` =
  # tried first. Use `wpa_passphrase "SSID" "password"` on the Mac to
  # generate the hashed PSK if you'd rather not store cleartext.
  networking.wireless.networks = {
    "HomeWiFi" = {
      psk = "REPLACE_ME_HOME_WIFI_PASSWORD";   # TODO
      priority = 100;
    };
    # Phone hotspot — handy for dev iteration away from home WiFi
    "iPhone" = {
      psk = "REPLACE_ME_PHONE_HOTSPOT_PASSWORD"; # TODO
      priority = 50;
    };
  };

  ###############################################################
  # WiFi SSID + password the Pi BROADCASTS (used in FIELD / "AP" mode)
  ###############################################################
  # Pulled into hostapd config in modules/network-field.nix.
  networking.chdkpano = {
    apSsid = "chdkpano";
    apPassword = "panorama-rig";   # TODO: pick something memorable
  };

  ###############################################################
  # Tailscale
  ###############################################################
  # Provision an auth key from https://login.tailscale.com/admin/settings/keys
  # (reusable + pre-authorised + ephemeral for fleet hygiene). Either:
  #
  #   1. write it to /etc/tailscale-auth.key on the Pi (one-time scp) — the
  #      systemd unit reads from there, secret stays out of the Nix store.
  #
  #   2. inline below for first-rig convenience (cleartext in /nix/store):
  #
  # services.tailscale.authKeyFile = pkgs.writeText "ts-key" "tskey-auth-XXXXX";
  #
  # Both modes are toggled in modules/network-base.nix.

  ###############################################################
  # SD-image specifics
  ###############################################################
  # NixOS' aarch64 SD image module ships with a working u-boot config for
  # most Pis. Allow firmware (proprietary blobs needed by the Pi VC4 GPU
  # and the BCM wifi/bt chip).
  hardware.enableRedistributableFirmware = true;

  # Don't run sshd on the WiFi-broadcasting interface for the AP-mode case
  # — the field network is potentially shared with strangers. See
  # modules/network-field.nix where we bind sshd to the client interface.

  system.stateVersion = "24.11";   # TODO: match the nixpkgs you build with
}
