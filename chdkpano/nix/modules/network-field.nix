# FIELD MODE — the Pi:
#   1. broadcasts its own WiFi SSID on the USB dongle (wlan1) via hostapd
#   2. hands out DHCP leases on that AP via dnsmasq
#   3. optionally still acts as a wpa_supplicant client on wlan0 (built-in)
#      so deploys/Tailscale work as long as a known SSID is in range
#   4. binds sshd to wlan0 only (so strangers on the AP can't hit ssh)
#
# This is the DEFAULT mode (imported directly in flake.nix, not as a
# specialisation), so a freshly-flashed/booted Pi comes up ready for
# guests in the field even with zero infrastructure.
{ config, lib, pkgs, ... }:

let
  cfg = config.networking.chdkpano;
  apSubnet = cfg.apSubnet;
  apGateway = "${apSubnet}.1";
in
{
  ###############################################################
  # Don't use NetworkManager (NM races with hostapd). wpa_supplicant
  # for the client iface only.
  ###############################################################
  networking.wireless = {
    enable = true;
    # Only manage the built-in radio. The USB dongle is owned by hostapd.
    interfaces = [ cfg.clientInterface ];
  };

  ###############################################################
  # hostapd — broadcast the field SSID on the USB dongle
  ###############################################################
  services.hostapd = {
    enable = true;
    radios.${cfg.apInterface} = {
      band = "2g";
      channel = 6;          # 1/6/11 are the non-overlapping 2.4GHz channels
      countryCode = "US";
      networks.${cfg.apInterface} = {
        ssid = cfg.apSsid;
        authentication = {
          mode = "wpa2-sha256";
          wpaPassword = cfg.apPassword;
        };
        # Cap concurrent clients — Pi Zero 2 W's radio doesn't enjoy more.
        # bssMaxStations = 8;
      };
    };
  };

  ###############################################################
  # Static IP on the AP interface
  ###############################################################
  networking.interfaces.${cfg.apInterface} = {
    ipv4.addresses = [{
      address = apGateway;
      prefixLength = 24;
    }];
  };

  ###############################################################
  # dnsmasq — DHCP + DNS on the AP. Acts as a captive-portal-ish
  # resolver: any hostname requests resolve to the Pi.
  ###############################################################
  services.dnsmasq = {
    enable = true;
    settings = {
      interface = cfg.apInterface;
      bind-interfaces = true;        # don't try to listen on wlan0/lo
      dhcp-range = "${apSubnet}.10,${apSubnet}.50,12h";
      dhcp-option = [
        # Tell clients the Pi is the gateway + DNS
        "option:router,${apGateway}"
        "option:dns-server,${apGateway}"
      ];

      # Resolve every hostname to the Pi. Phones probing
      # `connectivitycheck.gstatic.com` or `captive.apple.com` land on
      # us — chdkpano-server should serve a 302 to / for those paths if
      # you want the captive-portal sheet to auto-open the UI.
      address = "/#/${apGateway}";
    };
  };

  ###############################################################
  # Bind sshd to the client interface ONLY
  ###############################################################
  # Stops random AP guests from poking at ssh. If wlan0 has no IP (out
  # of range of known networks), sshd will simply not listen — pair
  # with Tailscale for the "field with no infrastructure" case.
  services.openssh.listenAddresses = [
    # Loopback (so Tailscale + local debug still work)
    { addr = "127.0.0.1"; port = 22; }
    # Tailscale interface (always present once tailscale is up)
    # tailscaled adds this automatically — no need to list it here.
  ];
  # We let Tailscale provide remote access. To allow sshd over the client
  # WiFi too, add an entry binding to the wlan0 address (annoying because
  # the address is DHCP-assigned). Easier: just use Tailscale for ssh.

  ###############################################################
  # Forwarding — disable. AP clients should NOT route through wlan0
  # to your home WiFi. The AP is intentionally isolated; clients can
  # only reach the Pi itself (chdkpano UI + DHCP/DNS).
  ###############################################################
  boot.kernel.sysctl."net.ipv4.ip_forward" = 0;
  boot.kernel.sysctl."net.ipv6.conf.all.forwarding" = 0;
}
