//! chdkpano-client: Leptos 0.8 + rust-ui components. 100% Rust.

mod ui_button;
mod ui_collapsible;

use leptos::prelude::*;
use leptos::ev;
use leptos::context::Provider;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_params;
use leptos_router::params::Params;
use leptos_router::path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use ui_button::{Button, ButtonVariant};
use ui_collapsible::{Collapsible, CollapsibleContent, CollapsibleTrigger};

fn event_target_value(ev: &ev::Event) -> String {
    use wasm_bindgen::JsCast;
    let target = ev.target().expect("event target");
    let input: web_sys::HtmlTextAreaElement = target
        .dyn_into()
        .expect("input/textarea");
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

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <header class="bg-card border-b border-border px-6 py-4 flex items-baseline gap-6">
                <h1 class="text-lg font-semibold tracking-tight">"chdkpano"</h1>
                <nav class="flex gap-4">
                    <A href="/" attr:class="text-sm text-muted-foreground hover:text-foreground">"Cameras"</A>
                    <A href="/api-docs" attr:class="text-sm text-muted-foreground hover:text-foreground">"API"</A>
                </nav>
            </header>
            <main class="max-w-6xl mx-auto px-6 py-6">
                <Routes fallback=|| "Not found">
                    <Route path=path!("/") view=CameraListPage/>
                    <Route path=path!("/camera/:serial") view=CameraDetailPage/>
                    <Route path=path!("/api-docs") view=ApiDocsPage/>
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

// ─── API docs page ────────────────────────────────────────────────────
#[component]
fn ApiDocsPage() -> impl IntoView {
    view! {
        <h2 class="text-base font-semibold mb-1">"HTTP API"</h2>
        <p class="text-sm text-muted-foreground mb-6">
            "All responses are JSON unless the content-type says otherwise. "
            ":serial is the camera's USB serial (from "
            <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"GET /api/cameras"</code>
            ")."
        </p>

        <div class="space-y-6">
            <Endpoint
                method="GET" path="/api/cameras"
                summary="List Canon devices currently enumerated over USB. The serial returned here is the key for every other endpoint."
                params=""
                response=r#"[
  {
    "serial": "EB1A78FBC...",
    "vendor_id": 1193, "product_id": 12881,
    "bus_number": 20, "device_address": 5,
    "manufacturer": "Canon Inc.",
    "product": "Canon PowerShot ELPH 180"
  }
]"#
            />

            <Endpoint
                method="GET" path="/api/info/:serial"
                summary="Full PTP DeviceInfo plus the CHDK protocol version (if CHDK is active). Cached implicitly by the server's per-camera session pool — first call opens the PTP session (~50–100ms)."
                params=""
                response=r#"{
  "serial": "...", "vendor_id": 1193, "product_id": 12881,
  "ptp_standard_version": 256,
  "vendor_extension_id": 11, "vendor_extension_version": 256,
  "ptp_manufacturer": "Canon Inc.", "ptp_model": "Canon PowerShot ELPH 180",
  "device_version": "1-15.0.1.0",
  "operations_supported": [4097, 4098, 4099, ..., 39321],
  "events_supported": [...], "device_properties_supported": [...],
  "image_formats": [12289, 12294],
  "chdk_advertised": true,
  "chdk_version_major": 2, "chdk_version_minor": 6
}"#
            />

            <Endpoint
                method="GET" path="/api/live_state/:serial"
                summary="Runtime state collected via a single Lua script: mode/zoom/exposure/battery/storage/focus/flash/etc. One PTP round-trip per call."
                params=""
                response=r#"{
  "in_record": true, "is_movie": false, "mode_code": 257,
  "zoom": 0, "exp_count": 4231, "vbatt_mv": 4096,
  "image_dir": "A/DCIM/100CANON", "free_kb": 7654321,
  "iso_mode": 0, "sv96": 0, "tv96": 0, "av96": 0,
  "focus": 0, "propset": 5,
  "flash_mode": 0, "flash_ready": false,
  "is_shooting": false,
  "raw": "true|false|257|0|4231|..."
}"#
            />

            <Endpoint
                method="GET" path="/api/viewport/:serial"
                summary="One live-view frame. Y411 (UYVYYY) decoded to RGB, JPEG-encoded at q=80. Content-type is image/jpeg. If the camera isn't producing a viewport (e.g. playback mode), returns an SVG placeholder explaining why — content-type image/svg+xml. Either way the response is binary, not JSON."
                params="cache-control: no-store (clients should cache-bust with ?t=<tick>)"
                response="(binary: JPEG ~30–80 KB at 640×480, or SVG placeholder)"
            />

            <Endpoint
                method="POST" path="/api/exec/:serial"
                summary="Run arbitrary Lua on the camera via ExecuteScript. Times out after 20 s. Every message returned by the script (return value, errors, user prints) is included."
                params=r#"request body: { "source": "return get_exp_count()" }"#
                response=r#"{
  "elapsed_ms": 47,
  "messages": [
    { "kind": "return", "value": { "type": "integer", "value": 4231 } }
  ]
}

// kind ∈ {return, error, user}
// value.type ∈ {nil, boolean, integer, string, table, unsupported}"#
            />

            <Endpoint
                method="POST" path="/api/mode/record/:serial"
                summary="Switch the camera into record mode (lens extends, sensor pipeline runs). Also forces LCD on so the viewport endpoint can produce frames. Idempotent — no-op if already in record."
                params=""
                response=r#"{ "mode": "record" }"#
            />

            <Endpoint
                method="POST" path="/api/mode/play/:serial"
                summary="Switch into playback mode (lens retracts, gallery on display). Viewport will return a placeholder until you switch back to record."
                params=""
                response=r#"{ "mode": "play" }"#
            />

            <Endpoint
                method="GET" path="/api/files/:serial?path=A/DCIM/100CANON"
                summary="Directory listing via on-camera Lua (os.listdir + os.stat). Directories are sorted first. If the camera build lacks os.listdir, returns entries:[] with a note explaining."
                params="?path= — camera-side path (default A). Restricted to [A-Za-z0-9/._\\-+ ], max 255 chars."
                response=r#"{
  "path": "A/DCIM/100CANON",
  "entries": [
    { "name": "100___01", "is_dir": true, "size": 0 },
    { "name": "IMG_0001.JPG", "is_dir": false, "size": 5783401 }
  ],
  "note": null
}"#
            />

            <Endpoint
                method="GET" path="/api/file/:serial?path=A/DCIM/100CANON/IMG_0001.JPG"
                summary="Stream a file from the camera. Uses chdkptp::PtpSession::download_file (~4.75 MB/s end-to-end). Content-type is guessed from the extension (image/jpeg for .jpg, image/png for .png, image/x-canon-raw for .cr2/.crw/.dng, video/mp4 for .mov/.mp4, otherwise application/octet-stream)."
                params="?path= — camera-side absolute path. Same restrictions as /api/files."
                response="(binary, content-type per file extension; cache-control: private, max-age=60)"
            />
        </div>

        <h3 class="text-sm font-semibold mt-10 mb-2">"Errors"</h3>
        <p class="text-sm text-muted-foreground mb-3">
            "All endpoints return "
            <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"500"</code>
            " + "
            <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"{\"error\":\"...\"}"</code>
            " on failure (camera not found, USB claim race, Lua timeout, script error, etc.). "
            "The server invalidates its cached PTP session on most failures, so the next call re-opens cleanly."
        </p>
    }
}

#[component]
fn Endpoint(
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    params: &'static str,
    response: &'static str,
) -> impl IntoView {
    let method_cls = match method {
        "GET" => "bg-success/15 text-success border-success/30",
        "POST" => "bg-warning/15 text-warning border-warning/30",
        _ => "bg-muted text-muted-foreground border-border",
    };
    view! {
        <section class="border-b border-border pb-5">
            <div class="flex items-center gap-3 mb-1.5">
                <span class=format!("font-mono text-xs font-semibold px-2 py-0.5 rounded border {method_cls}")>
                    {method}
                </span>
                <code class="font-mono text-sm">{path}</code>
            </div>
            <p class="text-sm text-foreground mb-2">{summary}</p>
            {(!params.is_empty()).then(|| view! {
                <p class="text-xs text-muted-foreground font-mono mb-2">{params}</p>
            })}
            <pre class="bg-card border border-border rounded-md p-3 font-mono text-xs whitespace-pre-wrap break-all max-h-72 overflow-y-auto">{response}</pre>
        </section>
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
