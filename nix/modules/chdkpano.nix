# NixOS module that wires up the chdkpano server: a system user, a systemd
# unit, the udev rule that lets that user open Canon USB devices via nusb,
# and a firewall hole.
#
# Doesn't build the package itself — that's pkgs/chdkpano.nix. This module
# just consumes a package via the `package` option.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.chdkpano;
in
{
  options.services.chdkpano = {
    enable = lib.mkEnableOption "chdkpano (Canon CHDK panorama web UI)";

    package = lib.mkOption {
      type = lib.types.package;
      description = ''
        The chdkpano package. Must provide `$out/bin/chdkpano-server` and
        `$out/share/chdkpano/dist/` (the wasm client bundle).
      '';
    };

    addr = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address chdkpano-server binds to.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 3030;
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "chdkpano";
      description = "System user the server runs as.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Open the server port in the firewall.";
    };

    canonVendorId = lib.mkOption {
      type = lib.types.str;
      default = "04a9";
      description = "USB vendor ID for the udev rule. 04a9 = Canon.";
    };
  };

  config = lib.mkIf cfg.enable {
    ###############################################################
    # User to run the daemon as
    ###############################################################

    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.user;
      # plugdev: nusb / udev convention for USB device access
      # dialout: in case we ever talk to a /dev/tty* CHDK USB-serial bridge
      extraGroups = [ "plugdev" "dialout" ];
    };
    users.groups.${cfg.user} = { };

    ###############################################################
    # udev rule: let the chdkpano user open Canon USB devices
    ###############################################################
    # nusb on Linux opens /dev/bus/usb/* directly. Default permissions are
    # 0664 root:root, so we need either root or a rule. MODE="0666" is the
    # pragmatic choice for a single-purpose device; tighten with TAG+="uaccess"
    # or GROUP="plugdev" if you care about multi-user separation.
    services.udev.extraRules = ''
      # chdkpano: let local users open Canon PTP cameras via nusb
      SUBSYSTEM=="usb", ATTR{idVendor}=="${cfg.canonVendorId}", MODE="0666"
    '';

    ###############################################################
    # systemd unit
    ###############################################################

    systemd.services.chdkpano = {
      description = "chdkpano server (Canon CHDK web UI)";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      environment = {
        CHDKPANO_STATIC_DIR = "${cfg.package}/share/chdkpano/dist";
        CHDKPANO_ADDR = "${cfg.addr}:${toString cfg.port}";
        RUST_LOG = "chdkpano_server=info,tower_http=info";
      };

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.user;
        ExecStart = "${cfg.package}/bin/chdkpano-server";
        Restart = "on-failure";
        RestartSec = 2;

        # Light hardening. If anything trips on USB access, the most
        # likely culprits are ProtectSystem (read-only /usr) and
        # DeviceAllow (no /dev/bus/usb access). Relax those first.
        ProtectHome = true;
        ProtectSystem = "strict";
        ReadWritePaths = [ ];
        PrivateTmp = true;
        NoNewPrivileges = true;
        # nusb needs /dev/bus/usb — grant it explicitly.
        DeviceAllow = [ "char-usb_device rw" "char-usbmon r" ];
        SupplementaryGroups = [ "plugdev" ];
      };
    };

    ###############################################################
    # Firewall
    ###############################################################

    networking.firewall = lib.mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };
  };
}
