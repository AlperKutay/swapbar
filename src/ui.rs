use gpui::{
    App, Context, Entity, FontWeight, Hsla, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window, div, prelude::*, px, relative, rgb,
};

use crate::advice::{self, Suggestion};
use crate::assets::icon;
use crate::cswap::{Account, Window as UsageWindow};
use crate::settings::{PercentMode, Prefs};
use crate::update::Release;
use crate::usage::{Summary, format_tokens, format_usd};
use crate::{AppState, Globals};

/// System UI font for labels; a monospaced face only where digits must line up.
const UI_FONT: &str = ".AppleSystemUIFont";
const NUM_FONT: &str = "Menlo";

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Colour palette; `t()` picks the light or dark one for the current appearance.
struct T {
    bg: u32,
    hairline: u32,
    hover: u32,
    text: u32,
    secondary: u32,
    tertiary: u32,
    track: u32,
    cache: u32,
    accent: u32,
    ok: u32,
    warn: u32,
    danger: u32,
}

const LIGHT: T = T {
    bg: 0xfafaf9,
    hairline: 0xe8e7e3,
    hover: 0xf1f0ec,
    text: 0x1d1d1f,
    secondary: 0x6e6e73,
    tertiary: 0xa1a1a6,
    track: 0xe9e8e4,
    cache: 0xc9c8c3,
    accent: 0xff4101,
    ok: 0x34a853,
    warn: 0xe0a100,
    danger: 0xd93025,
};

const DARK: T = T {
    bg: 0x1f1f1e,
    hairline: 0x353533,
    hover: 0x2b2b29,
    text: 0xf2f2f0,
    secondary: 0xa1a1a6,
    tertiary: 0x707074,
    track: 0x363634,
    cache: 0x5f5e5a,
    accent: 0xff5a1f,
    ok: 0x3fbf62,
    warn: 0xf0b429,
    danger: 0xff5f52,
};

fn t() -> &'static T {
    if crate::theme::is_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

fn remaining_color(remaining: f64) -> Hsla {
    if remaining <= 15.0 {
        c(t().danger)
    } else if remaining <= 40.0 {
        c(t().warn)
    } else {
        c(t().ok)
    }
}

pub struct Root {
    state: Entity<AppState>,
    was_active: bool,
    _subs: Vec<Subscription>,
}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<Globals>().state.clone();
        let mut subs = Vec::new();
        subs.push(cx.observe(&state, |_, _, cx| cx.notify()));
        subs.push(cx.observe_window_appearance(window, |_, _, cx| cx.notify()));
        subs.push(cx.observe_window_activation(window, |this, window, cx| {
            crate::debug_log(format_args!("popover active={}", window.is_window_active()));
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                cx.defer(crate::dismiss_window);
            }
        }));
        cx.on_release(|_, cx| {
            cx.global_mut::<Globals>().window = None;
            cx.global::<Globals>().status.set_highlighted(false);
            crate::statusbar::watch_outside_clicks(false);
        })
        .detach();
        Root {
            state,
            was_active: false,
            _subs: subs,
        }
    }
}

// ---------- primitives ----------

fn hairline() -> impl IntoElement {
    div().h(px(1.0)).w_full().bg(c(t().hairline))
}

fn num(s: impl Into<SharedString>) -> gpui::Div {
    div()
        .font_family(NUM_FONT)
        .whitespace_nowrap()
        .child(s.into())
}

fn section_header(glyph: &str, title: &str, trailing: Option<String>) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .justify_between()
        .px_4()
        .pt_3()
        .pb_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(icon(glyph, 12.0, c(t().secondary)))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(t().secondary))
                        .child(title.to_string()),
                ),
        );
    if let Some(text) = trailing {
        row = row.child(
            div()
                .text_size(px(10.5))
                .text_color(c(t().tertiary))
                .child(text),
        );
    }
    row
}

fn quota_row(label: &str, w: Option<&UsageWindow>, prefs: &Prefs) -> impl IntoElement {
    let mode = prefs.percent_mode;
    let remaining = w.map(|w| (100.0 - w.pct).clamp(0.0, 100.0));
    let shown = w.map(|w| mode.value(w.pct));
    let reset = if prefs.show_reset_times {
        w.and_then(|w| match (&w.countdown, &w.clock) {
            (Some(cd), Some(clock)) => Some(format!("{cd} · {clock}")),
            (Some(cd), None) => Some(cd.clone()),
            _ => None,
        })
        .unwrap_or_default()
    } else {
        String::new()
    };
    let color = remaining.map(remaining_color).unwrap_or(c(t().tertiary));
    // The bar always fills with the shown quantity so it reads the same way as the number.
    let fill = shown.unwrap_or(0.0) / 100.0;
    let bar_color = if mode == PercentMode::Used {
        c(t().secondary)
    } else {
        color
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .h(px(18.0))
        .child(
            div()
                .w(px(40.0))
                .text_size(px(11.0))
                .text_color(c(t().secondary))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(label.to_string()),
        )
        .child(
            div()
                .w(px(120.0))
                .h(px(3.0))
                .rounded_full()
                .bg(c(t().track))
                .child(
                    div()
                        .h_full()
                        .w(relative(fill as f32))
                        .rounded_full()
                        .bg(bar_color),
                ),
        )
        .child(
            num(shown
                .map(|r| format!("{r:>3.0}%"))
                .unwrap_or_else(|| "  —".into()))
            .w(px(36.0))
            .text_size(px(11.0))
            .text_color(color),
        )
        .child(
            div()
                .w(px(30.0))
                .text_size(px(10.0))
                .text_color(c(t().tertiary))
                .child(if shown.is_some() { mode.suffix() } else { "" }),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(10.5))
                .text_color(c(t().tertiary))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(reset),
        )
}

// ---------- banners ----------

fn banner(
    id: &'static str,
    glyph: &str,
    title: String,
    detail: String,
    action: &str,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div().px_3().pt_3().child(
        div()
            .flex()
            .items_center()
            .gap_2p5()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(c(t().hover))
            .border_1()
            .border_color(c(t().hairline))
            .child(icon(glyph, 14.0, c(t().accent)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(t().text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(c(t().secondary))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .id(id)
                    .flex_shrink_0()
                    .px_2p5()
                    .py(px(3.0))
                    .rounded(px(5.0))
                    .bg(c(t().accent))
                    .cursor_pointer()
                    .hover(|d| d.opacity(0.85))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c(0xffffff))
                    .child(action.to_string())
                    .on_click(move |_, _, cx| on_click(cx)),
            ),
    )
}

fn suggestion_banner(s: &Suggestion) -> impl IntoElement {
    let number = s.number;
    banner(
        "suggest-switch",
        "swap",
        format!("Current account has {:.0}% left", s.current_headroom),
        format!("{} has {:.0}% left", s.email, s.headroom),
        "Switch",
        move |cx| crate::switch_account(cx, number),
    )
}

fn update_banner(r: &Release) -> impl IntoElement {
    let url = r.url.clone();
    banner(
        "update",
        "rocket",
        format!("{} {} is available", crate::APP_NAME, r.version),
        format!("You have {}", crate::APP_VERSION),
        "Download",
        move |cx| cx.open_url(&url),
    )
}

// ---------- accounts ----------

fn account_row(
    acc: &Account,
    prefs: &Prefs,
    switching: Option<u32>,
    suggested: bool,
    is_last: bool,
    cx: &mut Context<Root>,
) -> impl IntoElement {
    let number = acc.number;
    let active = acc.active;
    let usage = acc.usage.as_ref();
    let five = usage.and_then(|u| u.five_hour.as_ref());
    let seven = usage.and_then(|u| u.seven_day.as_ref());
    let scoped: Vec<&UsageWindow> = usage.map(|u| u.scoped.iter().collect()).unwrap_or_default();

    let status: (String, u32) = if switching == Some(number) {
        ("switching…".into(), t().secondary)
    } else if active {
        ("current".into(), t().accent)
    } else if acc.disabled {
        ("paused".into(), t().tertiary)
    } else if suggested {
        ("suggested".into(), t().ok)
    } else {
        (String::new(), t().tertiary)
    };

    let title = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .size(px(7.0))
                .rounded_full()
                .bg(if active {
                    c(t().accent)
                } else {
                    c(t().hairline)
                })
                .flex_shrink_0(),
        )
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .text_color(c(t().text))
                .whitespace_nowrap()
                .child(acc.email.clone()),
        )
        .when_some(acc.plan.clone(), |d, plan| {
            d.child(
                div()
                    .flex_shrink_0()
                    .px(px(5.0))
                    .rounded(px(4.0))
                    .bg(c(t().hover))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c(t().secondary))
                    .whitespace_nowrap()
                    .child(plan),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(11.0))
                .text_color(c(t().tertiary))
                .child(acc.org_tag()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_size(px(11.0))
                .text_color(c(status.1))
                .whitespace_nowrap()
                .child(status.0),
        );

    let mut quotas = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .pl(px(15.0))
        .child(quota_row("5h", five, prefs));
    if seven.is_some() && prefs.show_seven_day {
        quotas = quotas.child(quota_row("7d", seven, prefs));
    }
    if prefs.show_model_limits {
        for s in scoped {
            quotas = quotas.child(quota_row(
                s.name.as_deref().unwrap_or("model"),
                Some(s),
                prefs,
            ));
        }
    }
    if let Some(st) = acc.usage_status.as_deref()
        && st != "ok"
    {
        quotas = quotas.child(
            div()
                .pl(px(0.0))
                .text_size(px(10.5))
                .text_color(c(t().warn))
                .child(st.to_string()),
        );
    }

    div()
        .id(SharedString::from(format!("acct-{number}")))
        .flex()
        .flex_col()
        .gap_1p5()
        .px_4()
        .py_2p5()
        .when(!is_last, |d| d.border_b_1().border_color(c(t().hairline)))
        .when(!active, |d| {
            d.cursor_pointer().hover(|d| d.bg(c(t().hover)))
        })
        .child(title)
        .child(quotas)
        .on_click(cx.listener(move |_, _, _, cx| {
            if !active {
                crate::switch_account(cx, number);
            }
        }))
}

fn accounts_section(
    state: &AppState,
    suggested: Option<u32>,
    cx: &mut Context<Root>,
) -> impl IntoElement {
    let fresh = state
        .accounts
        .as_ref()
        .and_then(|l| {
            l.accounts
                .iter()
                .filter_map(|a| a.usage_age_seconds)
                .reduce(f64::min)
        })
        .map(|s| {
            if s < 90.0 {
                "updated just now".to_string()
            } else if s < 3600.0 {
                format!("updated {}m ago", (s / 60.0) as u64)
            } else {
                format!("updated {}h ago", (s / 3600.0) as u64)
            }
        });
    let prefs = &state.prefs;
    let title = match prefs.percent_mode {
        PercentMode::Remaining => "Accounts · quota remaining",
        PercentMode::Used => "Accounts · quota used",
    };
    let mut col = div()
        .flex()
        .flex_col()
        .child(section_header("users", title, fresh));
    match (&state.accounts, &state.accounts_error) {
        (Some(list), _) => {
            let visible: Vec<&Account> = list
                .accounts
                .iter()
                .filter(|a| !prefs.is_hidden(a.number))
                .collect();
            let n = visible.len();
            if n == 0 {
                col = col.child(
                    div()
                        .px_4()
                        .py_2()
                        .text_size(px(12.0))
                        .text_color(c(t().secondary))
                        .child(if list.accounts.is_empty() {
                            "No accounts. Add one with `cswap add`."
                        } else {
                            "All accounts are hidden. Show them in Settings › Accounts."
                        }),
                );
            }
            for (i, acc) in visible.iter().enumerate() {
                col = col.child(account_row(
                    acc,
                    prefs,
                    state.switching,
                    suggested == Some(acc.number),
                    i + 1 == n,
                    cx,
                ));
            }
        }
        (None, Some(err)) => {
            col = col.child(
                div()
                    .px_4()
                    .py_2()
                    .text_size(px(11.5))
                    .text_color(c(t().danger))
                    .child(err.clone()),
            )
        }
        (None, None) => {
            col = col.child(
                div()
                    .px_4()
                    .py_2()
                    .text_size(px(12.0))
                    .text_color(c(t().tertiary))
                    .child("Loading…"),
            )
        }
    }
    if let (Some(_), Some(err)) = (&state.accounts, &state.accounts_error) {
        col = col.child(
            div()
                .px_4()
                .py_2()
                .text_size(px(11.0))
                .text_color(c(t().danger))
                .child(err.clone()),
        );
    }
    col
}

// ---------- usage ----------

const COL_W: f32 = 92.0;

fn usage_line(
    label: impl Into<SharedString>,
    today: String,
    month: String,
    bold: bool,
    muted: bool,
) -> impl IntoElement {
    let color = if muted { t().secondary } else { t().text };
    let weight = if bold {
        FontWeight::SEMIBOLD
    } else {
        FontWeight::NORMAL
    };
    div()
        .flex()
        .items_center()
        .h(px(20.0))
        .px_4()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(11.5))
                .text_color(c(color))
                .child(label.into()),
        )
        .child(
            num(today)
                .w(px(COL_W))
                .flex()
                .justify_end()
                .text_size(px(11.5))
                .font_weight(weight)
                .text_color(c(color)),
        )
        .child(
            num(month)
                .w(px(COL_W))
                .flex()
                .justify_end()
                .text_size(px(11.5))
                .font_weight(weight)
                .text_color(c(color)),
        )
}

fn stacked_bar(summary: &Summary) -> impl IntoElement {
    let tot = &summary.total;
    let total = tot.tokens() as f64;
    let segs = [
        (tot.input, t().text),
        (tot.cache_read, t().cache),
        (tot.cache_create, t().warn),
        (tot.output, t().accent),
    ];
    let mut bar = div()
        .flex()
        .h(px(4.0))
        .w_full()
        .rounded_full()
        .overflow_hidden()
        .bg(c(t().track));
    if total > 0.0 {
        for (v, color) in segs {
            if v > 0 {
                bar = bar.child(
                    div()
                        .h_full()
                        .w(relative((v as f64 / total) as f32))
                        .bg(c(color)),
                );
            }
        }
    }
    let pct = |v: u64| {
        if total > 0.0 {
            v as f64 * 100.0 / total
        } else {
            0.0
        }
    };
    let legend = div()
        .flex()
        .gap_3()
        .text_size(px(10.0))
        .text_color(c(t().tertiary))
        .child(format!("input {:.0}%", pct(tot.input)))
        .child(format!("cache read {:.0}%", pct(tot.cache_read)))
        .child(format!("cache write {:.0}%", pct(tot.cache_create)))
        .child(format!("output {:.0}%", pct(tot.output)));
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_4()
        .pt_1()
        .pb_2()
        .child(bar)
        .child(legend)
}

fn usage_section(state: &AppState) -> impl IntoElement {
    let status = if state.usage_scanning {
        Some("scanning transcripts…".to_string())
    } else if state.usage_scanned_once {
        Some("estimated at API prices".to_string())
    } else {
        None
    };
    let mut col = div()
        .flex()
        .flex_col()
        .child(section_header("coins", "Usage", status));

    let (Some(today), Some(month)) = (&state.summary_today, &state.summary_month) else {
        return col.child(
            div()
                .px_4()
                .py_2()
                .text_size(px(12.0))
                .text_color(c(t().tertiary))
                .child("Scanning Claude Code transcripts…"),
        );
    };

    // column headers
    col = col.child(
        div()
            .flex()
            .items_center()
            .h(px(18.0))
            .px_4()
            .child(div().flex_1())
            .child(
                num("today")
                    .w(px(COL_W))
                    .flex()
                    .justify_end()
                    .text_size(px(10.5))
                    .text_color(c(t().tertiary)),
            )
            .child(
                num("30 days")
                    .w(px(COL_W))
                    .flex()
                    .justify_end()
                    .text_size(px(10.5))
                    .text_color(c(t().tertiary)),
            ),
    );
    col = col
        .child(usage_line(
            "Cost",
            format_usd(today.total.cost),
            format_usd(month.total.cost),
            true,
            false,
        ))
        .child(usage_line(
            "Tokens",
            format_tokens(today.total.tokens()),
            format_tokens(month.total.tokens()),
            false,
            false,
        ))
        .child(usage_line(
            "Requests",
            group(today.total.requests),
            group(month.total.requests),
            false,
            false,
        ))
        .child(usage_line(
            "Output tokens",
            format_tokens(today.total.output),
            format_tokens(month.total.output),
            false,
            true,
        ))
        .child(usage_line(
            "Cache read",
            format_tokens(today.total.cache_read),
            format_tokens(month.total.cache_read),
            false,
            true,
        ));

    col = col.child(div().px_4().py_1p5().child(hairline()));

    // per-account costs, ordered by 30-day cost; only worth a block with two or more
    if month.by_account.len() > 1 {
        for (owner, tot) in &month.by_account {
            let today_cost = today
                .by_account
                .iter()
                .find(|(o, _)| o == owner)
                .map(|(_, t)| t.cost)
                .unwrap_or(0.0);
            col = col.child(usage_line(
                owner.label().to_string(),
                if today_cost > 0.0 {
                    format_usd(today_cost)
                } else {
                    "–".into()
                },
                format_usd(tot.cost),
                false,
                false,
            ));
        }
        col = col.child(div().px_4().py_1p5().child(hairline()));
    }

    // per-model costs, ordered by 30-day cost
    for (name, tot) in month.by_model.iter().take(6) {
        let short = name.strip_prefix("claude-").unwrap_or(name).to_string();
        let today_cost = today
            .by_model
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.cost)
            .unwrap_or(0.0);
        let unpriced = tot.unpriced >= tot.requests && tot.cost == 0.0;
        col = col.child(usage_line(
            short,
            if today_cost > 0.0 {
                format_usd(today_cost)
            } else {
                "–".into()
            },
            if unpriced {
                "unpriced".into()
            } else {
                format_usd(tot.cost)
            },
            false,
            false,
        ));
    }

    col.child(stacked_bar(month))
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

// ---------- footer ----------

fn footer_item(
    id: &'static str,
    glyph: &str,
    label: &str,
    danger: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .text_size(px(11.5))
        .text_color(c(t().secondary))
        .hover(move |d| {
            d.bg(c(t().hover))
                .text_color(c(if danger { t().danger } else { t().text }))
        })
        .child(icon(glyph, 13.0, c(t().secondary)))
        .child(label.to_string())
        .on_click(move |_, w, cx| on_click(w, cx))
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::theme::sync(cx);
        let s = self.state.read(cx).clone();
        let busy = s.accounts_loading || s.usage_scanning || s.switching.is_some();

        let show_usage = s.prefs.show_usage;
        let suggestion = s
            .accounts
            .as_ref()
            .and_then(|l| advice::suggestion(l, &s.prefs))
            .filter(|_| s.switching.is_none());
        let body = div()
            .id("body")
            .flex_1()
            .overflow_y_scroll()
            .when_some(s.update.as_ref(), |d, r| d.child(update_banner(r)))
            .when_some(suggestion.as_ref(), |d, sg| d.child(suggestion_banner(sg)))
            .child(accounts_section(
                &s,
                suggestion.as_ref().map(|sg| sg.number),
                cx,
            ))
            .when(show_usage, |d| {
                d.child(div().px_4().py_1().child(hairline()))
                    .child(usage_section(&s))
            });

        let footer = div()
            .flex()
            .items_center()
            .px_2()
            .py_1p5()
            .border_t_1()
            .border_color(c(t().hairline))
            .child(footer_item(
                "refresh",
                "refresh",
                if busy { "Refreshing…" } else { "Refresh" },
                false,
                |_, cx| {
                    crate::refresh_accounts(cx);
                    crate::refresh_usage(cx);
                },
            ))
            .child(div().flex_1())
            .child(footer_item(
                "settings",
                "settings-small",
                "Settings…",
                false,
                |_, cx| crate::open_settings(cx),
            ))
            .child(footer_item("quit", "power", "Quit", true, |_, cx| {
                cx.quit()
            }));

        div()
            .flex()
            .flex_col()
            .size_full()
            .rounded_lg()
            .border_1()
            .border_color(c(t().hairline))
            .bg(c(t().bg))
            .font_family(UI_FONT)
            .text_size(px(12.0))
            .text_color(c(t().text))
            .child(body)
            .child(footer)
    }
}
