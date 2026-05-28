# Common networking: things that are true regardless of whether the Pi is
# in "field" mode (AP) or "desk" mode (WiFi client).
#
# Includes the `networking.chdkpano` option set so the field/desk modules
# and hosts/chdk-pi.nix can share the same vocabulary (apSsid, apPassword,
# apInterface, clientInterface).
{ config, lib, pkgs, ... }:

{
  options.networking.chdkpano = {
    apInterface = lib.mkOption {
      type = lib.types.str;
      default = "wlan1";
      description = ''
        Interface used by hostapd to broadcast the field-mode SSID.
        Defaults to wlan1 = USB WiFi dongle. Use wlan0 if you only have
        the built-in radio and don't need simultaneous client mode.
      '';
    };
    clientInterface = lib.mkOption {
      type = lib.types.str;
      default = "wlan0";
      description = ''
        Interface used by wpa_supplicant to join existing networks.
        Defaults to wlan0 = built-in Pi WiFi.
      '';
    };
    apSsid = lib.mkOption {
      type = lib.types.str;
      default = "chdkpano";
    };
    apPassword = lib.mkOption {
      type = lib.types.str;
      example = "panorama-rig";
      description = ''
        WPA2 password for the field-mode AP. Min 8 characters.
        Stored in cleartext in /nix/store — fine for a single-purpose rig,
        upgrade to sops-nix/agenix if this matters.
      '';
    };
    apSubnet = lib.mkOption {
      type = lib.types.str;
      default = "192.168.42";
      description = ''
        First three octets of the AP subnet. The Pi takes .1, dnsmasq
        hands out .10–.50. Pick something unlikely to collide with home
        WiFi (192.168.1.x is overloaded; 192.168.42 is rare).
      '';
    };
  };

  config = {
    ###############################################################
    # SSH
    ###############################################################
    services.openssh = {
      enable = true;
      settings = {
        PasswordAuthentication = false;
        PermitRootLogin = "no";
        KbdInteractiveAuthentication = false;
      };
      # In field mode we bind to the client iface only (configured in
      # network-field.nix), so sshd isn't exposed to AP-joined strangers.
    };

    ###############################################################
    # mDNS: ssh pi@chdk-pi.local, browse http://chdk-pi.local:3030
    ###############################################################
    services.avahi = {
      enable = true;
      nssmdns4 = true;
      publish = {
        enable = true;
        addresses = true;
        workstation = true;
      };
    };

    ###############################################################
    # Tailscale (optional but recommended)
    ###############################################################
    # Two modes:
    #   - permanent: provide `authKeyFile` so the Pi auto-joins your tailnet
    #     after first boot
    #   - manual: omit and run `sudo tailscale up` once over ssh
    #
    # Once joined, `ssh pi@chdk-pi` works from anywhere with internet.
    services.tailscale = {
      enable = true;
      # authKeyFile = "/etc/tailscale-auth.key";   # TODO: see hosts/chdk-pi.nix
      useRoutingFeatures = "client";
    };

    ###############################################################
    # NetworkManager — convenient ad-hoc `nmcli` from the shell
    ###############################################################
    # Disabled because we drive wpa_supplicant/hostapd directly. NM and
    # wpa_supplicant fight each other; pick one. NixOS' `networking.wireless`
    # = wpa_supplicant path is what the rest of these modules assume.
    networking.networkmanager.enable = false;

    ###############################################################
    # Firewall — accept loopback + tailscale + ssh + chdkpano
    # (specific WiFi-mode-dependent rules live in field/desk modules)
    ###############################################################
    networking.firewall = {
      enable = true;
      allowedTCPPorts = [ 22 ];
      trustedInterfaces = [ "tailscale0" "lo" ];
    };

    ###############################################################
    # USB WiFi dongle quirks
    ###############################################################
    # Most Realtek RTL8188/8821-based dongles need their out-of-tree driver
    # OR the new in-kernel `rtw88` driver (kernel >= 5.16). nixos-unstable
    # ships modern kernels, so this usually Just Works. If your dongle
    # doesn't enumerate, run `dmesg | grep -i usb` and uncomment the
    # right entry below.
    boot.extraModulePackages = with config.boot.kernelPackages; [
      # rtl8821cu                      # Realtek 8821CU (AC600, 8821cu)
      # rtl88x2bu                      # Realtek 8822BU/CU/DU
    ];
  };
}
