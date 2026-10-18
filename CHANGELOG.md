# Changelog

All notable changes to Swapbar. The website shows the same notes at
[swapbar.afat.me/changelog](https://swapbar.afat.me/changelog).

## 0.2.0

### Added

- **Quota notifications.** A notification when the current account runs low, with a **Switch** button for the account with the most quota left, and another when a depleted account's quota resets. Turn each on or off, and set the low-quota level, in Settings › Notifications.
- **Switch suggestion.** Once the current account's tightest window reaches the low-quota level (15% left by default), the popover shows which account has the most room and switches to it in one click. That account is marked **suggested** in the list.
- **Global shortcuts.** `⌃⌥S` opens Swapbar from any app, and `⌃⌥⇧S` switches to the account with the most quota left. No Accessibility permission needed; turn them off in Settings › General.
- **Cost per account.** The usage section splits cost by account. The account comes from cswap's switch log for the default profile, from the slot number for `cswap run` sessions, and from the login of each other profile.
- **Update check.** Swapbar looks for a new release on GitHub at launch and every 6 hours and shows a **Download** banner when one is out. Check by hand, or turn it off, in Settings › General.
- **Homebrew.** `brew install --cask afatmustafa/tap/swapbar`.

### Fixed

- The popover could stay on screen after opening Settings and then ignore clicks outside it and on the menu bar icon.
- Amounts that round up to a whole dollar showed as `$0.100` instead of `$1.00`.

## 0.1.1

### Changed

- Releases are signed with a Developer ID and notarized by Apple, so Swapbar opens with a double-click.
- The DMG background uses the Swapbar mark in its menu bar.

### Fixed

- The menu bar popover sometimes didn't close, or reopened right after closing.

## 0.1.0

First release: every cswap account's 5h, 7d and per-model quota in the menu bar, one-click switching, token and cost totals from Claude Code transcripts, light and dark mode, and a settings window for cswap's auto-switch options.
