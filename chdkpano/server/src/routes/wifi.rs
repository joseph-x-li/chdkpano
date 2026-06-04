//! `/api/wifi/*` — inspect the rig's two radios and (re)point the client
//! radio at a network.
//!
//! Routes:
//!   GET  /api/wifi          — AP details (the SSID the Pi broadcasts) plus
//!                             the live status of the client radio.
//!   POST /api/wifi/client   — { "ssid": "...", "psk": "..." } — add+select a
//!                             network on the client radio via `wpa_cli`.
//!
//! This shells out to `wpa_cli` and `iw`. On a dev box where neither exists
//! the GET still returns the static AP details (from env) and fills the
//! dynamic fields with a `note` instead of failing — so the page renders
//! everywhere, and only the apply step needs the real hardware.
//!
//! Interfaces + AP identity come from the environment, set by the NixOS
//! module from `networking.chdkpano`:
//!   CHDKPANO_AP_IFACE       (default wlan1)   — hostapd radio
//!   CHDKPANO_CLIENT_IFACE   (default wlan0)   — wpa_supplicant radio
//!   CHDKPANO_AP_SSID        (default chdkpano)
//!   CHDKPANO_AP_PASSWORD    (default "")
//!   CHDKPANO_AP_SUBNET      (e.g. 192.168.42) — Pi takes .1

use crate::error::{Error, Result};
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use utoipa::ToSchema;

// ─── DTOs ──────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct ApInfoDto {
    pub iface: String,
    /// The SSID the Pi broadcasts in field mode.
    pub ssid: String,
    /// The WPA2 password for that AP (this is a single-purpose rig; the
    /// password is not a secret from whoever's standing at the rig).
    pub password: String,
    /// Gateway IP the Pi holds on the AP subnet (e.g. 192.168.42.1).
    pub ip: Option<String>,
    /// 2.4 GHz channel hostapd is on, if `iw` could read it.
    pub channel: Option<u32>,
    /// How many stations are currently associated to the AP.
    pub connected_clients: Option<usize>,
    /// Set when live AP data couldn't be read (e.g. `iw` missing).
    pub note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ClientInfoDto {
    pub iface: String,
    /// SSID the client radio is currently joined to (None if not associated).
    pub ssid: Option<String>,
    /// wpa_supplicant state, e.g. "COMPLETED", "SCANNING", "DISCONNECTED".
    pub state: Option<String>,
    /// IPv4 address the client radio obtained, if any.
    pub ip: Option<String>,
    /// Link signal in dBm (closer to 0 is stronger), if `iw` could read it.
    pub signal_dbm: Option<i32>,
    /// Set when live client data couldn't be read (e.g. `wpa_cli` missing).
    pub note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct WifiStatusDto {
    pub ap: ApInfoDto,
    pub client: ClientInfoDto,
}

#[derive(Deserialize, ToSchema)]
pub struct SetClientBody {
    /// SSID to join (1–32 chars, no double-quote or control chars).
    pub ssid: String,
    /// WPA2 passphrase (8–63 chars). Omit or empty for an open network.
    pub psk: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SetClientResultDto {
    pub ok: bool,
    pub message: String,
    /// The wpa_supplicant network id that was created, if the add succeeded.
    pub network_id: Option<i32>,
    /// Client radio status read back right after applying.
    pub client: ClientInfoDto,
}

// ─── GET /api/wifi ─────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/wifi",
    tag = "wifi",
    responses((status = 200, body = WifiStatusDto)),
)]
pub async fn wifi_status() -> Json<WifiStatusDto> {
    let ap_iface = env_or("CHDKPANO_AP_IFACE", "wlan1");
    let client_iface = env_or("CHDKPANO_CLIENT_IFACE", "wlan0");
    Json(WifiStatusDto {
        ap: gather_ap(&ap_iface).await,
        client: gather_client(&client_iface).await,
    })
}

async fn gather_ap(iface: &str) -> ApInfoDto {
    let ssid = env_or("CHDKPANO_AP_SSID", "chdkpano");
    let password = env_or("CHDKPANO_AP_PASSWORD", "");
    let ip = std::env::var("CHDKPANO_AP_SUBNET")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|subnet| format!("{subnet}.1"));

    let mut channel = None;
    let mut connected_clients = None;
    let mut note = None;

    // `iw dev <iface> info` → "channel 6 (...)"
    match run("iw", &["dev", iface, "info"]).await {
        Ok(out) => {
            channel = out
                .split_whitespace()
                .skip_while(|w| *w != "channel")
                .nth(1)
                .and_then(|c| c.parse().ok());
        }
        Err(e) => note = Some(e),
    }

    // `iw dev <iface> station dump` → one "Station <mac>" block per client.
    if let Ok(out) = run("iw", &["dev", iface, "station", "dump"]).await {
        connected_clients = Some(out.lines().filter(|l| l.starts_with("Station ")).count());
    }

    ApInfoDto {
        iface: iface.to_string(),
        ssid,
        password,
        ip,
        channel,
        connected_clients,
        note,
    }
}

async fn gather_client(iface: &str) -> ClientInfoDto {
    let mut ssid = None;
    let mut state = None;
    let mut ip = None;
    let mut signal_dbm = None;
    let mut note = None;

    // `wpa_cli -i <iface> status` → key=value lines.
    match run("wpa_cli", &["-i", iface, "status"]).await {
        Ok(out) => {
            for line in out.lines() {
                let Some((k, v)) = line.split_once('=') else { continue };
                match k {
                    "ssid" => ssid = Some(v.to_string()),
                    "wpa_state" => state = Some(v.to_string()),
                    "ip_address" => ip = Some(v.to_string()),
                    _ => {}
                }
            }
        }
        Err(e) => note = Some(e),
    }

    // `iw dev <iface> link` → "signal: -52 dBm"
    if let Ok(out) = run("iw", &["dev", iface, "link"]).await {
        signal_dbm = out
            .lines()
            .find_map(|l| l.trim().strip_prefix("signal:"))
            .and_then(|s| s.split_whitespace().next())
            .and_then(|n| n.parse().ok());
    }

    ClientInfoDto {
        iface: iface.to_string(),
        ssid,
        state,
        ip,
        signal_dbm,
        note,
    }
}

// ─── POST /api/wifi/client ─────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/wifi/client",
    tag = "wifi",
    request_body = SetClientBody,
    responses(
        (status = 200, body = SetClientResultDto),
        (status = 500, description = "Invalid input or wpa_cli failure"),
    ),
)]
pub async fn set_client(Json(body): Json<SetClientBody>) -> Result<Json<SetClientResultDto>> {
    let iface = env_or("CHDKPANO_CLIENT_IFACE", "wlan0");
    let ssid = body.ssid.trim().to_string();
    let psk = body.psk.unwrap_or_default();
    let psk = psk.trim();

    validate_ssid(&ssid)?;
    let open = psk.is_empty();
    if !open {
        validate_psk(psk)?;
    }

    // Create a fresh network, point it at the requested SSID, and select it
    // (which disables the others until the next wpa_supplicant reconfigure —
    // exactly the "switch to this network now" the user asked for). Runtime
    // only: we deliberately don't `save_config`, so a NixOS rebuild restores
    // the declared networks.
    let id_out = run("wpa_cli", &["-i", &iface, "add_network"]).await?;
    let id = id_out
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .parse::<i32>()
        .map_err(|_| Error::new(format!("wpa_cli add_network returned unexpected: {id_out:?}")))?;
    let id_s = id.to_string();

    expect_ok(
        run(
            "wpa_cli",
            &["-i", &iface, "set_network", &id_s, "ssid", &quote(&ssid)],
        )
        .await?,
        "set ssid",
    )?;

    if open {
        expect_ok(
            run(
                "wpa_cli",
                &["-i", &iface, "set_network", &id_s, "key_mgmt", "NONE"],
            )
            .await?,
            "set open network",
        )?;
    } else {
        expect_ok(
            run(
                "wpa_cli",
                &["-i", &iface, "set_network", &id_s, "psk", &quote(psk)],
            )
            .await?,
            "set psk",
        )?;
    }

    expect_ok(
        run("wpa_cli", &["-i", &iface, "enable_network", &id_s]).await?,
        "enable network",
    )?;
    expect_ok(
        run("wpa_cli", &["-i", &iface, "select_network", &id_s]).await?,
        "select network",
    )?;

    // Give association a brief moment, then read the radio back so the UI can
    // show progress immediately (it may still be SCANNING/ASSOCIATING).
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    let client = gather_client(&iface).await;

    Ok(Json(SetClientResultDto {
        ok: true,
        message: format!("Pointed {iface} at \"{ssid}\". It may take a few seconds to associate."),
        network_id: Some(id),
        client,
    }))
}

// ─── helpers ───────────────────────────────────────────────────────────

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Run a command to completion, returning trimmed stdout or a readable error.
/// Never panics; a missing binary becomes `Err`, not a crash.
async fn run(bin: &str, args: &[&str]) -> std::result::Result<String, String> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("{bin} unavailable: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!("{bin} {} failed: {detail}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// wpa_cli set/enable/select commands answer "OK" on success and "FAIL"
/// otherwise (with a zero exit code either way), so we check the text.
fn expect_ok(out: String, step: &str) -> Result<()> {
    if out.trim() == "OK" {
        Ok(())
    } else {
        Err(Error::new(format!("wpa_cli {step} did not return OK: {out:?}")))
    }
}

/// wpa_supplicant wants string values wrapped in literal double quotes. We
/// reject quotes in the input (see validation) so this can't break out.
fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

fn validate_ssid(ssid: &str) -> Result<()> {
    if ssid.is_empty() {
        return Err(Error::new("SSID must not be empty"));
    }
    if ssid.len() > 32 {
        return Err(Error::new("SSID must be at most 32 bytes"));
    }
    if ssid.contains('"') || ssid.chars().any(|c| c.is_control()) {
        return Err(Error::new("SSID must not contain quotes or control characters"));
    }
    Ok(())
}

fn validate_psk(psk: &str) -> Result<()> {
    if !(8..=63).contains(&psk.len()) {
        return Err(Error::new("WPA2 passphrase must be 8–63 characters"));
    }
    if psk.contains('"') || psk.chars().any(|c| c.is_control()) {
        return Err(Error::new("passphrase must not contain quotes or control characters"));
    }
    Ok(())
}
