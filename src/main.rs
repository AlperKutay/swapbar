mod assets;
mod cswap;
mod pricing;
mod settings;
mod settings_ui;
mod statusbar;
mod theme;
mod ui;
mod usage;

use gpui::{
    App, AppContext, Application, AsyncApp, Bounds, Entity, Global, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, point, px, size,
};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pricing::PriceBook;
use settings::{Prefs, TitleMode};
use usage::{Period, Rec, Summary};

pub const WINDOW_WIDTH: f32 = 420.0;
pub const WINDOW_HEIGHT: f32 = 780.0;
pub const APP_NAME: &str = "Swapbar";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct AppState {
    pub accounts: Option<cswap::AccountList>,
    pub accounts_error: Option<String>,
    pub accounts_loading: bool,
    pub switching: Option<u32>,
    pub records: Arc<Vec<Rec>>,
    pub prices: Arc<PriceBook>,
    pub summary_today: Option<Summary>,
    pub summary_month: Option<Summary>,
    pub usage_scanning: bool,
    pub usage_scanned_once: bool,
    pub prefs: Prefs,
    pub cswap_config: Option<cswap::ConfigList>,
    pub config_error: Option<String>,
    pub config_busy: bool,
}

impl AppState {
    fn new() -> Self {
        AppState {
            accounts: None,
            accounts_error: None,
            accounts_loading: false,
            switching: None,
            records: Arc::new(Vec::new()),
            prices: Arc::new(PriceBook::default()),
            summary_today: None,
            summary_month: None,
            usage_scanning: false,
            usage_scanned_once: false,
            prefs: Prefs::load(),
            cswap_config: None,
            config_error: None,
            config_busy: false,
        }
    }

    pub fn recompute_summary(&mut self) {
        self.summary_today = Some(usage::summarize(&self.records, Period::Today, &self.prices));
        self.summary_month = Some(usage::summarize(&self.records, Period::Month, &self.prices));
    }

    pub fn active_account(&self) -> Option<&cswap::Account> {
        self.accounts.as_ref()?.accounts.iter().find(|a| a.active)
    }
}

pub struct Globals {
    pub state: Entity<AppState>,
    pub status: Rc<statusbar::StatusItem>,
    pub window: Option<WindowHandle<ui::Root>>,
    pub settings_window: Option<WindowHandle<settings_ui::SettingsRoot>>,
}

impl Global for Globals {}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--scan") {
        let started = std::time::Instant::now();
        let prices = PriceBook::load_with_overlay();
        let recs = usage::collect_records();
        eprintln!("{} records in {:?}", recs.len(), started.elapsed());
        for p in [Period::Today, Period::Week, Period::Month] {
            let s = usage::summarize(&recs, p, &prices);
            println!(
                "{:>7}: {} | {} tokens | {} req | {} unpriced",
                p.label(),
                usage::format_usd(s.total.cost),
                usage::format_tokens(s.total.tokens()),
                s.total.requests,
                s.total.unpriced
            );
            for (m, t) in &s.by_model {
                println!(
                    "         {m:<28} {:>10} {:>8} {:>6}",
                    usage::format_usd(t.cost),
                    usage::format_tokens(t.tokens()),
                    t.requests
                );
            }
        }
        return;
    }
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            let state = cx.new(|_| AppState::new());
            theme::apply(state.read(cx).prefs.theme);

            let status = Rc::new(statusbar::StatusItem::new(
                " …",
                Box::new({
                    let cx = cx.to_async();
                    move || {
                        let cx = cx.clone();
                        let _ = cx.update(toggle_window);
                    }
                }),
            ));

            cx.set_global(Globals {
                state,
                status,
                window: None,
                settings_window: None,
            });

            refresh_accounts(cx);
            refresh_usage(cx);

            if std::env::var("SWAPBAR_SETTINGS").is_ok() {
                open_settings(cx);
            }
            if std::env::var("SWAPBAR_OPEN").is_ok() {
                // give the status item a layout pass before measuring it
                cx.spawn(async move |cx: &mut AsyncApp| {
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                    let _ = cx.update(|cx| {
                        if cx.global::<Globals>().window.is_none() {
                            toggle_window(cx)
                        }
                    });
                })
                .detach();
            }

            // periodic refreshes (intervals re-read from prefs each tick)
            cx.spawn(async move |cx: &mut AsyncApp| {
                loop {
                    let secs = cx
                        .update(|cx| {
                            cx.global::<Globals>()
                                .state
                                .read(cx)
                                .prefs
                                .accounts_refresh_secs
                        })
                        .unwrap_or(60);
                    cx.background_executor()
                        .timer(Duration::from_secs(secs.max(15)))
                        .await;
                    let _ = cx.update(refresh_accounts);
                }
            })
            .detach();
            cx.spawn(async move |cx: &mut AsyncApp| {
                loop {
                    let secs = cx
                        .update(|cx| {
                            cx.global::<Globals>()
                                .state
                                .read(cx)
                                .prefs
                                .usage_refresh_secs
                        })
                        .unwrap_or(120);
                    cx.background_executor()
                        .timer(Duration::from_secs(secs.max(30)))
                        .await;
                    let _ = cx.update(refresh_usage);
                }
            })
            .detach();
        });
}

pub fn refresh_accounts(cx: &mut App) {
    let state = cx.global::<Globals>().state.clone();
    let already = state.read(cx).accounts_loading;
    if already {
        return;
    }
    state.update(cx, |s, cx| {
        s.accounts_loading = true;
        cx.notify();
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = cx
            .background_executor()
            .spawn(async { cswap::list() })
            .await;
        let _ = state.update(cx, |s, cx| {
            s.accounts_loading = false;
            match result {
                Ok(list) => {
                    s.accounts = Some(list);
                    s.accounts_error = None;
                }
                Err(e) => s.accounts_error = Some(e),
            }
            cx.notify();
        });
        let _ = cx.update(update_title);
    })
    .detach();
}

pub fn switch_account(cx: &mut App, number: u32) {
    let state = cx.global::<Globals>().state.clone();
    if state.read(cx).switching.is_some() {
        return;
    }
    state.update(cx, |s, cx| {
        s.switching = Some(number);
        cx.notify();
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = cx
            .background_executor()
            .spawn(async move { cswap::switch(number) })
            .await;
        let _ = state.update(cx, |s, cx| {
            s.switching = None;
            if let Err(e) = result {
                s.accounts_error = Some(e);
            }
            cx.notify();
        });
        let _ = cx.update(refresh_accounts);
    })
    .detach();
}

pub fn refresh_usage(cx: &mut App) {
    let state = cx.global::<Globals>().state.clone();
    if state.read(cx).usage_scanning {
        return;
    }
    state.update(cx, |s, cx| {
        s.usage_scanning = true;
        cx.notify();
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let (prices, recs) = cx
            .background_executor()
            .spawn(async {
                let prices = PriceBook::load_with_overlay();
                let recs = usage::collect_records();
                (prices, recs)
            })
            .await;
        let _ = state.update(cx, |s, cx| {
            s.prices = Arc::new(prices);
            s.records = Arc::new(recs);
            s.usage_scanning = false;
            s.usage_scanned_once = true;
            s.recompute_summary();
            cx.notify();
        });
    })
    .detach();
}

pub fn update_title(cx: &mut App) {
    let g = cx.global::<Globals>();
    let state = g.state.read(cx);
    let mode = state.prefs.percent_mode;
    let title = match (state.prefs.title_mode, state.active_account()) {
        (TitleMode::Off, _) | (_, None) => String::new(),
        (tm, Some(acc)) => {
            let u = acc.usage.as_ref();
            let h5 = u
                .and_then(|u| u.five_hour.as_ref())
                .map(|w| format!("{:.0}%", mode.value(w.pct)));
            let d7 = u
                .and_then(|u| u.seven_day.as_ref())
                .map(|w| format!("{:.0}%", mode.value(w.pct)));
            match (tm, h5, d7) {
                (TitleMode::Both, Some(a), Some(b)) => format!(" {a}·{b}"),
                (_, Some(a), _) => format!(" {a}"),
                _ => String::new(),
            }
        }
    };
    g.status.set_title(&title);
}

/// Mutate prefs, persist, and refresh everything that depends on them.
pub fn update_prefs(cx: &mut App, f: impl FnOnce(&mut Prefs)) {
    let state = cx.global::<Globals>().state.clone();
    state.update(cx, |s, cx| {
        f(&mut s.prefs);
        s.prefs.save();
        cx.notify();
    });
    theme::apply(state.read(cx).prefs.theme);
    update_title(cx);
}

pub fn refresh_config(cx: &mut App) {
    let state = cx.global::<Globals>().state.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = cx
            .background_executor()
            .spawn(async { cswap::config_list() })
            .await;
        let _ = state.update(cx, |s, cx| {
            match result {
                Ok(list) => {
                    s.cswap_config = Some(list);
                    s.config_error = None;
                }
                Err(e) => s.config_error = Some(e),
            }
            cx.notify();
        });
    })
    .detach();
}

/// `value = None` resets the key to its cswap default.
pub fn set_cswap_config(cx: &mut App, key: String, value: Option<String>) {
    let state = cx.global::<Globals>().state.clone();
    state.update(cx, |s, cx| {
        s.config_busy = true;
        cx.notify();
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = cx
            .background_executor()
            .spawn(async move {
                match value {
                    Some(v) => cswap::config_set(&key, &v),
                    None => cswap::config_unset(&key),
                }
            })
            .await;
        let _ = state.update(cx, |s, cx| {
            s.config_busy = false;
            if let Err(e) = result {
                s.config_error = Some(e);
            }
            cx.notify();
        });
        let _ = cx.update(refresh_config);
    })
    .detach();
}

pub fn set_account_enabled(cx: &mut App, number: u32, enabled: bool) {
    let state = cx.global::<Globals>().state.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = cx
            .background_executor()
            .spawn(async move { cswap::set_enabled(number, enabled) })
            .await;
        let _ = state.update(cx, |s, cx| {
            if let Err(e) = result {
                s.accounts_error = Some(e);
            }
            cx.notify();
        });
        let _ = cx.update(refresh_accounts);
    })
    .detach();
}

pub fn open_settings(cx: &mut App) {
    if let Some(h) = cx.global::<Globals>().settings_window {
        if h.update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            cx.activate(true);
            return;
        }
        cx.global_mut::<Globals>().settings_window = None;
    }
    close_window(cx);
    refresh_config(cx);
    let bounds = Bounds::centered(None, size(px(760.0), px(540.0)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(format!("{APP_NAME} Settings").into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            window_min_size: Some(size(px(640.0), px(440.0))),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| settings_ui::SettingsRoot::new(window, cx)),
    );
    match handle {
        Ok(h) => {
            cx.global_mut::<Globals>().settings_window = Some(h);
            cx.activate(true);
        }
        Err(e) => eprintln!("ayarlar penceresi açılamadı: {e}"),
    }
}

pub fn close_window(cx: &mut App) {
    if std::env::var("SWAPBAR_DEBUG").is_ok() {
        eprintln!("close_window");
    }
    let handle = cx.global_mut::<Globals>().window.take();
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    cx.global::<Globals>().status.set_highlighted(false);
}

pub fn toggle_window(cx: &mut App) {
    if cx.global::<Globals>().window.is_some() {
        close_window(cx);
        return;
    }
    let status = cx.global::<Globals>().status.clone();
    let geo = status.geometry();
    if std::env::var("SWAPBAR_DEBUG").is_ok() {
        eprintln!(
            "geometry: {geo:?}; displays: {:?}",
            cx.displays()
                .iter()
                .map(|d| (d.id(), d.bounds()))
                .collect::<Vec<_>>()
        );
    }

    let height = geo
        .map(|g| (g.screen_height - g.bottom_y - 24.0).min(WINDOW_HEIGHT as f64) as f32)
        .unwrap_or(WINDOW_HEIGHT);
    let (display_id, origin) = match geo {
        Some(g) => {
            let display = cx
                .displays()
                .into_iter()
                .find(|d| format!("{:?}", d.id()) == format!("DisplayId({})", g.screen_display_id));
            let x = (g.x + g.width / 2.0 - WINDOW_WIDTH as f64 / 2.0)
                .clamp(8.0, (g.screen_width - WINDOW_WIDTH as f64 - 8.0).max(8.0));
            (
                display.map(|d| d.id()),
                point(px(x as f32), px(g.bottom_y as f32 + 4.0)),
            )
        }
        None => (None, point(px(200.0), px(30.0))),
    };

    let bounds = Bounds {
        origin,
        size: size(px(WINDOW_WIDTH), px(height)),
    };
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            titlebar: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        |window, cx| cx.new(|cx| ui::Root::new(window, cx)),
    );
    match handle {
        Ok(h) => {
            cx.global_mut::<Globals>().window = Some(h);
            status.set_highlighted(true);
            cx.activate(true);
            refresh_accounts(cx);
        }
        Err(e) => eprintln!("pencere açılamadı: {e}"),
    }
}
