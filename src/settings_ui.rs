//! Settings window: sidebar + grouped rows, in the style of macOS System Settings.

use gpui::{
    App, Context, Entity, FontWeight, Hsla, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window, div, prelude::*, px, rgb,
};

use crate::assets::icon;
use crate::cswap::ConfigList;
use crate::settings::{PercentMode, Prefs, Theme, TitleMode};
use crate::{APP_NAME, APP_VERSION, AppState, Globals, hotkey, notify};

const UI_FONT: &str = ".AppleSystemUIFont";

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Colour palette; `t()` picks the light or dark one for the current appearance.
struct T {
    sidebar: u32,
    content: u32,
    group: u32,
    hairline: u32,
    hover: u32,
    selected: u32,
    text: u32,
    secondary: u32,
    tertiary: u32,
    accent: u32,
    switch_on: u32,
    switch_off: u32,
    danger: u32,
}

const LIGHT: T = T {
    sidebar: 0xf2f1ee,
    content: 0xfbfbfa,
    group: 0xffffff,
    hairline: 0xe6e5e1,
    hover: 0xe8e7e3,
    selected: 0xdcdbd6,
    text: 0x1d1d1f,
    secondary: 0x6e6e73,
    tertiary: 0xa1a1a6,
    accent: 0xff4101,
    switch_on: 0x3478f6,
    switch_off: 0xd1d0cc,
    danger: 0xd93025,
};

const DARK: T = T {
    sidebar: 0x1c1c1b,
    content: 0x232322,
    group: 0x2c2c2a,
    hairline: 0x3a3a38,
    hover: 0x353533,
    selected: 0x41413e,
    text: 0xf2f2f0,
    secondary: 0xa1a1a6,
    tertiary: 0x707074,
    accent: 0xff5a1f,
    switch_on: 0x0a84ff,
    switch_off: 0x4a4a48,
    danger: 0xff5f52,
};

fn t() -> &'static T {
    if crate::theme::is_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    General,
    MenuBar,
    Notifications,
    Accounts,
    AutoSwitch,
    About,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::General,
        Page::MenuBar,
        Page::Notifications,
        Page::Accounts,
        Page::AutoSwitch,
        Page::About,
    ];
    fn label(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::MenuBar => "Menu Bar",
            Page::Notifications => "Notifications",
            Page::Accounts => "Accounts",
            Page::AutoSwitch => "Auto-switch",
            Page::About => "About",
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Page::General => "settings",
            Page::MenuBar => "laptop",
            Page::Notifications => "bell",
            Page::Accounts => "users",
            Page::AutoSwitch => "swap",
            Page::About => "info",
        }
    }
}

pub struct SettingsRoot {
    state: Entity<AppState>,
    page: Page,
    _subs: Vec<Subscription>,
}

impl SettingsRoot {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<Globals>().state.clone();
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
        ];
        cx.on_release(|_, cx| {
            cx.global_mut::<Globals>().settings_window = None;
        })
        .detach();
        let page = match std::env::var("SWAPBAR_PAGE").as_deref() {
            Ok("menubar") => Page::MenuBar,
            Ok("notifications") => Page::Notifications,
            Ok("accounts") => Page::Accounts,
            Ok("auto") => Page::AutoSwitch,
            Ok("about") => Page::About,
            _ => Page::General,
        };
        SettingsRoot {
            state,
            page,
            _subs: subs,
        }
    }
}

// ---------- controls ----------

fn toggle(
    id: impl Into<SharedString>,
    on: bool,
    on_change: impl Fn(bool, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .w(px(38.0))
        .h(px(22.0))
        .rounded_full()
        .bg(if on {
            c(t().switch_on)
        } else {
            c(t().switch_off)
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .px(px(2.0))
        .when(on, |d| d.justify_end())
        .child(
            div()
                .size(px(18.0))
                .rounded_full()
                .bg(c(0xffffff))
                .shadow_sm(),
        )
        .on_click(move |_, _, cx| on_change(!on, cx))
}

fn segmented<V: Copy + PartialEq + 'static>(
    id: &str,
    options: &[V],
    selected: V,
    label: fn(V) -> &'static str,
    on_select: impl Fn(V, &mut App) + Clone + 'static,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .p(px(2.0))
        .rounded_md()
        .bg(c(t().hover))
        .gap(px(2.0));
    for (i, opt) in options.iter().copied().enumerate() {
        let on_select = on_select.clone();
        let is_sel = opt == selected;
        row = row.child(
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .px_2p5()
                .py(px(3.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(c(t().text))
                .when(is_sel, |d| {
                    d.bg(c(t().group))
                        .shadow_sm()
                        .font_weight(FontWeight::MEDIUM)
                })
                .child(label(opt).to_string())
                .on_click(move |_, _, cx| on_select(opt, cx)),
        );
    }
    row
}

fn stepper(
    id: &str,
    value: String,
    on_step: impl Fn(i64, &mut App) + Clone + 'static,
) -> impl IntoElement {
    let dec = on_step.clone();
    let inc = on_step;
    let btn = |id: String, glyph: &str| {
        div()
            .id(SharedString::from(id))
            .w(px(24.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(c(t().secondary))
            .hover(|d| d.bg(c(t().hover)).text_color(c(t().text)))
            .child(glyph.to_string())
    };
    div()
        .flex()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(c(t().hairline))
        .bg(c(t().group))
        .overflow_hidden()
        .child(btn(format!("{id}-dec"), "−").on_click(move |_, _, cx| dec(-1, cx)))
        .child(
            div()
                .min_w(px(64.0))
                .px_2()
                .flex()
                .justify_center()
                .font_family("Menlo")
                .text_size(px(11.5))
                .text_color(c(t().text))
                .child(value),
        )
        .child(btn(format!("{id}-inc"), "+").on_click(move |_, _, cx| inc(1, cx)))
}

fn button(
    id: &str,
    label: &str,
    danger: bool,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id.to_string()))
        .px_3()
        .py(px(4.0))
        .rounded_md()
        .border_1()
        .border_color(c(t().hairline))
        .bg(c(t().group))
        .cursor_pointer()
        .text_size(px(11.5))
        .text_color(c(if danger { t().danger } else { t().text }))
        .hover(|d| d.bg(c(t().hover)))
        .child(label.to_string())
        .on_click(move |_, _, cx| on_click(cx))
}

fn link(id: &str, glyph: &str, label: &str, url: &'static str) -> impl IntoElement {
    div()
        .id(SharedString::from(id.to_string()))
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .h(px(36.0))
        .cursor_pointer()
        .hover(|d| d.bg(c(t().hover)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2p5()
                .child(icon(glyph, 15.0, c(t().secondary)))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(c(t().text))
                        .child(label.to_string()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div().text_size(px(11.0)).text_color(c(t().tertiary)).child(
                        url.trim_start_matches("https://")
                            .trim_end_matches('/')
                            .to_string(),
                    ),
                )
                .child(icon("external", 12.0, c(t().tertiary))),
        )
        .on_click(move |_, _, cx| cx.open_url(url))
}

// ---------- layout ----------

fn group(children: Vec<gpui::AnyElement>) -> impl IntoElement {
    let n = children.len();
    let mut g = div()
        .flex()
        .flex_col()
        .rounded_lg()
        .bg(c(t().group))
        .border_1()
        .border_color(c(t().hairline))
        .overflow_hidden();
    for (i, ch) in children.into_iter().enumerate() {
        g = g.child(
            div()
                .when(i + 1 < n, |d| d.border_b_1().border_color(c(t().hairline)))
                .child(ch),
        );
    }
    g
}

fn row(label: &str, description: Option<&str>, control: impl IntoElement) -> gpui::AnyElement {
    let mut text = div()
        .flex()
        .flex_col()
        .gap(px(1.0))
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(px(12.5))
                .text_color(c(t().text))
                .child(label.to_string()),
        );
    if let Some(d) = description {
        text = text.child(
            div()
                .text_size(px(10.5))
                .text_color(c(t().secondary))
                .child(d.to_string()),
        );
    }
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .px_3()
        .py_2p5()
        .min_h(px(44.0))
        .child(text)
        .child(control)
        .into_any_element()
}

fn heading(label: &str) -> impl IntoElement {
    div()
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c(t().text))
        .mt_3()
        .mb_1()
        .child(label.to_string())
}

// ---------- cswap config helpers ----------

fn cfg<'a>(list: Option<&'a ConfigList>, key: &str) -> Option<&'a serde_json::Value> {
    list?
        .settings
        .iter()
        .find(|e| e.key == key)
        .map(|e| &e.value)
}
fn cfg_num(list: Option<&ConfigList>, key: &str, default: f64) -> f64 {
    cfg(list, key).and_then(|v| v.as_f64()).unwrap_or(default)
}
fn cfg_str(list: Option<&ConfigList>, key: &str) -> Option<String> {
    cfg(list, key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
fn cfg_bool(list: Option<&ConfigList>, key: &str) -> bool {
    cfg(list, key).and_then(|v| v.as_bool()).unwrap_or(false)
}

// A flat argument list reads better at the call sites than a one-off options struct.
#[expect(clippy::too_many_arguments)]
fn num_setting(
    id: &'static str,
    key: &'static str,
    list: Option<&ConfigList>,
    default: f64,
    step: f64,
    min: f64,
    max: f64,
    unit: &str,
) -> impl IntoElement {
    let cur = cfg_num(list, key, default);
    let shown = if unit.is_empty() {
        format!("{cur:.0}")
    } else {
        format!("{cur:.0}{unit}")
    };
    stepper(id, shown, move |dir, cx| {
        let next = (cur + dir as f64 * step).clamp(min, max);
        crate::set_cswap_config(cx, key.to_string(), Some(format!("{next:.0}")));
    })
}

// ---------- pages ----------

fn page_general(s: &AppState) -> impl IntoElement {
    let p = &s.prefs;
    let launch = crate::settings::launch_agent_installed();
    div()
        .flex()
        .flex_col()
        .child(heading("Appearance"))
        .child(group(vec![row(
            "Theme",
            Some("System follows the macOS light/dark setting."),
            segmented("theme", &Theme::ALL, p.theme, Theme::label, |t, cx| crate::update_prefs(cx, |p| p.theme = t)),
        )]))
        .child(heading("Startup"))
        .child(group(vec![row(
            "Launch at login",
            Some("Installs a LaunchAgent that starts the app when you sign in."),
            toggle("launch", launch, |on, cx| {
                if let Err(e) = crate::settings::set_launch_at_login(on) {
                    eprintln!("launch agent: {e}");
                }
                crate::update_prefs(cx, |p| p.launch_at_login = on);
            }),
        )]))
        .child(heading("Refresh"))
        .child(group(vec![
            row(
                "Account quotas",
                Some("How often to ask cswap for fresh limits. cswap itself paces the Anthropic API."),
                stepper("acc-refresh", format!("{}s", p.accounts_refresh_secs), |d, cx| {
                    crate::update_prefs(cx, |p| p.accounts_refresh_secs = (p.accounts_refresh_secs as i64 + d * 15).clamp(15, 600) as u64)
                }),
            ),
            row(
                "Token & cost scan",
                Some("How often to re-scan Claude Code transcripts. Scans are incremental."),
                stepper("usage-refresh", format!("{}s", p.usage_refresh_secs), |d, cx| {
                    crate::update_prefs(cx, |p| p.usage_refresh_secs = (p.usage_refresh_secs as i64 + d * 30).clamp(30, 1800) as u64)
                }),
            ),
        ]))
        .child(heading("Sections"))
        .child(group(vec![row(
            "Show token & cost usage",
            Some("Today / 30-day totals under the account list."),
            toggle("show-usage", p.show_usage, |on, cx| crate::update_prefs(cx, |p| p.show_usage = on)),
        )]))
        .child(heading("Keyboard shortcuts"))
        .child(group(vec![
            row(
                "Global shortcuts",
                Some("Work from any app. Turn off if another app needs these keys."),
                toggle("hotkeys", p.hotkeys, |on, cx| crate::update_prefs(cx, |p| p.hotkeys = on)),
            ),
            row("Open Swapbar", None, shortcut(hotkey::TOGGLE_LABEL, p.hotkeys)),
            row(
                "Switch to the account with the most quota left",
                None,
                shortcut(hotkey::SWITCH_BEST_LABEL, p.hotkeys),
            ),
        ]))
        .child(heading("Updates"))
        .child(group(vec![
            row(
                "Check for updates automatically",
                Some("Looks for a new release on GitHub at launch and every 6 hours."),
                toggle("check-updates", p.check_updates, |on, cx| {
                    crate::update_prefs(cx, |p| p.check_updates = on);
                    if on {
                        crate::check_for_updates(cx, false);
                    }
                }),
            ),
            update_row(s),
        ]))
}

fn shortcut(keys: &str, enabled: bool) -> impl IntoElement {
    div()
        .px_2()
        .py(px(2.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(c(t().hairline))
        .bg(c(t().content))
        .text_size(px(12.0))
        .text_color(c(if enabled { t().text } else { t().tertiary }))
        .child(keys.to_string())
}

fn update_row(s: &AppState) -> gpui::AnyElement {
    let status = if s.update_checking {
        "Checking…".to_string()
    } else if let Some(m) = &s.update_message {
        m.clone()
    } else {
        format!("You have version {APP_VERSION}.")
    };
    let control: gpui::AnyElement = match &s.update {
        Some(r) => {
            let url = r.url.clone();
            button(
                "download-update",
                &format!("Download {}", r.version),
                false,
                move |cx| cx.open_url(&url),
            )
            .into_any_element()
        }
        None => button("check-update", "Check now", false, |cx| {
            crate::check_for_updates(cx, true)
        })
        .into_any_element(),
    };
    row("Latest version", Some(&status), control)
}

fn page_notifications(s: &AppState) -> impl IntoElement {
    let p = &s.prefs;
    let mut col = div().flex().flex_col();
    if !notify::available() {
        col = col.child(
            div()
                .text_size(px(11.0))
                .text_color(c(t().danger))
                .mt_3()
                .child("Notifications need Swapbar to run from its app bundle (Swapbar.app)."),
        );
    }
    col.child(heading("Quota"))
        .child(group(vec![
            row(
                "Low quota at",
                Some("A window counts as low at this much left. Swapbar then suggests the account with the most room."),
                stepper("low-pct", format!("{}%", p.low_quota_pct), |d, cx| {
                    crate::update_prefs(cx, |p| p.low_quota_pct = (p.low_quota_pct as i64 + d * 5).clamp(5, 50) as u32)
                }),
            ),
            row(
                "When the current account runs low",
                Some("Once per 5h or 7d window, with a Switch button for the best other account."),
                toggle("notify-low", p.notify_low, |on, cx| {
                    crate::update_prefs(cx, |p| p.notify_low = on);
                    if on {
                        notify::request_authorization();
                    }
                }),
            ),
            row(
                "When an account's quota resets",
                Some("For accounts that had run low. Paused and hidden accounts are skipped."),
                toggle("notify-reset", p.notify_reset, |on, cx| {
                    crate::update_prefs(cx, |p| p.notify_reset = on);
                    if on {
                        notify::request_authorization();
                    }
                }),
            ),
        ]))
        .child(heading("System"))
        .child(group(vec![
            row(
                "Send a test notification",
                Some("If nothing appears, allow Swapbar in System Settings › Notifications."),
                div()
                    .flex()
                    .gap_2()
                    .child(button("test-notify", "Send test", false, |_| {
                        notify::request_authorization();
                        notify::post("test", "Swapbar notifications are on", "You'll hear from Swapbar when quota runs low or resets.", None);
                    }))
                    .child(button("open-notify-settings", "Open System Settings", false, |_| notify::open_system_settings())),
            ),
        ]))
}

fn page_menubar(s: &AppState) -> impl IntoElement {
    let p: &Prefs = &s.prefs;
    div()
        .flex()
        .flex_col()
        .child(heading("Menu bar"))
        .child(group(vec![
            row(
                "Percentage",
                Some("Show how much quota is left, or how much has been used. Applies to the menu bar and the account list."),
                segmented("pct-mode", &PercentMode::ALL, p.percent_mode, PercentMode::label, |m, cx| crate::update_prefs(cx, |p| p.percent_mode = m)),
            ),
            row(
                "Title next to the icon",
                Some("Which windows of the current account to show in the menu bar."),
                segmented("title-mode", &TitleMode::ALL, p.title_mode, TitleMode::label, |m, cx| crate::update_prefs(cx, |p| p.title_mode = m)),
            ),
        ]))
        .child(heading("Account rows"))
        .child(group(vec![
            row("Show 7-day window", None, toggle("show-7d", p.show_seven_day, |on, cx| crate::update_prefs(cx, |p| p.show_seven_day = on))),
            row("Show per-model limits", Some("Weekly model-scoped windows such as Fable."), toggle("show-scoped", p.show_model_limits, |on, cx| crate::update_prefs(cx, |p| p.show_model_limits = on))),
            row("Show reset times", None, toggle("show-reset", p.show_reset_times, |on, cx| crate::update_prefs(cx, |p| p.show_reset_times = on))),
        ]))
}

fn page_accounts(s: &AppState) -> impl IntoElement {
    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    match &s.accounts {
        Some(list) if !list.accounts.is_empty() => {
            for acc in &list.accounts {
                let n = acc.number;
                let hidden = s.prefs.is_hidden(n);
                let enabled = !acc.disabled;
                let label = format!("{}  {}", n, acc.email);
                let desc = format!(
                    "{}{}{}",
                    acc.org_tag(),
                    acc.plan
                        .as_deref()
                        .map(|p| format!(" · {p}"))
                        .unwrap_or_default(),
                    if acc.active { " · current" } else { "" }
                );
                rows.push(row(
                    &label,
                    Some(&desc),
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(c(t().secondary))
                                        .child("Show in menu"),
                                )
                                .child(toggle(format!("hide-{n}"), !hidden, move |on, cx| {
                                    crate::update_prefs(cx, |p| {
                                        if on != !p.is_hidden(n) {
                                            p.toggle_hidden(n);
                                        }
                                    })
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(c(t().secondary))
                                        .child("Auto-rotation"),
                                )
                                .child(toggle(format!("enable-{n}"), enabled, move |on, cx| {
                                    crate::set_account_enabled(cx, n, on)
                                })),
                        ),
                ));
            }
        }
        _ => rows.push(row(
            "No accounts",
            Some("Add one with `cswap add` in a terminal."),
            div(),
        )),
    }
    div()
        .flex()
        .flex_col()
        .child(heading("Managed accounts"))
        .child(div().text_size(px(11.0)).text_color(c(t().secondary)).mb_2().child("“Show in menu” only hides a row in this app. “Auto-rotation” runs `cswap enable/disable`, which keeps the account out of cswap's automatic switching."))
        .child(group(rows))
        .child(heading("Add or remove"))
        .child(group(vec![row(
            "Accounts are managed by cswap",
            Some("cswap add · cswap remove <n> · cswap alias <n> <name>"),
            button("open-cswap", "Open claude-swap on GitHub", false, |cx| cx.open_url("https://github.com/realiti4/claude-swap")),
        )]))
}

fn page_autoswitch(s: &AppState) -> impl IntoElement {
    let list = s.cswap_config.as_ref();
    let strategy = cfg_str(list, "autoswitch.strategy").unwrap_or_else(|| "best".into());
    let model = cfg_str(list, "autoswitch.model").unwrap_or_default();
    let strategy_sel: &'static str = if strategy == "consume-first" {
        "consume-first"
    } else {
        "best"
    };
    let model_sel: &'static str = match model.as_str() {
        "" => "none",
        "all" => "all",
        _ => "Fable",
    };
    let mut col = div().flex().flex_col();
    if let Some(e) = &s.config_error {
        col = col.child(
            div()
                .text_size(px(11.0))
                .text_color(c(t().danger))
                .mt_3()
                .child(e.clone()),
        );
    }
    col = col
        .child(heading("Thresholds"))
        .child(div().text_size(px(11.0)).text_color(c(t().secondary)).mb_2().child("These are cswap's own settings (`cswap config`). They drive `cswap auto`; run `cswap auto` or `cswap menubar --install-service` to have switching happen automatically."))
        .child(group(vec![
            row("Switch at", Some("Switch when the binding 5h/7d window reaches this much used."), num_setting("threshold", "autoswitch.threshold", list, 90.0, 5.0, 50.0, 99.0, "%")),
            row("Hysteresis", Some("A target must beat the active account by this many points."), num_setting("hyst", "autoswitch.hysteresisPct", list, 10.0, 1.0, 0.0, 50.0, "%")),
            row("Poll interval", Some("For the `cswap auto` loop."), num_setting("interval", "autoswitch.intervalSeconds", list, 60.0, 15.0, 15.0, 900.0, "s")),
            row("Cooldown", Some("Minimum time between proactive switches."), num_setting("cooldown", "autoswitch.cooldownSeconds", list, 300.0, 60.0, 0.0, 3600.0, "s")),
            row("Unhealthy after", Some("Consecutive failed polls before an account is skipped."), num_setting("unhealthy", "autoswitch.unhealthyTicks", list, 3.0, 1.0, 1.0, 20.0, " polls")),
        ]))
        .child(heading("Strategy"))
        .child(group(vec![
            row(
                "Pick the target by",
                Some("best: most headroom · consume-first: soonest weekly reset"),
                segmented("strategy", &["best", "consume-first"], strategy_sel, |m| m, |m, cx| {
                    crate::set_cswap_config(cx, "autoswitch.strategy".into(), Some(m.to_string()))
                })
                .into_any_element(),
            ),
            row(
                "Also watch model limits",
                Some("Switch on weekly model-scoped limits too."),
                segmented("model", &["none", "Fable", "all"], model_sel, |m| m, |m, cx| {
                    crate::set_cswap_config(cx, "autoswitch.model".into(), if m == "none" { None } else { Some(m.to_string()) })
                })
                .into_any_element(),
            ),
            row(
                "Include API-key accounts",
                Some("Allow rotating onto managed API-key accounts (billed per token)."),
                toggle("apikeys", cfg_bool(list, "autoswitch.includeApiKeyAccounts"), |on, cx| crate::set_cswap_config(cx, "autoswitch.includeApiKeyAccounts".into(), Some(on.to_string()))),
            ),
        ]))
        .child(heading(""))
        .child(group(vec![row(
            "Reset auto-switch settings",
            Some("Clears every autoswitch.* key back to cswap defaults."),
            button("reset-auto", "Reset to defaults", true, |cx| {
                for k in ["threshold", "intervalSeconds", "cooldownSeconds", "hysteresisPct", "strategy", "includeApiKeyAccounts", "unhealthyTicks", "model"] {
                    crate::set_cswap_config(cx, format!("autoswitch.{k}"), None);
                }
            }),
        )]));
    if s.config_busy {
        col = col.child(
            div()
                .text_size(px(11.0))
                .text_color(c(t().tertiary))
                .mt_2()
                .child("saving…"),
        );
    }
    col
}

fn page_about() -> impl IntoElement {
    let built = option_env!("SWAPBAR_BUILT").unwrap_or("source build");
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .py_6()
                .mt_3()
                .rounded_lg()
                .bg(c(t().group))
                .border_1()
                .border_color(c(t().hairline))
                .child(icon("logomark", 64.0, c(t().accent)))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(t().text))
                        .mt_2()
                        .child(APP_NAME),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(c(t().secondary))
                        .child(format!("Version {APP_VERSION}")),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(c(t().tertiary))
                        .child(built.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(c(t().secondary))
                        .mt_2()
                        .child("Every Claude account's quota in view, one click to switch."),
                ),
        )
        .child(heading("Author"))
        .child(group(vec![row(
            "Mustafa Afat",
            Some("Co-Owner & CTO of Miova and Founder & CEO of Fidkeep"),
            div()
                .text_size(px(11.0))
                .text_color(c(t().tertiary))
                .child("Turkey"),
        )]))
        .child(heading("Links"))
        .child(group(vec![
            link(
                "l-src",
                "github",
                "Swapbar source",
                "https://github.com/afatmustafa/swapbar",
            )
            .into_any_element(),
            link("l-web", "globe", "Website", "https://afat.me").into_any_element(),
            link("l-gh", "github", "GitHub", "https://github.com/afatmustafa").into_any_element(),
            link("l-x", "x", "X / Twitter", "https://twitter.com/afatmus").into_any_element(),
            link(
                "l-ig",
                "instagram",
                "Instagram",
                "https://www.instagram.com/afatmustafa/",
            )
            .into_any_element(),
            link(
                "l-miova",
                "rocket",
                "Miova",
                "https://github.com/miovaagency",
            )
            .into_any_element(),
        ]))
        .child(heading("Built with"))
        .child(group(vec![
            link(
                "l-cswap",
                "github",
                "claude-swap — multi-account switching and quota polling",
                "https://github.com/realiti4/claude-swap",
            )
            .into_any_element(),
            link(
                "l-gpui",
                "github",
                "gpui — Zed's GPU-accelerated UI framework",
                "https://github.com/zed-industries/zed/tree/main/crates/gpui",
            )
            .into_any_element(),
            link(
                "l-hugeicons",
                "globe",
                "Hugeicons — icon set",
                "https://hugeicons.com/?via=afat",
            )
            .into_any_element(),
        ]))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(c(t().tertiary))
                .mt_4()
                .mb_2()
                .flex()
                .justify_center()
                .child("© 2026 Mustafa Afat. MIT License."),
        )
}

// ---------- root ----------

impl Render for SettingsRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::theme::sync(cx);
        let s = self.state.read(cx).clone();
        let page = self.page;

        let mut sidebar = div()
            .flex()
            .flex_col()
            .w(px(200.0))
            .h_full()
            .bg(c(t().sidebar))
            .border_r_1()
            .border_color(c(t().hairline))
            .pt(px(48.0))
            .px_2()
            .gap(px(2.0));
        for p in Page::ALL {
            let selected = p == page;
            sidebar = sidebar.child(
                div()
                    .id(SharedString::from(format!("nav-{}", p.label())))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(28.0))
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(c(t().selected)))
                    .when(!selected, |d| d.hover(|d| d.bg(c(t().hover))))
                    .child(div().w(px(18.0)).flex().justify_center().child(icon(
                        p.glyph(),
                        15.0,
                        c(t().secondary),
                    )))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(c(t().text))
                            .child(p.label()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = p;
                        if p == Page::AutoSwitch {
                            crate::refresh_config(cx);
                        }
                        cx.notify();
                    })),
            );
        }

        let content: gpui::AnyElement = match page {
            Page::General => page_general(&s).into_any_element(),
            Page::MenuBar => page_menubar(&s).into_any_element(),
            Page::Notifications => page_notifications(&s).into_any_element(),
            Page::Accounts => page_accounts(&s).into_any_element(),
            Page::AutoSwitch => page_autoswitch(&s).into_any_element(),
            Page::About => page_about().into_any_element(),
        };

        div()
            .flex()
            .size_full()
            .bg(c(t().content))
            .font_family(UI_FONT)
            .text_color(c(t().text))
            .child(sidebar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px_6()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(page.label()),
                    )
                    .child(
                        div()
                            .id("settings-body")
                            .flex_1()
                            .overflow_y_scroll()
                            .px_6()
                            .pb_6()
                            .child(content),
                    ),
            )
    }
}
