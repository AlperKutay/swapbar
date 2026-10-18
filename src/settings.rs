//! App preferences persisted at `~/.config/swapbar/settings.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitleMode {
    /// Icon only
    Off,
    /// 5h remaining
    FiveHour,
    /// 5h · 7d remaining
    Both,
}

impl TitleMode {
    pub const ALL: [TitleMode; 3] = [TitleMode::Off, TitleMode::FiveHour, TitleMode::Both];
    pub fn label(self) -> &'static str {
        match self {
            TitleMode::Off => "Icon only",
            TitleMode::FiveHour => "5h",
            TitleMode::Both => "5h · 7d",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PercentMode {
    Remaining,
    Used,
}

impl PercentMode {
    pub const ALL: [PercentMode; 2] = [PercentMode::Remaining, PercentMode::Used];
    pub fn label(self) -> &'static str {
        match self {
            PercentMode::Remaining => "Remaining",
            PercentMode::Used => "Used",
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            PercentMode::Remaining => "left",
            PercentMode::Used => "used",
        }
    }
    /// Value to display for a window reporting `used` percent.
    pub fn value(self, used: f64) -> f64 {
        match self {
            PercentMode::Remaining => (100.0 - used).clamp(0.0, 100.0),
            PercentMode::Used => used.clamp(0.0, 100.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];
    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: Theme,
    pub title_mode: TitleMode,
    pub percent_mode: PercentMode,
    pub show_usage: bool,
    pub show_seven_day: bool,
    pub show_model_limits: bool,
    pub show_reset_times: bool,
    /// Account slot numbers hidden from the popover (still switchable via cswap).
    pub hidden_accounts: Vec<u32>,
    pub accounts_refresh_secs: u64,
    pub usage_refresh_secs: u64,
    pub launch_at_login: bool,
    /// Remaining percentage at which a window counts as low (notifications, switch suggestion).
    pub low_quota_pct: u32,
    pub notify_low: bool,
    pub notify_reset: bool,
    pub check_updates: bool,
    pub hotkeys: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: Theme::System,
            title_mode: TitleMode::FiveHour,
            percent_mode: PercentMode::Remaining,
            show_usage: true,
            show_seven_day: true,
            show_model_limits: true,
            show_reset_times: true,
            hidden_accounts: Vec::new(),
            accounts_refresh_secs: 60,
            usage_refresh_secs: 120,
            launch_at_login: false,
            low_quota_pct: 15,
            notify_low: true,
            notify_reset: true,
            check_updates: true,
            hotkeys: true,
        }
    }
}

fn path() -> PathBuf {
    crate::usage::data_dir().join("settings.json")
}

impl Prefs {
    pub fn load() -> Prefs {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let p = path();
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, text);
        }
    }

    pub fn is_hidden(&self, number: u32) -> bool {
        self.hidden_accounts.contains(&number)
    }

    pub fn toggle_hidden(&mut self, number: u32) {
        if let Some(i) = self.hidden_accounts.iter().position(|n| *n == number) {
            self.hidden_accounts.remove(i);
        } else {
            self.hidden_accounts.push(number);
        }
    }
}

// ---- Launch at login via a LaunchAgent plist ----------------------------

const AGENT_LABEL: &str = "com.miova.swapbar";

fn agent_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join("Library/LaunchAgents")
        .join(format!("{AGENT_LABEL}.plist"))
}

pub fn launch_agent_installed() -> bool {
    agent_path().exists()
}

pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    let p = agent_path();
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{AGENT_LABEL}</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><false/>
</dict></plist>
"#,
            exe.display()
        );
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, plist).map_err(|e| e.to_string())?;
    } else if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    Ok(())
}
