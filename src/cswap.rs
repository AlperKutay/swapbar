//! Bridge to the `cswap` CLI (claude-swap). cswap owns credentials, the
//! Anthropic usage API cadence (rate limited to ~30 req/h/account) and the
//! switch transaction, so we never reimplement any of it here.

use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub pct: f64,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub countdown: Option<String>,
    #[serde(default)]
    pub clock: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub expected_pct: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Spend {
    pub used: f64,
    pub limit: f64,
    pub pct: f64,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub five_hour: Option<Window>,
    #[serde(default)]
    pub seven_day: Option<Window>,
    #[serde(default)]
    pub scoped: Vec<Window>,
    #[serde(default)]
    pub spend: Option<Spend>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub number: u32,
    pub email: String,
    #[serde(default)]
    pub organization_name: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub usage_status: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub usage_age_seconds: Option<f64>,
    /// Plan label ("Max 20x", "Premium", …), read from cswap's saved Claude profile.
    #[serde(skip)]
    pub plan: Option<String>,
}

impl Account {
    pub fn org_tag(&self) -> String {
        match &self.organization_name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => "personal".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountList {
    #[serde(default)]
    pub active_account_number: Option<u32>,
    #[serde(default)]
    pub accounts: Vec<Account>,
}

fn cswap_bin() -> String {
    if let Ok(p) = std::env::var("CSWAP_BIN") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    for candidate in [
        format!("{home}/.local/bin/cswap"),
        "/opt/homebrew/bin/cswap".to_string(),
        "/usr/local/bin/cswap".to_string(),
    ] {
        if std::path::Path::new(&candidate).exists() {
            return candidate;
        }
    }
    "cswap".to_string()
}

fn run(args: &[&str]) -> Result<String, String> {
    let out = Command::new(cswap_bin())
        .args(args)
        .output()
        .map_err(|e| format!("cswap başlatılamadı: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if stderr.is_empty() {
            stdout.trim().to_string()
        } else {
            stderr
        };
        return Err(format!("cswap {} hata: {}", args.join(" "), msg));
    }
    Ok(stdout)
}

pub fn list() -> Result<AccountList, String> {
    let raw = run(&["list", "--json"])?;
    let mut list: AccountList =
        serde_json::from_str(&raw).map_err(|e| format!("cswap list çıktısı okunamadı: {e}"))?;
    for acc in &mut list.accounts {
        acc.plan = plan_for(acc.number);
    }
    Ok(list)
}

// ---- Plan labels ---------------------------------------------------------

/// The `oauthAccount` block Claude Code keeps in its config; cswap saves one
/// copy per account as `~/.claude-swap-backup/configs/.claude-config-<n>-<email>.json`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OauthAccount {
    #[serde(default)]
    organization_type: Option<String>,
    #[serde(default)]
    organization_rate_limit_tier: Option<String>,
    #[serde(default)]
    user_rate_limit_tier: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SavedConfig {
    #[serde(default)]
    oauth_account: Option<OauthAccount>,
}

fn plan_for(number: u32) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = std::path::Path::new(&home)
        .join(".claude-swap-backup")
        .join("configs");
    let prefix = format!(".claude-config-{number}-");
    let path = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".json"))
        })?;
    let cfg: SavedConfig = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    plan_label(&cfg.oauth_account?)
}

fn plan_label(a: &OauthAccount) -> Option<String> {
    let tier = |t: &Option<String>| t.as_deref().unwrap_or("").to_string();
    let max = |t: &str| {
        if t.contains("20x") {
            "Max 20x"
        } else if t.contains("5x") {
            "Max 5x"
        } else {
            "Max"
        }
    };
    let label = match a.organization_type.as_deref()? {
        "claude_max" => max(&tier(&a.organization_rate_limit_tier)),
        "claude_pro" => "Pro",
        // Team and Enterprise seats: a Premium seat carries a per-user Max rate-limit tier.
        "claude_team" | "claude_enterprise" => {
            if tier(&a.user_rate_limit_tier).contains("max") {
                "Premium"
            } else {
                "Standard"
            }
        }
        _ => return None,
    };
    Some(label.to_string())
}

pub fn switch(number: u32) -> Result<(), String> {
    run(&["switch", &number.to_string(), "--json"]).map(|_| ())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    pub key: String,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub is_set: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigList {
    #[serde(default)]
    pub settings: Vec<ConfigEntry>,
}

pub fn config_list() -> Result<ConfigList, String> {
    let raw = run(&["config", "--json"])?;
    serde_json::from_str(&raw).map_err(|e| format!("cswap config çıktısı okunamadı: {e}"))
}

pub fn config_set(key: &str, value: &str) -> Result<(), String> {
    run(&["config", "set", key, value]).map(|_| ())
}

pub fn config_unset(key: &str) -> Result<(), String> {
    run(&["config", "unset", key]).map(|_| ())
}

pub fn set_enabled(number: u32, enabled: bool) -> Result<(), String> {
    run(&[
        if enabled { "enable" } else { "disable" },
        &number.to_string(),
    ])
    .map(|_| ())
}
