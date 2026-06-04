//! chdkpano-client: Leptos 0.8 + rust-ui components. 100% Rust.

mod ui_button;
mod ui_collapsible;
mod ui_dialog;

use leptos::prelude::*;
use leptos::ev;
use leptos::context::Provider;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_params;
use leptos_router::params::Params;
use leptos_router::path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ui_button::{Button, ButtonVariant};
use ui_collapsible::{Collapsible, CollapsibleContent, CollapsibleTrigger};
use ui_dialog::{
    Dialog, DialogBody, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader,
    DialogTitle, DialogTrigger,
};

fn event_target_value(ev: &ev::Event) -> String {
    use wasm_bindgen::JsCast;
    let target = ev.target().expect("event target");
    let input: web_sys::HtmlTextAreaElement = target
        .dyn_into()
        .expect("input/textarea");
    input.value()
}

/// Like `event_target_value` but for `<input>` elements (the WiFi form).
fn input_value(ev: &ev::Event) -> String {
    use wasm_bindgen::JsCast;
    let target = ev.target().expect("event target");
    let input: web_sys::HtmlInputElement = target.unchecked_into();
    input.value()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CameraDto {
    serial: String,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    manufacturer: Option<String>,
    product: Option<String>,
}

/// The four cameras of the panorama rig, indexed so that camera N lives at
/// `RIG_SERIALS[N - 1]` (the rig labels cameras 1–4). These are fixed
/// hardware — edit this list to match the rig's actual USB serials. They
/// must be the *full* serial string: `/api/viewport/<serial>` matches
/// exactly (see registry.rs `get_or_open`).
const RIG_SERIALS: [&str; 4] = [
    "DUMMY_SERIAL_CAM1", // placeholder — not connected today
    "FA934BBFD3514EF19CA0B81E72A213F7", // camera 2
    "D8359439FEB74E79899654E98FD41CA1", // camera 3
    "524EE2E7D9E34C6194BB238558A9EF91", // camera 4
];

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <header class="bg-card border-b border-border px-6 py-4 flex items-baseline gap-6">
                <h1 class="text-lg font-semibold tracking-tight flex items-center gap-2">
                    <img src="/favicon.png" alt="📸" class="w-5 h-5"/>
                    "chdkpano"
                </h1>
                <nav class="flex gap-4">
                    <A href="/" attr:class="text-sm text-muted-foreground hover:text-foreground">"Cameras"</A>
                    <A href="/pano" attr:class="text-sm text-muted-foreground hover:text-foreground">"Rig"</A>
                    <A href="/wifi" attr:class="text-sm text-muted-foreground hover:text-foreground">"WiFi"</A>
                // In-app route that embeds the backend Swagger UI in an iframe,
                // keeping this nav bar (vs. /swagger-ui/ which replaces the page).
                <A href="/api" attr:class="text-sm text-muted-foreground hover:text-foreground">"API"</A>
                </nav>
            </header>
            <main class="max-w-6xl mx-auto px-6 py-6">
                <Routes fallback=|| "Not found">
                    <Route path=path!("/") view=CameraListPage/>
                    <Route path=path!("/pano") view=PanoPage/>
                    <Route path=path!("/wifi") view=WifiPage/>
                    <Route path=path!("/api") view=ApiDocsPage/>
                    <Route path=path!("/camera/:serial") view=CameraDetailPage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn CameraListPage() -> impl IntoView {
    let cameras = LocalResource::new(fetch_cameras);

    view! {
        <div class="flex items-baseline justify-between mb-1 gap-3">
            <h2 class="text-base font-semibold">"Connected cameras"</h2>
            <Button
                variant=ButtonVariant::Outline
                size=ui_button::ButtonSize::Sm
                on:click=move |_| { cameras.refetch(); }
            >
                "Refresh"
            </Button>
        </div>
        <p class="text-sm text-muted-foreground mb-4">
            "Lists all Canon devices currently enumerated over USB."
        </p>

        <Suspense fallback=|| view! { <p class="text-sm text-muted-foreground">"Loading…"</p> }>
            {move || Suspend::new(async move {
                match cameras.await {
                    Err(e) => view! { <p class="text-sm text-destructive">{format!("Error: {e}")}</p> }.into_any(),
                    Ok(cams) if cams.is_empty() => view! {
                        <p class="text-sm text-destructive">
                            "No Canon devices found. Wake the camera (half-press shutter / power on) and reload."
                        </p>
                    }.into_any(),
                    Ok(cams) => view! {
                        <div class="border border-border rounded-md bg-card overflow-hidden">
                            <table class="w-full text-sm">
                                <thead class="bg-muted text-xs uppercase tracking-wide text-muted-foreground">
                                    <tr>
                                        <th class="text-left font-medium px-4 py-2">"Model"</th>
                                        <th class="text-left font-medium px-4 py-2">"Serial"</th>
                                        <th class="text-left font-medium px-4 py-2 font-mono">"VID / PID"</th>
                                        <th class="text-left font-medium px-4 py-2 font-mono">"Bus / Addr"</th>
                                        <th class="text-right font-medium px-4 py-2"></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <For
                                        each=move || cams.clone()
                                        key=|c| c.serial.clone()
                                        children=move |cam| view! { <CameraRow cam=cam/> }
                                    />
                                </tbody>
                            </table>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn CameraRow(cam: CameraDto) -> impl IntoView {
    let serial = cam.serial.clone();
    let serial_short: String = serial.chars().take(12).collect();
    let model = format!(
        "{} {}",
        cam.manufacturer.clone().unwrap_or_default(),
        cam.product.clone().unwrap_or_default(),
    )
    .trim()
    .to_string();
    let model = if model.is_empty() { "(unknown)".into() } else { model };

    view! {
        <tr class="border-t border-border hover:bg-accent/30">
            <td class="px-4 py-2 font-medium">{model}</td>
            <td class="px-4 py-2 font-mono text-xs text-muted-foreground">{serial_short} "…"</td>
            <td class="px-4 py-2 font-mono text-xs">
                {format!("0x{:04X} / 0x{:04X}", cam.vendor_id, cam.product_id)}
            </td>
            <td class="px-4 py-2 font-mono text-xs">
                {format!("{} / {}", cam.bus_number, cam.device_address)}
            </td>
            <td class="px-4 py-2 text-right">
                <A href=format!("/camera/{}", serial)>
                    <Button variant=ButtonVariant::Default size=ui_button::ButtonSize::Sm>
                        "Open →"
                    </Button>
                </A>
            </td>
        </tr>
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Viewport,
    Overview,
    Inspector,
    Live,
    Lua,
    Files,
}

impl DetailTab {
    const ALL: &'static [(Self, &'static str)] = &[
        (Self::Viewport, "Viewport"),
        (Self::Overview, "Overview"),
        (Self::Inspector, "Inspector"),
        (Self::Live, "Live state"),
        (Self::Lua, "Lua REPL"),
        (Self::Files, "Files"),
    ];

    fn slug(self) -> &'static str {
        match self {
            Self::Viewport => "viewport",
            Self::Overview => "overview",
            Self::Inspector => "inspector",
            Self::Live => "live",
            Self::Lua => "lua",
            Self::Files => "files",
        }
    }

    fn from_slug(s: &str) -> Self {
        match s {
            "overview" => Self::Overview,
            "inspector" => Self::Inspector,
            "live" => Self::Live,
            "lua" => Self::Lua,
            "files" => Self::Files,
            _ => Self::Viewport,
        }
    }
}

/// Read `?tab=...` from the current URL (or default to Viewport).
fn tab_from_url() -> DetailTab {
    let Some(window) = web_sys::window() else { return DetailTab::Viewport };
    let Ok(search) = window.location().search() else { return DetailTab::Viewport };
    if search.is_empty() {
        return DetailTab::Viewport;
    }
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()
        .and_then(|p| p.get("tab"))
        .map(|s| DetailTab::from_slug(&s))
        .unwrap_or(DetailTab::Viewport)
}

/// Update the URL's `?tab=` query without triggering a router navigation —
/// preserves all in-page component state (Lua output, file-tree expansion,
/// live-state cache, etc.) across tab switches.
fn replace_tab_in_url(t: DetailTab) {
    use wasm_bindgen::JsValue;
    let Some(window) = web_sys::window() else { return };
    let pathname = window.location().pathname().unwrap_or_default();
    let new_url = format!("{pathname}?tab={}", t.slug());
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&new_url));
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct DirEntryDto {
    name: String,
    is_dir: bool,
    size: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct ListDirResponseDto {
    path: String,
    entries: Vec<DirEntryDto>,
    note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct InfoDto {
    serial: String,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    usb_manufacturer: Option<String>,
    usb_product: Option<String>,
    ptp_standard_version: u16,
    vendor_extension_id: u32,
    vendor_extension_version: u16,
    vendor_extension_desc: String,
    functional_mode: u16,
    ptp_manufacturer: String,
    ptp_model: String,
    device_version: String,
    serial_number: String,
    operations_supported: Vec<u16>,
    events_supported: Vec<u16>,
    device_properties_supported: Vec<u16>,
    capture_formats: Vec<u16>,
    image_formats: Vec<u16>,
    chdk_advertised: bool,
    chdk_version_major: Option<u32>,
    chdk_version_minor: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct LiveStateDto {
    in_record: Option<bool>,
    is_movie: Option<bool>,
    mode_code: Option<i32>,
    zoom: Option<i32>,
    exp_count: Option<i32>,
    vbatt_mv: Option<i32>,
    image_dir: Option<String>,
    free_kb: Option<i32>,
    iso_mode: Option<i32>,
    sv96: Option<i32>,
    tv96: Option<i32>,
    av96: Option<i32>,
    focus: Option<i32>,
    propset: Option<i32>,
    flash_mode: Option<i32>,
    flash_ready: Option<bool>,
    is_shooting: Option<bool>,
    raw: String,
}

/// Map known PTP standard opcode → name; unknowns return None.
fn ptp_op_name(code: u16) -> Option<&'static str> {
    Some(match code {
        0x1001 => "GetDeviceInfo",
        0x1002 => "OpenSession",
        0x1003 => "CloseSession",
        0x1004 => "GetStorageIDs",
        0x1005 => "GetStorageInfo",
        0x1006 => "GetNumObjects",
        0x1007 => "GetObjectHandles",
        0x1008 => "GetObjectInfo",
        0x1009 => "GetObject",
        0x100A => "GetThumb",
        0x100B => "DeleteObject",
        0x100C => "SendObjectInfo",
        0x100D => "SendObject",
        0x100E => "InitiateCapture",
        0x100F => "FormatStore",
        0x1012 => "SetObjectProtection",
        0x1014 => "GetDevicePropDesc",
        0x1015 => "GetDevicePropValue",
        0x1016 => "SetDevicePropValue",
        0x1017 => "ResetDevicePropValue",
        0x101B => "GetPartialObject",
        0x9999 => "CHDK (vendor)",
        c if c >= 0x9000 && c < 0x9999 => "Canon vendor",
        _ => return None,
    })
}

fn image_format_name(code: u16) -> Option<&'static str> {
    Some(match code {
        0x3000 => "Undefined",
        0x3001 => "EXIF/JPEG",
        0x3002 => "TIFF/EP",
        0x3006 => "RAW (Canon)",
        0x3008 => "PNG",
        0x3801 => "EXIF/JPEG",
        0x3800 => "Image (generic)",
        0xB103 => "CRW (Canon)",
        0xB982 => "MOV (Canon)",
        0xB105 => "CR2 (Canon RAW v2)",
        0xBF01 => "MPO (3D)",
        _ => return None,
    })
}

// ─── Pano rig page ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanoTab {
    Viewfinder,
}

impl PanoTab {
    const ALL: &'static [(Self, &'static str)] = &[(Self::Viewfinder, "Viewfinder")];
}

#[component]
fn PanoPage() -> impl IntoView {
    let (tab, set_tab) = signal(PanoTab::Viewfinder);

    view! {
        <h2 class="text-base font-semibold mb-1">"Camera Rig"</h2>
        <p class="text-sm text-muted-foreground mb-4">
            "The four fixed cameras of the panorama rig, by physical slot."
        </p>

        // ─── Tab strip ─────────────────────────────────────────────────
        <div class="border-b border-border mb-5 flex gap-0.5 -mx-6 px-6 overflow-x-auto">
            {PanoTab::ALL.iter().map(|(t, label)| {
                let t = *t;
                view! {
                    <button
                        class=move || {
                            let active = tab.get() == t;
                            format!(
                                "px-3 py-2 text-sm font-medium border-b-2 transition-colors whitespace-nowrap cursor-pointer {}",
                                if active {
                                    "border-foreground text-foreground"
                                } else {
                                    "border-transparent text-muted-foreground hover:text-foreground"
                                },
                            )
                        }
                        on:click=move |_| set_tab.set(t)
                    >{*label}</button>
                }
            }).collect_view()}
        </div>

        // ─── Tab content ───────────────────────────────────────────────
        {move || match tab.get() {
            PanoTab::Viewfinder => view! { <PanoViewfinder/> }.into_any(),
        }}
    }
}

/// One row of four live viewports. Each `RigCamera` self-paces its own poll
/// loop, so the cameras decorrelate naturally and never overlap requests.
#[component]
fn PanoViewfinder() -> impl IntoView {
    view! {
        <div class="grid grid-cols-4 gap-2">
            {RIG_SERIALS.iter().enumerate().map(|(idx, serial)| {
                view! { <RigCamera idx=idx serial=serial.to_string()/> }
            }).collect_view()}
        </div>
        <PanoShootButton/>
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct ClockSyncSlotDto {
    idx: usize,
    serial: Option<String>,
    status: String,
    offset_ms: Option<f64>,
    offset_rtt_ms: Option<f64>,
    target_tick: Option<i64>,
    busy_wait_ms: Option<i64>,
    actual_exit_host_ms: Option<f64>,
    overshoot_ms: Option<f64>,
    fired: Option<bool>,
    image_path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct ClockSyncReportDto {
    slots: Vec<ClockSyncSlotDto>,
    inter_camera_skew_ms: Option<f64>,
    target_host_ms: f64,
    lead_ms: f64,
    samples: usize,
    elapsed_ms: u64,
}

async fn post_clocksync(flash: bool) -> Result<ClockSyncReportDto, String> {
    let resp = gloo_net::http::Request::post(&format!("/api/pano/shoot_clocksync?flash={flash}"))
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<ClockSyncReportDto>()
        .await
        .map_err(|e| format!("decode: {e}"))
}

/// "Shoot all (clock-synced)" — fires the sophisticated multi-camera shoot
/// and renders the per-camera skew diagnostics it returns.
#[component]
fn PanoShootButton() -> impl IntoView {
    let (pending, set_pending) = signal(false);
    let (flash, set_flash) = signal(false);
    // Bumped each shoot so the "latest capture" <img> URLs change and refetch.
    let (seq, set_seq) = signal(0u64);
    let (report, set_report) = signal::<Option<Result<ClockSyncReportDto, String>>>(None);

    let shoot = move |_| {
        let flash_on = flash.get();
        set_pending.set(true);
        set_report.set(None);
        set_seq.update(|n| *n = n.wrapping_add(1));
        wasm_bindgen_futures::spawn_local(async move {
            let res = post_clocksync(flash_on).await;
            set_report.set(Some(res));
            set_pending.set(false);
        });
    };

    view! {
        <div class="mt-6 border-t border-border pt-4">
            <div class="flex items-center gap-3">
                <Button variant=ButtonVariant::Default on:click=shoot attr:disabled=move || pending.get()>
                    {move || if pending.get() { "Shooting… (~3 s)" } else { "Shoot all (clock-synced)" }}
                </Button>
                <label class="flex items-center gap-1.5 text-sm cursor-pointer select-none">
                    <input
                        type="checkbox"
                        class="accent-foreground cursor-pointer"
                        prop:checked=move || flash.get()
                        on:change=move |_| set_flash.update(|v| *v = !*v)
                    />
                    "Flash"
                </label>
                <span class="text-xs text-muted-foreground">
                    "Calibrates each camera's clock, then fires them on a shared deadline. "
                    "Cameras must be in record mode (the viewfinder above keeps them there)."
                </span>
            </div>
            {move || match report.get() {
                None => ().into_any(),
                Some(Err(e)) => view! {
                    <p class="text-sm text-destructive mt-3">{format!("Shoot failed: {e}")}</p>
                }.into_any(),
                Some(Ok(r)) => render_clocksync_report(r, seq.get()).into_any(),
            }}
        </div>
    }
}

fn render_clocksync_report(r: ClockSyncReportDto, bust: u64) -> impl IntoView {
    let skew = r.inter_camera_skew_ms
        .map(|s| format!("{s:.1} ms"))
        .unwrap_or_else(|| "—".into());
    let fired_n = r.slots.iter().filter(|s| s.fired == Some(true)).count();
    let active_n = r.slots.iter().filter(|s| s.status != "empty").count();

    // One gallery cell per slot. Cameras that fired show a thumbnail of the
    // file they wrote (path comes natively from the shoot: get_image_dir +
    // exp_count, no FS scan); the rest show a blank placeholder so the row
    // stays aligned with the four viewfinders above.
    let gallery: Vec<(usize, Option<(String, String)>)> = r.slots.iter()
        .map(|s| {
            let img = match (s.status.as_str(), &s.serial, &s.image_path) {
                ("fired", Some(ser), Some(p)) => Some((ser.clone(), p.clone())),
                _ => None,
            };
            (s.idx, img)
        })
        .collect();

    let rows = r.slots.into_iter().filter(|s| s.status != "empty").map(|s| {
        let opt_f = |o: Option<f64>| o.map(|v| format!("{v:.1}")).unwrap_or_else(|| "—".into());
        let opt_i = |o: Option<i64>| o.map(|v| v.to_string()).unwrap_or_else(|| "—".into());
        let (badge_cls, badge) = match s.status.as_str() {
            "fired" => ("text-green-500", "✓ fired"),
            "missed" => ("text-amber-500", "missed"),
            _ => ("text-destructive", "error"),
        };
        view! {
            <tr class="border-t border-border">
                <td class="px-3 py-1.5 font-medium">{format!("camera {}", s.idx + 1)}</td>
                <td class=format!("px-3 py-1.5 font-medium {badge_cls}")>{badge}</td>
                <td class="px-3 py-1.5 font-mono tabular-nums">{opt_f(s.offset_ms)}</td>
                <td class="px-3 py-1.5 font-mono tabular-nums">{opt_f(s.offset_rtt_ms)}</td>
                <td class="px-3 py-1.5 font-mono tabular-nums">{opt_i(s.busy_wait_ms)}</td>
                <td class="px-3 py-1.5 font-mono tabular-nums">{opt_f(s.overshoot_ms)}</td>
                <td class="px-3 py-1.5 text-destructive text-xs">{s.error.unwrap_or_default()}</td>
            </tr>
        }
    }).collect_view();

    view! {
        <div class="mt-3">
            <div class="flex flex-wrap items-baseline gap-x-6 gap-y-1 mb-2 text-sm">
                <span>
                    "Inter-camera skew: "
                    <span class="font-mono font-semibold">{skew}</span>
                </span>
                <span class="text-muted-foreground">
                    {format!("{fired_n}/{active_n} fired · lead {:.0} ms · {} samples · {} ms total",
                        r.lead_ms, r.samples, r.elapsed_ms)}
                </span>
            </div>
            <div class="border border-border rounded-md bg-card overflow-hidden">
                <table class="w-full text-sm">
                    <thead class="bg-muted text-xs uppercase tracking-wide text-muted-foreground">
                        <tr>
                            <th class="text-left font-medium px-3 py-2">"camera"</th>
                            <th class="text-left font-medium px-3 py-2">"status"</th>
                            <th class="text-left font-medium px-3 py-2">"offset ms"</th>
                            <th class="text-left font-medium px-3 py-2">"rtt ms"</th>
                            <th class="text-left font-medium px-3 py-2">"busy-wait ms"</th>
                            <th class="text-left font-medium px-3 py-2">"overshoot ms"</th>
                            <th class="text-left font-medium px-3 py-2">"error"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            <div class="mt-4">
                <h4 class="text-sm font-medium mb-2">"Latest captures"</h4>
                <div class="grid grid-cols-4 gap-2">
                    {gallery.into_iter().map(|(idx, img)| {
                        // Unlike the raw viewfinder frames, the captured stills are
                        // full-res, square-pixel, and carry an EXIF orientation tag
                        // (the camera's sensor noticed the physical rotation) — so
                        // the browser shows them upright; just contain them.
                        let (inner, open_link) = match img {
                            Some((ser, path)) => {
                                let url = format!("/api/file/{}?path={}", ser, urlencode(&path));
                                let img_view = view! {
                                    <img
                                        class="max-w-full max-h-full object-contain"
                                        src=format!("{url}&t={bust}")
                                        alt=format!("camera {} latest capture", idx + 1)
                                    />
                                }.into_any();
                                let link = view! {
                                    <a
                                        href=url
                                        target="_blank"
                                        rel="noopener"
                                        class="text-[10px] text-muted-foreground hover:text-foreground underline"
                                    >"open ↗"</a>
                                }.into_any();
                                (img_view, link)
                            }
                            None => (
                                view! { <span class="text-xs text-muted-foreground">"no capture"</span> }.into_any(),
                                ().into_any(),
                            ),
                        };
                        view! {
                            <div class="bg-black rounded-lg overflow-hidden">
                                <div class="px-2 py-1 bg-card/80 text-xs font-medium flex items-center justify-between gap-2">
                                    <span>{format!("camera {}", idx + 1)}</span>
                                    {open_link}
                                </div>
                                <div class="relative w-full aspect-[3/4] overflow-hidden flex items-center justify-center">
                                    {inner}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </div>
        </div>
    }
}

/// Delay between a completed frame and the next request. Keeps a healthy
/// camera smooth without busy-looping the USB bus.
const RIG_FRAME_GAP_MS: u32 = 120;

/// A single rig cell: persistent last-frame `<img>` + a self-terminating
/// fetch loop. On success it swaps in a fresh object-URL (revoking the old
/// one); on failure it leaves the last frame up and bumps an error counter.
#[component]
fn RigCamera(idx: usize, serial: String) -> impl IntoView {
    let serial_short: String = serial.chars().take(12).collect();

    // Currently-displayed object URL (last good frame), and the count of
    // consecutive failed fetches since the last success.
    let frame_url = RwSignal::new(Option::<String>::None);
    let err_count = RwSignal::new(0u32);

    // `alive` is flipped by on_cleanup when the cell unmounts; the loop polls
    // it to self-terminate. Arc<AtomicBool> because on_cleanup callbacks must
    // be Send. The object-URL bookkeeping lives entirely inside the one async
    // task as a plain local, so no shared interior-mutability is needed.
    let alive = Arc::new(AtomicBool::new(true));

    wasm_bindgen_futures::spawn_local({
        let serial = serial.clone();
        let alive = alive.clone();
        async move {
            let mut current: Option<String> = None;
            loop {
                if !alive.load(Ordering::Relaxed) {
                    break;
                }
                let fetched = fetch_viewport_jpeg_bytes(&serial).await;
                if !alive.load(Ordering::Relaxed) {
                    break;
                }
                match fetched.and_then(|b| bytes_to_object_url(&b)) {
                    Ok(new_url) => {
                        // Revoke the prior frame's URL — it's already painted,
                        // so freeing the mapping doesn't disturb the display —
                        // then swap the new frame in.
                        if let Some(old) = current.replace(new_url.clone()) {
                            revoke_object_url(&old);
                        }
                        frame_url.set(Some(new_url));
                        err_count.set(0);
                    }
                    Err(()) => err_count.update(|c| *c += 1),
                }
                gloo_timers::future::TimeoutFuture::new(RIG_FRAME_GAP_MS).await;
            }
            // Final revoke so we don't leak the last frame on teardown.
            if let Some(old) = current.take() {
                revoke_object_url(&old);
            }
        }
    });

    on_cleanup(move || alive.store(false, Ordering::Relaxed));

    // Per-camera mode switching (record extends the lens + runs the viewfinder
    // pipeline; play retracts it). Mirrors the single-camera Viewport tab.
    let (mode_pending, set_mode_pending) = signal(false);
    let (mode_status, set_mode_status) = signal::<Option<String>>(None);
    let switch_mode = {
        let serial = serial.clone();
        move |which: &'static str| {
            let serial = serial.clone();
            set_mode_pending.set(true);
            set_mode_status.set(None);
            wasm_bindgen_futures::spawn_local(async move {
                let url = format!("/api/mode/{which}/{serial}");
                let res = gloo_net::http::Request::post(&url)
                    .send()
                    .await
                    .map_err(|e| format!("network: {e}"))
                    .and_then(|r| if r.ok() { Ok(r) } else { Err(format!("HTTP {}", r.status())) });
                match res {
                    Ok(r) => match r.json::<serde_json::Value>().await {
                        Ok(v) => {
                            let m = v.get("mode").and_then(|m| m.as_str()).unwrap_or("?").to_string();
                            set_mode_status.set(Some(m));
                        }
                        Err(e) => set_mode_status.set(Some(format!("decode: {e}"))),
                    },
                    Err(e) => set_mode_status.set(Some(format!("err: {e}"))),
                }
                set_mode_pending.set(false);
            });
        }
    };
    let on_record = {
        let switch_mode = switch_mode.clone();
        move |_| switch_mode("record")
    };
    let on_play = move |_| switch_mode("play");

    view! {
        <div class="bg-black rounded-lg overflow-hidden flex flex-col">
            <div class="flex items-center justify-between px-2 py-1 bg-card/80 text-xs">
                <span class="font-medium">{format!("camera {}", idx + 1)}</span>
                <span class="font-mono text-muted-foreground">{serial_short} "…"</span>
            </div>
            <div class="relative w-full aspect-[3/4] overflow-hidden bg-black">
                {move || match frame_url.get() {
                    Some(url) => view! {
                        // The live buffer is an anamorphic 720×240 (non-square
                        // pixels the camera intends to display at 4:3), and the
                        // cameras are physically mounted rotated 90°. So: size the
                        // image to a 4:3 landscape box and `object-fit:fill` to
                        // stretch the 720×240 to true proportions, *then* rotate
                        // CCW into a 3:4 portrait that exactly fills the cell.
                        // Percentages keep it responsive — width 133.33% = cell
                        // height, height 75% = cell width, so the rotated 4:3 box
                        // covers the 3:4 cell with no letterboxing.
                        <img
                            class="absolute left-1/2 top-1/2"
                            style="width:133.3333%;height:75%;object-fit:fill;transform:translate(-50%,-50%) rotate(-90deg)"
                            src=url
                            alt=format!("rig camera {} viewport", idx + 1)
                        />
                    }.into_any(),
                    None => view! {
                        <div class="absolute inset-0 flex items-center justify-center">
                            <span class="text-xs text-muted-foreground">"waiting for first frame…"</span>
                        </div>
                    }.into_any(),
                }}
                // Consecutive-failure badge. Hidden while healthy.
                {move || {
                    let n = err_count.get();
                    if n == 0 {
                        ().into_any()
                    } else if frame_url.with(|f| f.is_some()) {
                        view! {
                            <span class="absolute top-1 right-1 px-1.5 py-0.5 rounded bg-destructive/80 text-white text-[10px] font-mono z-10">
                                {format!("⚠ {n} stale")}
                            </span>
                        }.into_any()
                    } else {
                        view! {
                            <div class="absolute inset-0 flex items-center justify-center">
                                <span class="px-2 py-1 rounded bg-destructive/80 text-white text-xs font-mono">
                                    {format!("⚠ no signal — {n} failed")}
                                </span>
                            </div>
                        }.into_any()
                    }
                }}
            </div>
            // ─── Per-camera mode controls ──────────────────────────────
            <div class="flex items-center gap-1 px-2 py-1.5 bg-card/80 border-t border-border">
                <Button variant=ButtonVariant::Default size=ui_button::ButtonSize::Sm
                    on:click=on_record attr:disabled=move || mode_pending.get()>
                    "Record"
                </Button>
                <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm
                    on:click=on_play attr:disabled=move || mode_pending.get()>
                    "Play"
                </Button>
                <span class="text-[10px] text-muted-foreground ml-auto truncate">
                    {move || {
                        if mode_pending.get() {
                            "switching…".to_string()
                        } else {
                            mode_status.get().map(|m| format!("→ {m}")).unwrap_or_default()
                        }
                    }}
                </span>
            </div>
        </div>
    }
}

/// Fetch one viewport frame. Returns the JPEG bytes only on a genuine
/// `image/jpeg` 200; the server's SVG-placeholder responses (camera asleep,
/// in play mode, errored) are treated as failures so the caller can keep the
/// last good frame instead of flashing a placeholder.
async fn fetch_viewport_jpeg_bytes(serial: &str) -> Result<Vec<u8>, ()> {
    let url = format!("/api/viewport/{serial}");
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|_| ())?;
    if !resp.ok() {
        return Err(());
    }
    let ct = resp.headers().get("content-type").unwrap_or_default();
    if !ct.contains("jpeg") {
        return Err(());
    }
    resp.binary().await.map_err(|_| ())
}

/// Wrap JPEG bytes in a Blob and mint an object URL the `<img>` can render.
fn bytes_to_object_url(bytes: &[u8]) -> Result<String, ()> {
    use wasm_bindgen::JsValue;
    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    arr.copy_from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from(arr));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&JsValue::from(parts)).map_err(|_| ())?;
    web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| ())
}

fn revoke_object_url(url: &str) {
    let _ = web_sys::Url::revoke_object_url(url);
}

// ─── WiFi page ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct ApInfoDto {
    iface: String,
    ssid: String,
    password: String,
    ip: Option<String>,
    channel: Option<u32>,
    connected_clients: Option<usize>,
    note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct WifiClientDto {
    iface: String,
    ssid: Option<String>,
    state: Option<String>,
    ip: Option<String>,
    signal_dbm: Option<i32>,
    note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct WifiStatusDto {
    ap: ApInfoDto,
    client: WifiClientDto,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct SetClientResultDto {
    ok: bool,
    message: String,
    network_id: Option<i32>,
    client: WifiClientDto,
}

async fn fetch_wifi() -> Result<WifiStatusDto, String> {
    let response = gloo_net::http::Request::get("/api/wifi")
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    response
        .json::<WifiStatusDto>()
        .await
        .map_err(|e| format!("decode: {e}"))
}

async fn post_set_client(ssid: String, psk: String) -> Result<SetClientResultDto, String> {
    let body = serde_json::json!({ "ssid": ssid, "psk": psk });
    let resp = gloo_net::http::Request::post("/api/wifi/client")
        .header("content-type", "application/json")
        .body(body.to_string())
        .map_err(|e| format!("build: {e}"))?
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;
    if !resp.ok() {
        // The server returns { "error": "..." } on failure (HTTP 500).
        let txt = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&txt)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or(txt);
        return Err(format!("HTTP {} — {msg}", resp.status()));
    }
    resp.json::<SetClientResultDto>()
        .await
        .map_err(|e| format!("decode: {e}"))
}

#[component]
fn WifiPage() -> impl IntoView {
    let status = LocalResource::new(fetch_wifi);

    view! {
        <div class="flex items-baseline justify-between mb-4 gap-3">
            <h2 class="text-base font-semibold">"WiFi"</h2>
            <Button
                variant=ButtonVariant::Outline
                size=ui_button::ButtonSize::Sm
                on:click=move |_| { status.refetch(); }
            >
                "Refresh"
            </Button>
        </div>
        <Suspense fallback=|| view! { <p class="text-sm text-muted-foreground">"Loading…"</p> }>
            {move || Suspend::new(async move {
                match status.await {
                    Err(e) => view! {
                        <p class="text-sm text-destructive">{format!("Error: {e}")}</p>
                    }.into_any(),
                    Ok(s) => view! {
                        <div class="divide-y divide-border border-t border-border">
                            <ApCard ap=s.ap/>
                            <ClientCard client=s.client status=status/>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn ApiDocsPage() -> impl IntoView {
    view! {
        <div class="flex items-baseline justify-between mb-4 gap-3">
            <h2 class="text-base font-semibold">"API"</h2>
            // Escape hatch to the standalone UI (full width, own tab).
            <a
                href="/swagger-ui/"
                rel="external"
                target="_blank"
                class="text-sm text-muted-foreground hover:text-foreground"
            >"Open full page ↗"</a>
        </div>
        // Swagger UI is a separate backend-served app; embedding it in an iframe
        // is the clean way to keep our nav bar above it. Height fills the
        // viewport below the header so it doesn't need its own outer scrollbar.
        <iframe
            src="/swagger-ui/"
            title="Swagger UI"
            class="w-full h-[calc(100vh-9rem)] border border-border rounded-md bg-white"
        ></iframe>
    }
}

#[component]
fn ApCard(ap: ApInfoDto) -> impl IntoView {
    let opt = |o: Option<String>| o.unwrap_or_else(|| "—".into());
    view! {
        <section class="py-5">
            <div class="flex items-center justify-between mb-3">
                <h3 class="text-sm font-semibold">"Access point"</h3>
                <span class="font-mono text-xs text-muted-foreground">{ap.iface.clone()}</span>
            </div>
            <p class="text-xs text-muted-foreground mb-3">
                "The network the rig broadcasts in the field — connect your phone or laptop to this to reach the UI."
            </p>
            <dl class="grid grid-cols-2 md:grid-cols-3 gap-x-8 gap-y-4 text-sm">
                <WifiField label="SSID" value=ap.ssid.clone() mono=true/>
                <WifiField label="Password" value=ap.password.clone() mono=true/>
                <WifiField label="Gateway IP" value=opt(ap.ip.clone()) mono=true/>
                <WifiField label="Channel" value=ap.channel.map(|c| c.to_string()).unwrap_or_else(|| "—".into()) mono=true/>
                <WifiField
                    label="Connected clients"
                    value=ap.connected_clients.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
                    mono=true
                />
            </dl>
            {ap.note.map(|n| view! {
                <p class="text-xs text-muted-foreground mt-3 italic">{format!("note: {n}")}</p>
            })}
        </section>
    }
}

#[component]
fn ClientCard(
    client: WifiClientDto,
    status: LocalResource<Result<WifiStatusDto, String>>,
) -> impl IntoView {
    let connected = client.ssid.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
    let ssid = client.ssid.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "(not associated)".into());
    view! {
        <section class="py-5">
            <div class="flex items-center justify-between mb-3">
                <h3 class="text-sm font-semibold">"Client radio"</h3>
                <span class="font-mono text-xs text-muted-foreground">{client.iface.clone()}</span>
            </div>
            <p class="text-xs text-muted-foreground mb-3">
                "Joins an existing network (home WiFi, a phone hotspot). Use the Join button below to point it at one."
            </p>
            <dl class="grid grid-cols-2 md:grid-cols-3 gap-x-8 gap-y-4 text-sm">
                <div>
                    <dt class="text-xs text-muted-foreground uppercase tracking-wide">"Joined network"</dt>
                    <dd class=move || if connected { "font-mono text-xs break-all" } else { "font-mono text-xs break-all text-muted-foreground" }>
                        {ssid}
                    </dd>
                </div>
                <WifiField label="State" value=client.state.clone().unwrap_or_else(|| "—".into()) mono=true/>
                <WifiField label="IP" value=client.ip.clone().unwrap_or_else(|| "—".into()) mono=true/>
                <WifiField
                    label="Signal"
                    value=client.signal_dbm.map(|s| format!("{s} dBm")).unwrap_or_else(|| "—".into())
                    mono=true
                />
            </dl>
            {client.note.map(|n| view! {
                <p class="text-xs text-muted-foreground mt-3 italic">{format!("note: {n}")}</p>
            })}
            <div class="mt-4">
                <WifiJoinDialog status=status/>
            </div>
        </section>
    }
}

#[component]
fn WifiField(label: &'static str, value: String, mono: bool) -> impl IntoView {
    let value_cls = if mono { "font-mono text-xs break-all" } else { "text-sm" };
    view! {
        <div>
            <dt class="text-xs text-muted-foreground uppercase tracking-wide">{label}</dt>
            <dd class=value_cls>{if value.is_empty() { "—".to_string() } else { value }}</dd>
        </div>
    }
}

#[component]
fn WifiJoinDialog(status: LocalResource<Result<WifiStatusDto, String>>) -> impl IntoView {
    let (ssid, set_ssid) = signal(String::new());
    let (psk, set_psk) = signal(String::new());
    let (show_psk, set_show_psk) = signal(false);
    let (pending, set_pending) = signal(false);
    let (result, set_result) = signal::<Option<Result<String, String>>>(None);

    let submit = move |_| {
        let ssid_v = ssid.get().trim().to_string();
        if ssid_v.is_empty() {
            set_result.set(Some(Err("SSID is required".into())));
            return;
        }
        let psk_v = psk.get();
        set_pending.set(true);
        set_result.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            match post_set_client(ssid_v, psk_v).await {
                Ok(r) => {
                    set_result.set(Some(Ok(r.message)));
                    // Pull fresh radio status into the cards behind the dialog.
                    status.refetch();
                }
                Err(e) => set_result.set(Some(Err(e))),
            }
            set_pending.set(false);
        });
    };

    view! {
        <Dialog>
            <DialogTrigger variant=ButtonVariant::Default size=ui_button::ButtonSize::Sm>
                "Join a network"
            </DialogTrigger>
            <DialogContent class="max-w-md text-left">
                <DialogHeader>
                    <DialogTitle>"Join a network"</DialogTitle>
                    <DialogDescription>
                        "Applied live via wpa_supplicant. Takes effect immediately but does "
                        "not survive a NixOS rebuild — add it to the flake to make it permanent."
                    </DialogDescription>
                </DialogHeader>

                <DialogBody class="mt-4">
                    <div>
                        <label class="block text-xs text-muted-foreground uppercase tracking-wide mb-1">"SSID"</label>
                        <input
                            class="w-full text-sm bg-background border border-border rounded-md px-3 py-2 focus:outline-none focus:ring-2 focus:ring-ring"
                            prop:value=move || ssid.get()
                            on:input=move |ev| set_ssid.set(input_value(&ev))
                            placeholder="network name"
                        />
                    </div>

                    <div>
                        <label class="block text-xs text-muted-foreground uppercase tracking-wide mb-1">"Password"</label>
                        <input
                            class="w-full text-sm bg-background border border-border rounded-md px-3 py-2 focus:outline-none focus:ring-2 focus:ring-ring"
                            type=move || if show_psk.get() { "text" } else { "password" }
                            prop:value=move || psk.get()
                            on:input=move |ev| set_psk.set(input_value(&ev))
                            placeholder="8–63 chars, leave blank for an open network"
                        />
                        <label class="flex items-center gap-1.5 text-xs text-muted-foreground mt-1.5 cursor-pointer select-none">
                            <input
                                type="checkbox"
                                class="accent-foreground cursor-pointer"
                                prop:checked=move || show_psk.get()
                                on:change=move |_| set_show_psk.update(|v| *v = !*v)
                            />
                            "Show password"
                        </label>
                    </div>

                    {move || match result.get() {
                        None => ().into_any(),
                        Some(Ok(msg)) => view! { <p class="text-sm text-green-500">{msg}</p> }.into_any(),
                        Some(Err(e)) => view! { <p class="text-sm text-destructive">{e}</p> }.into_any(),
                    }}
                </DialogBody>

                <DialogFooter class="mt-6">
                    <DialogClose variant=ButtonVariant::Outline size=ui_button::ButtonSize::Default>
                        "Cancel"
                    </DialogClose>
                    <Button variant=ButtonVariant::Default on:click=submit attr:disabled=move || pending.get()>
                        {move || if pending.get() { "Connecting…" } else { "Connect" }}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    }
}

#[component]
fn CameraDetailPage() -> impl IntoView {
    let params = use_params::<ViewportParams>();
    let serial = move || {
        params
            .with(|p| p.as_ref().ok().and_then(|p| p.serial.clone()))
            .unwrap_or_default()
    };

    let (tab, set_tab) = signal(tab_from_url());
    // Sync the URL whenever the active tab changes — bookmark/share-friendly
    // and survives browser refresh without re-mounting the page.
    Effect::new(move |_| {
        replace_tab_in_url(tab.get());
    });

    // Load full camera info on mount; shared across Overview and Inspector tabs.
    let info = LocalResource::new({
        let serial = serial.clone();
        move || fetch_info(serial())
    });

    view! {
        <A href="/" attr:class="text-xs text-muted-foreground hover:text-foreground">"← back to cameras"</A>
        <h2 class="text-base font-semibold mt-2 mb-4">
            {move || format!("Inspect: {}…", serial().chars().take(16).collect::<String>())}
        </h2>

        // ─── Tab strip ─────────────────────────────────────────────────────
        <div class="border-b border-border mb-5 flex gap-0.5 -mx-6 px-6 overflow-x-auto">
            {DetailTab::ALL.iter().map(|(t, label)| {
                let t = *t;
                view! {
                    <button
                        class=move || {
                            let active = tab.get() == t;
                            format!(
                                "px-3 py-2 text-sm font-medium border-b-2 transition-colors whitespace-nowrap cursor-pointer {}",
                                if active {
                                    "border-foreground text-foreground"
                                } else {
                                    "border-transparent text-muted-foreground hover:text-foreground"
                                },
                            )
                        }
                        on:click=move |_| set_tab.set(t)
                    >{*label}</button>
                }
            }).collect_view()}
        </div>

        // ─── Tab content ───────────────────────────────────────────────────
        {move || match tab.get() {
            DetailTab::Viewport => view! { <ViewportTab serial=serial()/> }.into_any(),
            DetailTab::Overview => view! {
                <Suspense fallback=|| view! { <p class="text-sm text-muted-foreground">"Loading camera info…"</p> }>
                    {move || Suspend::new(async move {
                        match info.await {
                            Err(e) => view! {
                                <p class="text-sm text-destructive">{format!("Failed to fetch info: {e}")}</p>
                            }.into_any(),
                            Ok(i) => render_overview(i).into_any(),
                        }
                    })}
                </Suspense>
            }.into_any(),
            DetailTab::Inspector => view! {
                <Suspense fallback=|| view! { <p class="text-sm text-muted-foreground">"Loading camera info…"</p> }>
                    {move || Suspend::new(async move {
                        match info.await {
                            Err(e) => view! {
                                <p class="text-sm text-destructive">{format!("Failed to fetch info: {e}")}</p>
                            }.into_any(),
                            Ok(i) => render_inspector(i).into_any(),
                        }
                    })}
                </Suspense>
            }.into_any(),
            DetailTab::Live => view! { <LiveStateTab serial=serial()/> }.into_any(),
            DetailTab::Lua => view! { <LuaReplTab serial=serial()/> }.into_any(),
            DetailTab::Files => view! { <FilesTab serial=serial()/> }.into_any(),
        }}
    }
}

#[component]
fn LiveStateTab(serial: String) -> impl IntoView {
    let (live_state, set_live_state) = signal::<Option<Result<LiveStateDto, String>>>(None);
    let (state_pending, set_state_pending) = signal(false);

    let refresh = {
        let serial = serial.clone();
        move |_| {
            let serial = serial.clone();
            set_state_pending.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let res = fetch_live_state(serial).await;
                set_live_state.set(Some(res));
                set_state_pending.set(false);
            });
        }
    };
    // Initial fetch
    {
        let serial = serial.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let res = fetch_live_state(serial).await;
            set_live_state.set(Some(res));
        });
    }

    view! {
        <div class="flex items-center gap-2 mb-3">
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm
                on:click=refresh attr:disabled=move || state_pending.get()>
                {move || if state_pending.get() { "Refreshing…" } else { "Refresh" }}
            </Button>
            <span class="text-xs text-muted-foreground">
                "One Lua script, all fields in one PTP round-trip."
            </span>
        </div>
        {move || match live_state.get() {
            None => view! { <span class="text-sm text-muted-foreground">"(loading)"</span> }.into_any(),
            Some(Err(e)) => view! { <span class="text-sm text-destructive">{format!("Error: {e}")}</span> }.into_any(),
            Some(Ok(s)) => render_live_state(s).into_any(),
        }}
    }
}

#[component]
fn LuaReplTab(serial: String) -> impl IntoView {
    let (source, set_source) = signal(String::from("return get_exp_count()"));
    let (output, set_output) = signal::<Vec<String>>(Vec::new());
    let (pending, set_pending) = signal(false);
    let (last_ms, set_last_ms) = signal::<Option<u64>>(None);

    let run_lua = {
        let serial = serial.clone();
        move |_| {
            let serial = serial.clone();
            let src = source.get();
            set_pending.set(true);
            set_output.set(Vec::new());
            set_last_ms.set(None);
            wasm_bindgen_futures::spawn_local(async move {
                let url = format!("/api/exec/{serial}");
                let body = serde_json::json!({ "source": src });
                let res = gloo_net::http::Request::post(&url)
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .ok()
                    .map(|r| r.send());
                let lines = if let Some(fut) = res {
                    match fut.await {
                        Ok(resp) => match resp.json::<serde_json::Value>().await {
                            Ok(v) => format_exec_response(&v),
                            Err(e) => vec![format!("decode error: {e}")],
                        },
                        Err(e) => vec![format!("network error: {e}")],
                    }
                } else {
                    vec!["could not construct request".into()]
                };
                for l in &lines {
                    if let Some(ms) = l.strip_prefix("elapsed_ms=") {
                        set_last_ms.set(ms.parse().ok());
                    }
                }
                set_output.set(lines.into_iter().filter(|l| !l.starts_with("elapsed_ms=")).collect());
                set_pending.set(false);
            });
        }
    };

    let preset = move |s: &'static str| {
        let s = s.to_string();
        move |_| set_source.set(s.clone())
    };

    view! {
        <p class="text-sm text-muted-foreground mb-3">
            "Send arbitrary Lua to the camera. Output shows every message returned."
        </p>

        <div class="flex flex-wrap gap-2 mb-3">
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("return get_mode()")>
                "get_mode()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("return get_zoom()")>
                "get_zoom()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("return get_exp_count()")>
                "get_exp_count()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("return get_image_dir()")>
                "get_image_dir()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("return get_vbatt()")>
                "get_vbatt()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("return get_propset()")>
                "get_propset()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("shoot()")>
                "shoot()"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("switch_mode_usb(1) sleep(2000) return get_mode()")>
                "→ record"
            </Button>
            <Button variant=ButtonVariant::Outline size=ui_button::ButtonSize::Sm on:click=preset("switch_mode_usb(0) sleep(2000) return get_mode()")>
                "→ play"
            </Button>
        </div>

        <textarea
            class="w-full font-mono text-sm bg-card border border-border rounded-md p-3 min-h-[100px] focus:outline-none focus:ring-2 focus:ring-ring"
            prop:value=move || source.get()
            on:input=move |ev| set_source.set(event_target_value(&ev))
        />

        <div class="flex items-center gap-2 mt-2 mb-4">
            <Button variant=ButtonVariant::Default on:click=run_lua attr:disabled=move || pending.get()>
                {move || if pending.get() { "Running…" } else { "Run" }}
            </Button>
            <Button variant=ButtonVariant::Ghost size=ui_button::ButtonSize::Sm on:click=move |_| { set_output.set(Vec::new()); set_last_ms.set(None); }>
                "Clear output"
            </Button>
            <span class="text-xs text-muted-foreground ml-2">
                {move || last_ms.get().map(|ms| format!("{ms} ms")).unwrap_or_default()}
            </span>
        </div>

        <div class="bg-card border border-border rounded-md p-3 min-h-[120px] font-mono text-sm whitespace-pre-wrap">
            {move || {
                let lines = output.get();
                if lines.is_empty() {
                    view! { <span class="text-muted-foreground">"(no output yet)"</span> }.into_any()
                } else {
                    lines
                        .into_iter()
                        .map(|line| view! { <div>{line}</div> })
                        .collect_view()
                        .into_any()
                }
            }}
        </div>
    }
}

// ─── Files tab: experimental tree-style directory browser ──────────────

#[derive(Clone)]
enum FetchResult {
    Loading,
    Ok(Vec<DirEntryDto>),
    Err(String),
    Note(String),
}

#[derive(Clone)]
struct TreeContext {
    serial: String,
    cache: RwSignal<HashMap<String, FetchResult>>,
    selected: RwSignal<Option<(String, String)>>, // (camera path, content url)
}

/// Fire off a fetch for `path` and stuff the result into `cache`. No-op if a
/// fetch is already in flight or done. Idempotent — calling twice with the
/// same path before the first finishes will not duplicate the request.
fn ensure_loaded(ctx: TreeContext, path: String) {
    let already = ctx.cache.with(|c| c.contains_key(&path));
    if already {
        return;
    }
    ctx.cache.update(|c| {
        c.insert(path.clone(), FetchResult::Loading);
    });
    let serial = ctx.serial.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let result = match fetch_dir(&serial, &path).await {
            Ok(resp) => match resp.note {
                Some(n) => FetchResult::Note(n),
                None => FetchResult::Ok(resp.entries),
            },
            Err(e) => FetchResult::Err(e),
        };
        ctx.cache.update(|c| {
            c.insert(path, result);
        });
    });
}

#[component]
fn FilesTab(serial: String) -> impl IntoView {
    let cache = RwSignal::new(HashMap::<String, FetchResult>::new());
    let selected = RwSignal::new(None::<(String, String)>);
    let ctx = TreeContext {
        serial: serial.clone(),
        cache,
        selected,
    };

    // Kick off the root fetch immediately so the tree isn't empty on first
    // paint. Subsequent expansions lazily fetch on demand.
    ensure_loaded(ctx.clone(), "A".to_string());

    view! {
        <Provider value=ctx.clone()>
            <p class="text-sm text-muted-foreground mb-3">
                "Experimental. Walks the SD card via on-camera Lua "
                <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"os.listdir"</code>
                " / "
                <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"os.stat"</code>
                ". Directories load on expand and stay cached. JPEGs preview inline."
            </p>

            <div class="grid grid-cols-1 lg:grid-cols-5 gap-4">
                // Left: tree (2/5 of width)
                <div class="lg:col-span-2 border border-border rounded-md bg-card overflow-hidden">
                    <div class="px-3 py-2 border-b border-border bg-muted text-xs font-mono">
                        "A/  (SD card)"
                    </div>
                    <div class="max-h-[60vh] overflow-y-auto py-1">
                        <DirChildren path="A".to_string() depth=0/>
                    </div>
                </div>

                // Right: preview (3/5 of width)
                <div class="lg:col-span-3 border border-border rounded-md bg-card overflow-hidden">
                    <div class="px-3 py-2 border-b border-border bg-muted text-xs font-mono break-all min-h-[2rem]">
                        {move || selected.get().map(|(p, _)| p).unwrap_or_else(|| "(click a file to preview)".into())}
                    </div>
                    <div class="p-3 flex items-center justify-center min-h-[40vh] bg-black/5">
                        {move || match selected.get() {
                            None => view! {
                                <p class="text-sm text-muted-foreground">"No file selected"</p>
                            }.into_any(),
                            Some((p, url)) => {
                                if is_previewable_image(&p) {
                                    view! {
                                        <div class="flex flex-col items-center gap-2 max-w-full">
                                            <img src=url.clone() class="max-w-full max-h-[55vh] block" alt=p.clone()/>
                                            <a href=url target="_blank" class="text-xs text-muted-foreground hover:text-foreground underline">
                                                "open full size →"
                                            </a>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="flex flex-col items-center gap-2 text-center">
                                            <p class="text-sm">{format!("Not previewable: {p}")}</p>
                                            <a href=url target="_blank" class="text-xs text-muted-foreground hover:text-foreground underline">
                                                "download →"
                                            </a>
                                        </div>
                                    }.into_any()
                                }
                            }
                        }}
                    </div>
                </div>
            </div>
        </Provider>
    }
}

/// Renders the children of a single directory (lazy-fetches if needed).
/// Used both at the top of the tree and inside every collapsed folder.
#[component]
fn DirChildren(path: String, depth: usize) -> impl IntoView {
    let ctx = expect_context::<TreeContext>();
    let cache = ctx.cache;
    let path_for_render = path.clone();

    view! {
        {move || {
            let key = path_for_render.clone();
            cache.with(|c| match c.get(&key) {
                None => view! { <TreeMsg depth=depth text="(not loaded)".into()/> }.into_any(),
                Some(FetchResult::Loading) => view! { <TreeMsg depth=depth text="Loading…".into()/> }.into_any(),
                Some(FetchResult::Err(e)) => view! { <TreeMsg depth=depth text=format!("Error: {e}")/> }.into_any(),
                Some(FetchResult::Note(n)) => view! { <TreeMsg depth=depth text=n.clone()/> }.into_any(),
                Some(FetchResult::Ok(entries)) if entries.is_empty() => {
                    view! { <TreeMsg depth=depth text="(empty)".into()/> }.into_any()
                }
                Some(FetchResult::Ok(entries)) => {
                    let parent = key.clone();
                    let rows: Vec<_> = entries.clone().into_iter().map(|e| {
                        view! { <TreeNode entry=e parent_path=parent.clone() depth=depth/> }
                    }).collect();
                    view! { <div>{rows}</div> }.into_any()
                }
            })
        }}
    }
}

#[component]
fn TreeMsg(depth: usize, text: String) -> impl IntoView {
    let indent = format!("padding-left: {}px", 8 + depth * 14);
    view! {
        <p class="text-xs text-muted-foreground italic py-0.5" style=indent>{text}</p>
    }
}

/// One entry in the tree. Directories render as a Collapsible whose content
/// is another DirChildren (recursive); files render as a clickable button.
#[component]
fn TreeNode(entry: DirEntryDto, parent_path: String, depth: usize) -> impl IntoView {
    let ctx = expect_context::<TreeContext>();
    let full_path = if parent_path.is_empty() {
        entry.name.clone()
    } else {
        format!("{parent_path}/{}", entry.name)
    };
    let indent = format!("padding-left: {}px", 8 + depth * 14);
    let name = entry.name.clone();
    let size = entry.size;

    if entry.is_dir {
        let open = RwSignal::new(false);
        // On first expand, kick off the directory fetch (no-op afterwards).
        let path_for_effect = full_path.clone();
        let ctx_for_effect = ctx.clone();
        Effect::new(move |_| {
            if open.get() {
                ensure_loaded(ctx_for_effect.clone(), path_for_effect.clone());
            }
        });

        let child_path = full_path.clone();
        view! {
            <Collapsible open=open class="block">
                <CollapsibleTrigger class="w-full flex items-center gap-1 py-0.5 hover:bg-accent text-left cursor-pointer">
                    <span style=indent class="flex items-center gap-1.5 w-full">
                        <span class="inline-block w-3 text-xs text-muted-foreground tabular-nums select-none">
                            {move || if open.get() { "▾" } else { "▸" }}
                        </span>
                        <span class="text-sm">"📁"</span>
                        <span class="font-mono text-sm">{name}</span>
                    </span>
                </CollapsibleTrigger>
                <CollapsibleContent>
                    <DirChildren path=child_path depth=depth + 1/>
                </CollapsibleContent>
            </Collapsible>
        }.into_any()
    } else {
        let on_click = {
            let ctx = ctx.clone();
            let full = full_path.clone();
            move |_| {
                let url = format!("/api/file/{}?path={}", ctx.serial, urlencode(&full));
                ctx.selected.set(Some((full.clone(), url)));
            }
        };
        let icon = if is_previewable_image(&name) { "🖼" } else { "📄" };
        view! {
            <button
                class="w-full flex items-center gap-1 py-0.5 hover:bg-accent text-left cursor-pointer"
                on:click=on_click
            >
                <span style=indent class="flex items-center gap-1.5 w-full">
                    <span class="inline-block w-3 select-none"/>
                    <span class="text-sm">{icon}</span>
                    <span class="font-mono text-sm flex-1 truncate">{name}</span>
                    <span class="font-mono text-xs text-muted-foreground tabular-nums pr-2">{format_size(size)}</span>
                </span>
            </button>
        }.into_any()
    }
}

fn is_previewable_image(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".jpg") || n.ends_with(".jpeg") || n.ends_with(".png")
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn urlencode(s: &str) -> String {
    // Tiny ASCII-only encoder — sufficient for camera paths (which are
    // already restricted to alnum + a few path-safe chars on the server).
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn fetch_dir(serial: &str, path: &str) -> Result<ListDirResponseDto, String> {
    let url = format!("/api/files/{serial}?path={}", urlencode(path));
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    if !response.ok() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {} — {body}", response.status()));
    }
    response
        .json::<ListDirResponseDto>()
        .await
        .map_err(|e| format!("decode: {e}"))
}

fn format_exec_response(v: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(ms) = v.get("elapsed_ms").and_then(|m| m.as_u64()) {
        out.push(format!("elapsed_ms={ms}"));
    }
    if let Some(msgs) = v.get("messages").and_then(|m| m.as_array()) {
        if msgs.is_empty() {
            out.push("(no messages — script ended without returning a value)".into());
        }
        for m in msgs {
            let kind = m.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
            match kind {
                "return" => {
                    let v = m.get("value");
                    out.push(format!("← RET: {}", format_value(v)));
                }
                "error" => {
                    let cat = m.get("category").and_then(|c| c.as_str()).unwrap_or("?");
                    let text = m.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    out.push(format!("! ERR [{cat}]: {text}"));
                }
                "user" => {
                    let v = m.get("value");
                    out.push(format!("· USER: {}", format_value(v)));
                }
                k => out.push(format!("? unknown kind '{k}'")),
            }
        }
    } else if let Some(err) = v.get("error") {
        out.push(format!("server error: {err}"));
    }
    out
}

fn format_value(v: Option<&serde_json::Value>) -> String {
    let Some(v) = v else { return "?".into() };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("?");
    let val = v.get("value");
    match ty {
        "nil" => "nil".into(),
        "boolean" => val.and_then(|b| b.as_bool()).map(|b| b.to_string()).unwrap_or("?".into()),
        "integer" => val.and_then(|i| i.as_i64()).map(|i| i.to_string()).unwrap_or("?".into()),
        "string" => format!("{:?}", val.and_then(|s| s.as_str()).unwrap_or("")),
        "table" => format!("table {:?}", val.and_then(|s| s.as_str()).unwrap_or("")),
        "unsupported" => "<unsupported>".into(),
        _ => format!("?({ty})"),
    }
}

#[derive(Params, PartialEq, Clone, Debug)]
struct ViewportParams {
    serial: Option<String>,
}

#[component]
fn ViewportTab(serial: String) -> impl IntoView {
    // Tick every 500ms for cache-busted viewport polling
    let (tick, set_tick) = signal(0u64);
    set_interval(
        move || set_tick.update(|n| *n = n.wrapping_add(1)),
        std::time::Duration::from_millis(500),
    );

    let (pending, set_pending) = signal(false);
    let (status, set_status) = signal::<Option<String>>(None);

    let switch_to = {
        let serial = serial.clone();
        move |which: &'static str| {
            let serial = serial.clone();
            set_pending.set(true);
            set_status.set(None);
            wasm_bindgen_futures::spawn_local(async move {
                let url = format!("/api/mode/{which}/{serial}");
                let res = gloo_net::http::Request::post(&url)
                    .send()
                    .await
                    .map_err(|e| format!("network: {e}"))
                    .and_then(|r| if r.ok() { Ok(r) } else { Err(format!("HTTP {}", r.status())) });
                match res {
                    Ok(r) => match r.json::<serde_json::Value>().await {
                        Ok(v) => {
                            let mode = v.get("mode").and_then(|m| m.as_str()).unwrap_or("?").to_string();
                            set_status.set(Some(format!("now in {mode} mode")));
                        }
                        Err(e) => set_status.set(Some(format!("decode error: {e}"))),
                    },
                    Err(e) => set_status.set(Some(format!("error: {e}"))),
                }
                set_pending.set(false);
            });
        }
    };

    let on_record = {
        let switch_to = switch_to.clone();
        move |_| switch_to("record")
    };
    let on_play = move |_| switch_to("play");

    let serial_for_src = serial.clone();
    view! {
        <p class="text-sm text-muted-foreground mb-3">
            "Polls "
            <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"/api/viewport/<serial>"</code>
            " every 500 ms. If you see a placeholder, switch to record mode."
        </p>

        <div class="flex items-center gap-2 mb-4">
            <Button variant=ButtonVariant::Default on:click=on_record attr:disabled=move || pending.get()>
                {move || if pending.get() { "Switching…" } else { "Record mode (lens out)" }}
            </Button>
            <Button variant=ButtonVariant::Outline on:click=on_play attr:disabled=move || pending.get()>
                "Play mode (lens in)"
            </Button>
            <span class="text-sm text-muted-foreground ml-2">
                {move || status.get().unwrap_or_default()}
            </span>
        </div>

        <div class="bg-black rounded-lg p-2 flex items-center justify-center min-h-[280px]">
            <img
                class="max-w-full block"
                src=move || format!("/api/viewport/{}?t={}", serial_for_src, tick.get())
                alt="camera viewport"
            />
        </div>
    }
}

async fn fetch_info(serial: String) -> Result<InfoDto, String> {
    let response = gloo_net::http::Request::get(&format!("/api/info/{serial}"))
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.json::<InfoDto>().await.map_err(|e| format!("decode: {e}"))
}

async fn fetch_live_state(serial: String) -> Result<LiveStateDto, String> {
    let response = gloo_net::http::Request::get(&format!("/api/live_state/{serial}"))
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.json::<LiveStateDto>().await.map_err(|e| format!("decode: {e}"))
}

fn render_overview(i: InfoDto) -> impl IntoView {
    let chdk_line = match (i.chdk_version_major, i.chdk_version_minor) {
        (Some(maj), Some(min)) => format!("v{maj}.{min}"),
        _ => "(not detected)".into(),
    };
    let usb_man = i.usb_manufacturer.clone().unwrap_or_default();
    let usb_prod = i.usb_product.clone().unwrap_or_default();

    let title = format!("{} {}", usb_man, usb_prod).trim().to_string();

    view! {
        <div class="mb-6">
            <div class="text-xl font-semibold">{if title.is_empty() { i.ptp_model.clone() } else { title }}</div>
            <div class="text-sm text-muted-foreground mt-1">
                "firmware " {i.device_version.clone()}
                " · "
                {if i.chdk_advertised { "CHDK active " } else { "CHDK inactive " }}
                {chdk_line}
            </div>
        </div>

        <dl class="grid grid-cols-2 md:grid-cols-3 gap-x-8 gap-y-3 text-sm">
            <Field label="VID / PID" value=format!("0x{:04X} / 0x{:04X}", i.vendor_id, i.product_id) mono=true/>
            <Field label="bus / addr" value=format!("{} / {}", i.bus_number, i.device_address) mono=true/>
            <Field label="serial" value=i.serial.clone() mono=true/>
            <Field label="PTP model" value=i.ptp_model.clone() mono=false/>
            <Field label="PTP manufacturer" value=i.ptp_manufacturer.clone() mono=false/>
            <Field label="PTP std version" value=format!("0x{:04X}", i.ptp_standard_version) mono=true/>
            <Field label="vendor ext id" value=format!("0x{:08X}", i.vendor_extension_id) mono=true/>
            <Field label="vendor ext ver" value=format!("0x{:04X}", i.vendor_extension_version) mono=true/>
            <Field label="functional mode" value=format!("0x{:04X}", i.functional_mode) mono=true/>
        </dl>

        <div class="mt-6 grid grid-cols-2 md:grid-cols-4 gap-3 text-sm">
            <Stat label="operations" value=i.operations_supported.len()/>
            <Stat label="events" value=i.events_supported.len()/>
            <Stat label="device props" value=i.device_properties_supported.len()/>
            <Stat label="image formats" value=i.image_formats.len()/>
        </div>
    }
}

#[component]
fn Field(label: &'static str, value: String, mono: bool) -> impl IntoView {
    let value_cls = if mono { "font-mono text-xs break-all" } else { "" };
    view! {
        <div>
            <dt class="text-xs text-muted-foreground uppercase tracking-wide">{label}</dt>
            <dd class=value_cls>{if value.is_empty() { "(empty)".to_string() } else { value }}</dd>
        </div>
    }
}

#[component]
fn Stat(label: &'static str, value: usize) -> impl IntoView {
    view! {
        <div class="border border-border rounded-md px-3 py-2 bg-muted/40">
            <div class="text-xs text-muted-foreground">{label}</div>
            <div class="text-lg font-semibold tabular-nums">{value}</div>
        </div>
    }
}

fn render_inspector(i: InfoDto) -> impl IntoView {
    let ops_view = i.operations_supported.iter().map(|op| {
        let name = ptp_op_name(*op).unwrap_or("?");
        view! {
            <li class="font-mono text-xs flex gap-2">
                <span class="tabular-nums">{format!("0x{:04X}", op)}</span>
                <span class="text-muted-foreground">{name}</span>
            </li>
        }
    }).collect_view();
    let img_view = i.image_formats.iter().map(|fmt| {
        let name = image_format_name(*fmt).unwrap_or("?");
        view! {
            <li class="font-mono text-xs flex gap-2">
                <span class="tabular-nums">{format!("0x{:04X}", fmt)}</span>
                <span class="text-muted-foreground">{name}</span>
            </li>
        }
    }).collect_view();
    let events_view = i.events_supported.iter().map(|ev| {
        view! { <li class="font-mono text-xs">{format!("0x{:04X}", ev)}</li> }
    }).collect_view();
    let props_view = i.device_properties_supported.iter().map(|p| {
        view! { <li class="font-mono text-xs">{format!("0x{:04X}", p)}</li> }
    }).collect_view();

    let n_ops = i.operations_supported.len();
    let n_img = i.image_formats.len();
    let n_ev = i.events_supported.len();
    let n_props = i.device_properties_supported.len();

    view! {
        <div class="grid grid-cols-1 md:grid-cols-2 gap-x-8 gap-y-6">
            <section>
                <h3 class="text-sm font-semibold mb-2">
                    "Operations supported "
                    <span class="text-muted-foreground font-normal">"(" {n_ops} ")"</span>
                </h3>
                <ul class="max-h-80 overflow-y-auto space-y-1 border border-border rounded-md p-3 bg-card">
                    {ops_view}
                </ul>
            </section>

            <section>
                <h3 class="text-sm font-semibold mb-2">
                    "Image formats "
                    <span class="text-muted-foreground font-normal">"(" {n_img} ")"</span>
                </h3>
                <ul class="space-y-1 border border-border rounded-md p-3 bg-card">{img_view}</ul>
            </section>

            <section>
                <h3 class="text-sm font-semibold mb-2">
                    "Events supported "
                    <span class="text-muted-foreground font-normal">"(" {n_ev} ")"</span>
                </h3>
                <ul class="max-h-64 overflow-y-auto space-y-1 border border-border rounded-md p-3 bg-card">
                    {events_view}
                </ul>
            </section>

            <section>
                <h3 class="text-sm font-semibold mb-2">
                    "Device properties "
                    <span class="text-muted-foreground font-normal">"(" {n_props} ")"</span>
                </h3>
                <ul class="max-h-64 overflow-y-auto space-y-1 border border-border rounded-md p-3 bg-card">
                    {props_view}
                </ul>
            </section>
        </div>
    }
}

fn render_live_state(s: LiveStateDto) -> impl IntoView {
    let mode_str = match s.in_record {
        Some(true) => "record",
        Some(false) => "playback",
        None => "?",
    };
    let movie_str = match s.is_movie {
        Some(true) => "movie",
        Some(false) => "still",
        None => "?",
    };
    let opt_i = |o: Option<i32>| o.map(|v| v.to_string()).unwrap_or_else(|| "?".into());
    let opt_s = |o: Option<String>| o.unwrap_or_else(|| "?".into());
    let opt_b = |o: Option<bool>| match o { Some(true) => "yes", Some(false) => "no", None => "?" };

    view! {
        <div class="grid grid-cols-2 md:grid-cols-4 gap-x-6 gap-y-2 text-sm">
            <div><span class="text-muted-foreground">"mode:"</span> " " {mode_str} " / " {movie_str} " / " {opt_i(s.mode_code)}</div>
            <div><span class="text-muted-foreground">"zoom:"</span> " " {opt_i(s.zoom)}</div>
            <div><span class="text-muted-foreground">"exp_count:"</span> " " {opt_i(s.exp_count)}</div>
            <div><span class="text-muted-foreground">"vbatt:"</span> " " {opt_i(s.vbatt_mv)} " mV"</div>
            <div class="col-span-2"><span class="text-muted-foreground">"image_dir:"</span> " " <span class="font-mono text-xs">{opt_s(s.image_dir)}</span></div>
            <div><span class="text-muted-foreground">"free:"</span> " " {opt_i(s.free_kb)} " KB"</div>
            <div><span class="text-muted-foreground">"focus:"</span> " " {opt_i(s.focus)}</div>
            <div><span class="text-muted-foreground">"iso_mode:"</span> " " {opt_i(s.iso_mode)}</div>
            <div><span class="text-muted-foreground">"sv96:"</span> " " {opt_i(s.sv96)}</div>
            <div><span class="text-muted-foreground">"tv96:"</span> " " {opt_i(s.tv96)}</div>
            <div><span class="text-muted-foreground">"av96:"</span> " " {opt_i(s.av96)}</div>
            <div><span class="text-muted-foreground">"propset:"</span> " " {opt_i(s.propset)}</div>
            <div><span class="text-muted-foreground">"flash_mode:"</span> " " {opt_i(s.flash_mode)}</div>
            <div><span class="text-muted-foreground">"flash_ready:"</span> " " {opt_b(s.flash_ready)}</div>
            <div><span class="text-muted-foreground">"shooting:"</span> " " {opt_b(s.is_shooting)}</div>
        </div>
        <details class="mt-3 text-xs text-muted-foreground">
            <summary class="cursor-pointer">"raw response"</summary>
            <pre class="font-mono mt-1 break-all whitespace-pre-wrap">{s.raw}</pre>
        </details>
    }
}

async fn fetch_cameras() -> Result<Vec<CameraDto>, String> {
    let response = gloo_net::http::Request::get("/api/cameras")
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    response
        .json::<Vec<CameraDto>>()
        .await
        .map_err(|e| format!("decode: {e}"))
}

/// `set_interval` for Leptos 0.8 (replaces `set_interval_with_handle` from 0.6).
fn set_interval(f: impl Fn() + 'static, period: std::time::Duration) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    let cb = Closure::wrap(Box::new(f) as Box<dyn Fn()>);
    let window = web_sys::window().expect("window");
    let _ = window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            cb.as_ref().unchecked_ref(),
            period.as_millis() as i32,
        );
    cb.forget();
}

fn main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    leptos::mount::mount_to_body(App);
}
