# DESK MODE — the Pi:
#   1. joins your home WiFi as a client on the built-in radio (wlan0)
#   2. ALSO joins your home WiFi (or stays idle) on the USB dongle (wlan1)
#   3. exposes sshd on every interface (you're inside your trusted LAN now)
#   4. does NOT run hostapd/dnsmasq — no AP
#
# This module is imported as a *specialisation* (see flake.nix), so flipping
# to it is one command on the Pi:
#
#   sudo /run/booted-system/specialisation/desk/bin/switch-to-configuration switch
#
# And back to field:
#
#   sudo /run/current-system/bin/switch-to-configuration switch
{ config, lib, pkgs, ... }:

let
  cfg = config.networking.chdkpano;
in
{
  ###############################################################
  # Kill the AP-mode services
  ###############################################################
  services.hostapd.enable = lib.mkForce false;
  services.dnsmasq.enable = lib.mkForce false;

  ###############################################################
  # Both radios as wpa_supplicant clients
  ###############################################################
  networking.wireless = {
    enable = true;
    interfaces = [ cfg.clientInterface cfg.apInterface ];
  };

  ###############################################################
  # No static IP on the dongle — DHCP from your home AP
  ###############################################################
  networking.interfaces.${cfg.apInterface}.ipv4.addresses = lib.mkForce [ ];

  ###############################################################
  # sshd open on all interfaces — desk mode = trusted LAN
  ###############################################################
  services.openssh.listenAddresses = lib.mkForce [ ];   # = listen on 0.0.0.0
}
