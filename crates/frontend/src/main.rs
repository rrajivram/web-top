use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::MetricsSnapshot;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CloseEvent, MessageEvent, WebSocket as WsNative};

// ── WebSocket helpers ─────────────────────────────────────────────────────────

fn connect_ws(on_message: impl Fn(MetricsSnapshot) + 'static) {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let host = location.host().unwrap_or_default();
    let proto = if location.protocol().unwrap_or_default() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let url = format!("{proto}://{host}/ws");

    let ws = WsNative::new(&url).expect("WebSocket connect");

    let on_msg = Closure::<dyn Fn(MessageEvent)>::new(move |e: MessageEvent| {
        if let Some(txt) = e.data().as_string() {
            if let Ok(snap) = serde_json::from_str::<MetricsSnapshot>(&txt) {
                on_message(snap);
            }
        }
    });
    ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
    on_msg.forget();

    // reconnect on close
    let on_close = Closure::<dyn Fn(CloseEvent)>::new(move |_e: CloseEvent| {
        // schedule a reconnect attempt after 2s
        let window = web_sys::window().unwrap();
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            &js_sys::Function::new_no_args("location.reload()"),
            2000,
        );
    });
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();
}

// ── Formatting helpers ────────────────────────────────────────────────────────

fn fmt_bytes(b: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    let b = b as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

fn usage_color(pct: f32) -> &'static str {
    if pct >= 80.0 {
        "bg-red-500"
    } else if pct >= 50.0 {
        "bg-yellow-400"
    } else {
        "bg-green-500"
    }
}

// ── Sub-components ────────────────────────────────────────────────────────────

#[component]
fn UsageBar(#[prop(into)] label: String, usage: f32, #[prop(default = "")] unit: &'static str) -> impl IntoView {
    let pct = usage.clamp(0.0, 100.0);
    let color = usage_color(pct);
    view! {
        <div class="flex items-center gap-2 text-xs">
            <span class="w-20 truncate text-gray-400">{label}</span>
            <div class="flex-1 bg-gray-800 rounded h-3 overflow-hidden">
                <div
                    class=format!("h-full rounded transition-all duration-300 {color}")
                    style=format!("width: {pct:.1}%")
                />
            </div>
            <span class="w-14 text-right text-gray-300">
                {format!("{pct:.1}{unit}")}
            </span>
        </div>
    }
}

#[component]
fn MemBar(label: &'static str, bytes: u64, total: u64) -> impl IntoView {
    let pct = if total > 0 { (bytes as f64 / total as f64 * 100.0) as f32 } else { 0.0 };
    let color = usage_color(pct);
    view! {
        <div class="flex items-center gap-2 text-xs">
            <span class="w-20 text-gray-400">{label}</span>
            <div class="flex-1 bg-gray-800 rounded h-3 overflow-hidden">
                <div
                    class=format!("h-full rounded transition-all duration-300 {color}")
                    style=format!("width: {pct:.1}%")
                />
            </div>
            <span class="w-20 text-right text-gray-300">{fmt_bytes(bytes)}</span>
        </div>
    }
}

// ── Overview tab ──────────────────────────────────────────────────────────────

#[component]
fn OverviewTab(snap: ReadSignal<Option<MetricsSnapshot>>) -> impl IntoView {
    view! {
        <div class="space-y-6">
            {move || snap.get().map(|s| {
                let mem = &s.memory;
                let total = mem.total;
                view! {
                    // CPU grid
                    <section>
                        <h2 class="text-sm font-semibold text-gray-400 mb-2 uppercase tracking-wider">
                            "CPU — " {s.cpu_cores.len()} " cores"
                        </h2>
                        <div class="grid grid-cols-2 gap-1">
                            {s.cpu_cores.iter().map(|c| {
                                let label = format!("Core {}", c.id);
                                let usage = c.usage;
                                view! { <UsageBar label=label usage=usage unit="%" /> }
                            }).collect_view()}
                        </div>
                    </section>

                    // GPU section
                    <section>
                        <h2 class="text-sm font-semibold text-gray-400 mb-2 uppercase tracking-wider">
                            "GPU"
                        </h2>
                        <div class="space-y-1">
                            {s.gpu_cores.iter().map(|g| {
                                let gname = g.name.clone();
                                let usage = g.usage;
                                if usage < 0.0 {
                                    view! {
                                        <div class="flex items-center gap-2 text-xs">
                                            <span class="w-20 text-gray-400">{gname}</span>
                                            <span class="text-gray-600 italic">"unavailable (needs sudo)"</span>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <UsageBar label=gname usage=usage unit="%" /> }.into_any()
                                }
                            }).collect_view()}
                        </div>
                    </section>

                    // Memory section
                    <section>
                        <h2 class="text-sm font-semibold text-gray-400 mb-2 uppercase tracking-wider">
                            "Memory — " {fmt_bytes(total)} " total"
                        </h2>
                        <div class="space-y-1">
                            <MemBar label="Used" bytes=mem.used total=total />
                            <MemBar label="Wired" bytes=mem.wired total=total />
                            <MemBar label="Cached" bytes=mem.cached total=total />
                            <MemBar label="Available" bytes=mem.available total=total />
                            <MemBar label="Swap used" bytes=mem.swap_used total=mem.swap_total.max(1) />
                        </div>
                    </section>
                }
            })}
        </div>
    }
}

// ── Process table ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum SortKey {
    Cpu,
    Memory,
}

#[component]
fn ProcessTable(
    snap: ReadSignal<Option<MetricsSnapshot>>,
    sort: SortKey,
    #[prop(default = false)] gpu_first: bool,
) -> impl IntoView {
    let selected = RwSignal::new(0usize);

    // keyboard handler
    let handle_key = move |ev: web_sys::KeyboardEvent| {
        let snap_len = snap.get().map(|s| s.processes.len()).unwrap_or(0);
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                selected.update(|i| *i = (*i + 1).min(snap_len.saturating_sub(1)));
            }
            "ArrowUp" => {
                ev.prevent_default();
                selected.update(|i| *i = i.saturating_sub(1));
            }
            "k" | "K" => {
                if let Some(snap) = snap.get() {
                    let mut procs = snap.processes.clone();
                    sort_procs(&mut procs, sort);
                    if let Some(p) = procs.get(selected.get()) {
                        send_kill(p.pid);
                    }
                }
            }
            _ => {}
        }
    };

    view! {
        <div
            tabindex="0"
            class="outline-none"
            on:keydown=handle_key
        >
            // Header — extra GPU column when gpu_first=true
            {if gpu_first {
                view! {
                    <div class="grid grid-cols-[4rem_1fr_2rem_6rem_6rem] gap-2 text-xs text-gray-500 uppercase tracking-wider pb-1 border-b border-gray-800 sticky top-0 bg-gray-950">
                        <span>"PID"</span><span>"Command"</span><span></span>
                        <span class="text-right">"Mem"</span>
                        <span class="text-right">"CPU%"</span>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="grid grid-cols-[4rem_1fr_6rem_6rem] gap-2 text-xs text-gray-500 uppercase tracking-wider pb-1 border-b border-gray-800 sticky top-0 bg-gray-950">
                        <span>"PID"</span><span>"Command"</span>
                        <span class="text-right">
                            {match sort { SortKey::Cpu => "CPU%", SortKey::Memory => "Mem" }}
                        </span>
                        <span class="text-right">
                            {match sort { SortKey::Cpu => "Mem", SortKey::Memory => "CPU%" }}
                        </span>
                    </div>
                }.into_any()
            }}
            // Rows
            {move || snap.get().map(|s| {
                let mut procs = s.processes.clone();
                if gpu_first {
                    // GPU-active processes first, then by memory within each group
                    procs.sort_by(|a, b| {
                        b.gpu_active.cmp(&a.gpu_active)
                            .then(b.memory_bytes.cmp(&a.memory_bytes))
                    });
                } else {
                    sort_procs(&mut procs, sort);
                }
                let sel = selected.get();
                procs.iter().enumerate().map(|(idx, p)| {
                    let is_sel = idx == sel;
                    let name = p.name.clone();
                    let pid = p.pid;
                    let gpu_on = p.gpu_active;
                    let primary = match sort {
                        SortKey::Cpu => format!("{:.1}%", p.cpu_usage),
                        SortKey::Memory => fmt_bytes(p.memory_bytes),
                    };
                    let secondary = match sort {
                        SortKey::Cpu => fmt_bytes(p.memory_bytes),
                        SortKey::Memory => format!("{:.1}%", p.cpu_usage),
                    };
                    let mem_str = fmt_bytes(p.memory_bytes);
                    let cpu_str = format!("{:.1}%", p.cpu_usage);

                    if gpu_first {
                        let row_class = if is_sel {
                            "grid grid-cols-[4rem_1fr_2rem_6rem_6rem] gap-2 text-xs py-0.5 px-1 rounded bg-blue-900 cursor-pointer"
                        } else if gpu_on {
                            "grid grid-cols-[4rem_1fr_2rem_6rem_6rem] gap-2 text-xs py-0.5 px-1 rounded bg-purple-950 hover:bg-purple-900 cursor-pointer"
                        } else {
                            "grid grid-cols-[4rem_1fr_2rem_6rem_6rem] gap-2 text-xs py-0.5 px-1 rounded hover:bg-gray-900 cursor-pointer"
                        };
                        view! {
                            <div class=row_class on:click=move |_| selected.set(idx)>
                                <span class="text-gray-500">{pid}</span>
                                <span class="min-w-0 truncate text-gray-100">{name}</span>
                                <span class="text-center text-purple-400">
                                    {if gpu_on { "G" } else { "" }}
                                </span>
                                <span class="text-right text-green-400">{mem_str}</span>
                                <span class="text-right text-gray-400">{cpu_str}</span>
                            </div>
                        }.into_any()
                    } else {
                        let row_class = if is_sel {
                            "grid grid-cols-[4rem_1fr_6rem_6rem] gap-2 text-xs py-0.5 px-1 rounded bg-blue-900 cursor-pointer"
                        } else {
                            "grid grid-cols-[4rem_1fr_6rem_6rem] gap-2 text-xs py-0.5 px-1 rounded hover:bg-gray-900 cursor-pointer"
                        };
                        view! {
                            <div class=row_class on:click=move |_| selected.set(idx)>
                                <span class="text-gray-500">{pid}</span>
                                <span class="min-w-0 truncate text-gray-100">{name}</span>
                                <span class="text-right text-green-400">{primary}</span>
                                <span class="text-right text-gray-400">{secondary}</span>
                            </div>
                        }.into_any()
                    }
                }).collect_view()
            })}
            <p class="text-xs text-gray-600 mt-3">
                "↑↓ navigate  •  k = kill selected process"
            </p>
        </div>
    }
}

fn sort_procs(procs: &mut Vec<shared::ProcessInfo>, key: SortKey) {
    match key {
        SortKey::Cpu => procs.sort_by(|a, b| {
            b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortKey::Memory => procs.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes)),
    }
}

// ── GPU tab ───────────────────────────────────────────────────────────────────

#[component]
fn GpuTab(snap: ReadSignal<Option<MetricsSnapshot>>) -> impl IntoView {
    view! {
        <div class="space-y-6">
            {move || snap.get().map(|s| {
                view! {
                    // GPU hardware utilization
                    <section>
                        <h2 class="text-sm font-semibold text-gray-400 mb-2 uppercase tracking-wider">
                            "GPU Hardware"
                        </h2>
                        <div class="space-y-1">
                            {s.gpu_cores.iter().map(|g| {
                                let gname = g.name.clone();
                                let usage = g.usage;
                                if usage < 0.0 {
                                    view! {
                                        <div class="flex items-center gap-2 text-xs">
                                            <span class="w-20 text-gray-400">{gname}</span>
                                            <span class="text-gray-600 italic">"unavailable (needs sudo)"</span>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <UsageBar label=gname usage=usage unit="%" /> }.into_any()
                                }
                            }).collect_view()}
                        </div>
                    </section>

                    // Process list: GPU-active first, then by memory
                    <section>
                        {
                            let gpu_count = s.processes.iter().filter(|p| p.gpu_active).count();
                            view! {
                                <h2 class="text-sm font-semibold text-gray-400 mb-2 uppercase tracking-wider">
                                    "Processes — " {gpu_count} " with active GPU context"
                                    <span class="normal-case text-gray-600 ml-2 font-normal">
                                        "(sorted GPU-first, then by memory)"
                                    </span>
                                </h2>
                            }
                        }
                        <ProcessTable snap=snap sort=SortKey::Memory gpu_first=true />
                    </section>
                }
            })}
        </div>
    }
}

fn send_kill(pid: u32) {
    spawn_local(async move {
        let window = web_sys::window().unwrap();
        let base = window.location().origin().unwrap_or_default();
        let url = format!("{base}/api/process/{pid}");
        if web_sys::window().unwrap().confirm_with_message(
            &format!("Kill process {pid}?")
        ).unwrap_or(false) {
            let opts = web_sys::RequestInit::new();
            opts.set_method("DELETE");
            let request = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
            let _ = JsFuture::from(
                web_sys::window().unwrap().fetch_with_request(&request)
            ).await;
        }
    });
}

// ── Root app ──────────────────────────────────────────────────────────────────

#[component]
fn App() -> impl IntoView {
    let snap: RwSignal<Option<MetricsSnapshot>> = RwSignal::new(None);
    let (read_snap, write_snap) = snap.split();

    // Connect WebSocket and update signal on each message
    connect_ws(move |s| write_snap.set(Some(s)));

    let tab: RwSignal<&'static str> = RwSignal::new("overview");

    view! {
        <div class="min-h-screen bg-gray-950 text-gray-100 p-4 max-w-4xl mx-auto">
            // Header
            <div class="flex items-center justify-between mb-4">
                <h1 class="text-lg font-bold text-white tracking-tight">"web-top"</h1>
                {move || read_snap.get().map(|s| view! {
                    <span class="text-xs text-gray-500">
                        {s.cpu_cores.len()} " cores  •  "
                        {fmt_bytes(s.memory.total)} " RAM"
                    </span>
                })}
            </div>

            // Tab bar
            <div class="flex gap-1 mb-4 bg-gray-900 rounded p-1">
                {["overview", "cpu", "gpu", "memory"].map(|t| {
                    let is_active = move || tab.get() == t;
                    view! {
                        <button
                            class=move || if is_active() {
                                "flex-1 py-1.5 text-xs font-medium rounded bg-gray-700 text-white"
                            } else {
                                "flex-1 py-1.5 text-xs font-medium rounded text-gray-400 hover:text-white hover:bg-gray-800"
                            }
                            on:click=move |_| tab.set(t)
                        >
                            {t.to_uppercase()}
                        </button>
                    }
                }).collect_view()}
            </div>

            // Tab content
            <div class="space-y-4">
                {move || match tab.get() {
                    "overview" => view! { <div><OverviewTab snap=read_snap /></div> }.into_any(),
                    "cpu"      => view! { <div><ProcessTable snap=read_snap sort=SortKey::Cpu /></div> }.into_any(),
                    "gpu"      => view! { <div><GpuTab snap=read_snap /></div> }.into_any(),
                    "memory"   => view! { <div><ProcessTable snap=read_snap sort=SortKey::Memory /></div> }.into_any(),
                    _          => view! { <div></div> }.into_any(),
                }}
            </div>
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
