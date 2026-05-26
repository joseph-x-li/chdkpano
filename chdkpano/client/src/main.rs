//! chdkpano-client: Leptos 0.8 + rust-ui components. 100% Rust.

mod ui_button;
mod ui_card;

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_params;
use leptos_router::params::Params;
use leptos_router::path;
use serde::{Deserialize, Serialize};

use ui_button::{Button, ButtonVariant};
use ui_card::{Card, CardContent, CardFooter, CardHeader, CardTitle};

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
                </nav>
            </header>
            <main class="max-w-6xl mx-auto px-6 py-6">
                <Routes fallback=|| "Not found">
                    <Route path=path!("/") view=CameraListPage/>
                    <Route path=path!("/viewport/:serial") view=ViewportPage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn CameraListPage() -> impl IntoView {
    let cameras = LocalResource::new(fetch_cameras);

    view! {
        <h2 class="text-base font-semibold mb-1">"Connected cameras"</h2>
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
                        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                            <For
                                each=move || cams.clone()
                                key=|c| c.serial.clone()
                                children=move |cam| view! { <CameraCard cam=cam/> }
                            />
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn CameraCard(cam: CameraDto) -> impl IntoView {
    let serial = cam.serial.clone();
    let serial_short: String = serial.chars().take(8).collect();
    let viewport_href = format!("/viewport/{serial}");
    let title = format!(
        "{} {}",
        cam.manufacturer.clone().unwrap_or_default(),
        cam.product.clone().unwrap_or_default(),
    );

    view! {
        <Card>
            <CardHeader>
                <CardTitle>{title}</CardTitle>
            </CardHeader>
            <CardContent>
                <div class="font-mono text-xs text-muted-foreground space-y-1">
                    <div>"serial: " {serial_short} "…"</div>
                    <div>{format!("VID 0x{:04X}  PID 0x{:04X}", cam.vendor_id, cam.product_id)}</div>
                    <div>{format!("bus {} addr {}", cam.bus_number, cam.device_address)}</div>
                </div>
            </CardContent>
            <CardFooter>
                <A href=viewport_href>
                    <Button variant=ButtonVariant::Outline>"Open viewport →"</Button>
                </A>
            </CardFooter>
        </Card>
    }
}

#[derive(Params, PartialEq, Clone, Debug)]
struct ViewportParams {
    serial: Option<String>,
}

#[component]
fn ViewportPage() -> impl IntoView {
    let params = use_params::<ViewportParams>();
    let serial = move || {
        params
            .with(|p| p.as_ref().ok().and_then(|p| p.serial.clone()))
            .unwrap_or_default()
    };

    // Tick every 500ms for cache-busted viewport polling
    let (tick, set_tick) = signal(0u64);
    set_interval(
        move || set_tick.update(|n| *n = n.wrapping_add(1)),
        std::time::Duration::from_millis(500),
    );

    // Mode-switch state
    let (pending, set_pending) = signal(false);
    let (status, set_status) = signal::<Option<String>>(None);

    let switch_to = move |which: &'static str| {
        let serial = serial();
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
    };

    let on_record = move |_| switch_to("record");
    let on_play = move |_| switch_to("play");

    view! {
        <A href="/" attr:class="text-xs text-muted-foreground hover:text-foreground">"← back to cameras"</A>
        <h2 class="text-base font-semibold mt-2 mb-1">
            {move || format!("Viewport: {}…", serial().chars().take(12).collect::<String>())}
        </h2>
        <p class="text-sm text-muted-foreground mb-4">
            "Polls "
            <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">"/api/viewport/<serial>"</code>
            " every 500 ms."
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
                src=move || format!("/api/viewport/{}?t={}", serial(), tick.get())
                alt="camera viewport"
            />
        </div>
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
