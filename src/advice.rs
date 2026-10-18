//! Quota reasoning shared by the popover, the notifications and the hotkeys: how much
//! room an account has left, which account to switch to, and which quota changes are
//! worth a notification. Pure functions over cswap's account list, so they're testable.

use std::collections::HashMap;

use crate::cswap::{Account, AccountList};
use crate::settings::Prefs;

/// A target must have at least this many more points of headroom than the current
/// account before we suggest it (the same idea as cswap's hysteresis).
const MIN_GAIN: f64 = 10.0;

/// A window counts as reset when its used percentage falls by at least this much
/// between two polls.
const RESET_DROP: f64 = 30.0;

/// Percentage left in the account's tightest window (5h or 7d), or `None` without usage data.
pub fn headroom(acc: &Account) -> Option<f64> {
    if acc.usage_status.as_deref().is_some_and(|s| s != "ok") {
        return None;
    }
    let u = acc.usage.as_ref()?;
    [u.five_hour.as_ref(), u.seven_day.as_ref()]
        .into_iter()
        .flatten()
        .map(|w| (100.0 - w.pct).clamp(0.0, 100.0))
        .reduce(f64::min)
}

/// The inactive account with the most headroom that is shown in the menu and not paused.
pub fn best_other<'a>(list: &'a AccountList, prefs: &Prefs) -> Option<(&'a Account, f64)> {
    list.accounts
        .iter()
        .filter(|a| !a.active && !a.disabled && !prefs.is_hidden(a.number))
        .filter_map(|a| headroom(a).map(|h| (a, h)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub number: u32,
    pub email: String,
    pub headroom: f64,
    pub current_headroom: f64,
}

/// Suggest a switch once the current account is at or below `low_pct` and another one
/// has clearly more room.
pub fn suggestion(list: &AccountList, prefs: &Prefs) -> Option<Suggestion> {
    let current = headroom(list.accounts.iter().find(|a| a.active)?)?;
    if current > prefs.low_quota_pct as f64 {
        return None;
    }
    let (acc, h) = best_other(list, prefs)?;
    (h >= current + MIN_GAIN).then(|| Suggestion {
        number: acc.number,
        email: acc.email.clone(),
        headroom: h,
        current_headroom: current,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The current account's `window` dropped to `left` percent.
    Low {
        email: String,
        window: &'static str,
        left: f64,
        suggestion: Option<Suggestion>,
    },
    /// An account whose `window` had run low has quota again.
    Reset {
        number: u32,
        email: String,
        window: &'static str,
        left: f64,
    },
}

/// Remembers the last used percentage per account and window, so each threshold
/// crossing produces one event rather than one per poll.
#[derive(Debug, Default)]
pub struct Watch {
    last: HashMap<(u32, &'static str), f64>,
}

impl Watch {
    /// Compare a fresh account list with the previous one. The first list only sets the
    /// baseline: quota that was already low at launch is not news.
    pub fn update(&mut self, list: &AccountList, prefs: &Prefs) -> Vec<Event> {
        let low_used = 100.0 - prefs.low_quota_pct as f64;
        let mut events = Vec::new();
        let mut next = HashMap::new();
        for acc in &list.accounts {
            if acc.usage_status.as_deref().is_some_and(|s| s != "ok") {
                // keep the old reading so a failed poll doesn't look like a reset later
                for key in self.last.keys().filter(|k| k.0 == acc.number) {
                    next.insert(*key, self.last[key]);
                }
                continue;
            }
            let Some(u) = acc.usage.as_ref() else {
                continue;
            };
            for (window, w) in [("5h", &u.five_hour), ("7d", &u.seven_day)] {
                let Some(w) = w else { continue };
                let key = (acc.number, window);
                next.insert(key, w.pct);
                let Some(&prev) = self.last.get(&key) else {
                    continue;
                };
                let left = (100.0 - w.pct).clamp(0.0, 100.0);
                if acc.active && prefs.notify_low && prev < low_used && w.pct >= low_used {
                    events.push(Event::Low {
                        email: acc.email.clone(),
                        window,
                        left,
                        suggestion: suggestion(list, prefs),
                    });
                }
                if prefs.notify_reset
                    && !acc.disabled
                    && !prefs.is_hidden(acc.number)
                    && prev >= low_used
                    && w.pct <= prev - RESET_DROP
                {
                    events.push(Event::Reset {
                        number: acc.number,
                        email: acc.email.clone(),
                        window,
                        left,
                    });
                }
            }
        }
        self.last = next;
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cswap::{Usage, Window};

    fn acc(number: u32, active: bool, five: f64, seven: Option<f64>) -> Account {
        Account {
            number,
            email: format!("a{number}@example.com"),
            active,
            usage_status: Some("ok".into()),
            usage: Some(Usage {
                five_hour: Some(Window {
                    pct: five,
                    ..Default::default()
                }),
                seven_day: seven.map(|pct| Window {
                    pct,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn list(accounts: Vec<Account>) -> AccountList {
        AccountList {
            active_account_number: accounts.iter().find(|a| a.active).map(|a| a.number),
            accounts,
        }
    }

    fn prefs() -> Prefs {
        Prefs {
            low_quota_pct: 15,
            ..Default::default()
        }
    }

    #[test]
    fn headroom_uses_the_tightest_window() {
        assert_eq!(headroom(&acc(1, true, 20.0, Some(70.0))), Some(30.0));
        assert_eq!(headroom(&acc(1, true, 80.0, None)), Some(20.0));
        let mut broken = acc(1, true, 0.0, None);
        broken.usage_status = Some("rate limited".into());
        assert_eq!(headroom(&broken), None);
    }

    #[test]
    fn best_other_skips_active_paused_and_hidden() {
        let mut paused = acc(2, false, 0.0, None);
        paused.disabled = true;
        let l = list(vec![
            acc(1, true, 0.0, None),
            paused,
            acc(3, false, 50.0, None),
            acc(4, false, 10.0, None),
        ]);
        let mut p = prefs();
        assert_eq!(best_other(&l, &p).map(|(a, _)| a.number), Some(4));
        p.hidden_accounts.push(4);
        assert_eq!(best_other(&l, &p).map(|(a, _)| a.number), Some(3));
    }

    #[test]
    fn suggestion_only_when_current_is_low_and_target_is_clearly_better() {
        let p = prefs();
        assert_eq!(
            suggestion(
                &list(vec![acc(1, true, 50.0, None), acc(2, false, 0.0, None)]),
                &p
            ),
            None
        );
        let s = suggestion(
            &list(vec![acc(1, true, 90.0, None), acc(2, false, 20.0, None)]),
            &p,
        )
        .unwrap();
        assert_eq!((s.number, s.headroom, s.current_headroom), (2, 80.0, 10.0));
        // 15% left vs 18% left is not worth a switch
        assert_eq!(
            suggestion(
                &list(vec![acc(1, true, 85.0, None), acc(2, false, 82.0, None)]),
                &p
            ),
            None
        );
    }

    #[test]
    fn first_list_is_only_a_baseline() {
        let mut w = Watch::default();
        let events = w.update(&list(vec![acc(1, true, 95.0, None)]), &prefs());
        assert!(events.is_empty());
    }

    #[test]
    fn low_fires_once_per_crossing() {
        let p = prefs();
        let mut w = Watch::default();
        w.update(
            &list(vec![acc(1, true, 70.0, None), acc(2, false, 0.0, None)]),
            &p,
        );
        let events = w.update(
            &list(vec![acc(1, true, 88.0, None), acc(2, false, 0.0, None)]),
            &p,
        );
        assert_eq!(events.len(), 1);
        let Event::Low {
            window,
            left,
            suggestion,
            ..
        } = &events[0]
        else {
            panic!("expected Low, got {events:?}")
        };
        assert_eq!((*window, *left), ("5h", 12.0));
        assert_eq!(suggestion.as_ref().map(|s| s.number), Some(2));
        assert!(
            w.update(
                &list(vec![acc(1, true, 92.0, None), acc(2, false, 0.0, None)]),
                &p
            )
            .is_empty()
        );
    }

    #[test]
    fn low_ignores_inactive_accounts_and_respects_the_pref() {
        let mut p = prefs();
        let mut w = Watch::default();
        w.update(
            &list(vec![acc(1, true, 0.0, None), acc(2, false, 70.0, None)]),
            &p,
        );
        assert!(
            w.update(
                &list(vec![acc(1, true, 0.0, None), acc(2, false, 90.0, None)]),
                &p
            )
            .is_empty()
        );

        p.notify_low = false;
        let mut w = Watch::default();
        w.update(&list(vec![acc(1, true, 70.0, None)]), &p);
        assert!(
            w.update(&list(vec![acc(1, true, 90.0, None)]), &p)
                .is_empty()
        );
    }

    #[test]
    fn reset_fires_when_a_low_window_recovers() {
        let p = prefs();
        let mut w = Watch::default();
        w.update(
            &list(vec![
                acc(1, true, 0.0, None),
                acc(2, false, 100.0, Some(40.0)),
            ]),
            &p,
        );
        let events = w.update(
            &list(vec![
                acc(1, true, 0.0, None),
                acc(2, false, 0.0, Some(40.0)),
            ]),
            &p,
        );
        assert_eq!(
            events,
            vec![Event::Reset {
                number: 2,
                email: "a2@example.com".into(),
                window: "5h",
                left: 100.0
            }]
        );
    }

    #[test]
    fn a_failed_poll_does_not_count_as_a_reset() {
        let p = prefs();
        let mut w = Watch::default();
        w.update(&list(vec![acc(1, false, 100.0, None)]), &p);
        let mut failed = acc(1, false, 0.0, None);
        failed.usage_status = Some("error".into());
        failed.usage = None;
        assert!(w.update(&list(vec![failed]), &p).is_empty());
        // still at 100% when it comes back: no reset
        assert!(
            w.update(&list(vec![acc(1, false, 100.0, None)]), &p)
                .is_empty()
        );
        // a real reset afterwards is still reported
        assert_eq!(w.update(&list(vec![acc(1, false, 5.0, None)]), &p).len(), 1);
    }

    #[test]
    fn reset_skips_paused_accounts() {
        let p = prefs();
        let mut w = Watch::default();
        let mut a = acc(2, false, 100.0, None);
        a.disabled = true;
        w.update(&list(vec![a.clone()]), &p);
        a.usage.as_mut().unwrap().five_hour.as_mut().unwrap().pct = 0.0;
        assert!(w.update(&list(vec![a]), &p).is_empty());
    }
}
