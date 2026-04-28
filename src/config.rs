// Design Decision: Separate config module keeps parsing concerns isolated from
// runtime logic. All override/merge logic lives here so main.rs never touches
// raw TOML — it just calls load_final() and gets a ready-to-use Config.

use anyhow::{Context, Result};
use serde::Deserialize;

// ── Core config types ─────────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub interval_secs: u64,
    pub discord_webhook_url: Option<String>,
    pub services: Vec<ServiceConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ServiceConfig {
    pub name: String,
    /// HTTP: full URL (e.g. "http://host:port/health") | TCP: "host:port"
    pub url: String,
    pub method: CheckMethod,
    pub restart_command: String,
}

// Design Decision: serde rename_all = "UPPERCASE" lets config authors write
// method = "HTTP" / "TCP" naturally without needing to match Rust variants.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum CheckMethod {
    Http,
    Tcp,
}

// ── Test override types ───────────────────────────────────────────────────────

// Design Decision: PartialConfig mirrors Config but every field is Option<T>.
// Missing fields in testconfig.toml silently fall back to config.toml values,
// so the test file only needs to declare what it wants to change.
#[derive(Deserialize, Debug, Default)]
struct PartialConfig {
    pub interval_secs: Option<u64>,
    pub discord_webhook_url: Option<String>,
    /// If present, replaces the *entire* services list from config.toml.
    /// Omit the [[services]] sections entirely to keep the base services.
    pub services: Option<Vec<ServiceConfig>>,
}

/// Returned by load_final so callers can surface the override status in the TUI.
pub struct LoadResult {
    pub config: Config,
    /// Optional startup message for the TUI Events panel (test mode warning, etc.)
    pub banner: Option<String>,
}

const TEST_CONFIG: &str = "testconfig.toml";

// ── Public API ────────────────────────────────────────────────────────────────

/// Primary entry point. Loads config.toml, then tries to apply testconfig.toml
/// overrides on top. Never returns an error for test config problems — those
/// are downgraded to a banner message so the program always starts.
pub fn load_final(base_path: &str) -> Result<LoadResult> {
    let base = load(base_path)?;

    // Fast path: no test file present
    if !std::path::Path::new(TEST_CONFIG).exists() {
        return Ok(LoadResult { config: base, banner: None });
    }

    // Try to read the file
    let raw = match std::fs::read_to_string(TEST_CONFIG) {
        Ok(s) => s,
        Err(e) => {
            return Ok(LoadResult {
                config: base,
                banner: Some(format!(
                    "⚠ testconfig.toml found but unreadable ({e}) — using config.toml"
                )),
            });
        }
    };

    // Try to parse — a syntax error must not crash the monitor
    let partial: PartialConfig = match toml::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            return Ok(LoadResult {
                config: base,
                banner: Some(format!(
                    "⚠ testconfig.toml parse error: {e} — falling back to config.toml"
                )),
            });
        }
    };

    // Merge: test file wins field-by-field; missing fields keep the base value
    let merged = Config {
        interval_secs:       partial.interval_secs.unwrap_or(base.interval_secs),
        discord_webhook_url: partial.discord_webhook_url.or(base.discord_webhook_url),
        services:            partial.services.unwrap_or(base.services),
    };

    let svc_count = merged.services.len();
    let interval  = merged.interval_secs;

    Ok(LoadResult {
        config: merged,
        banner: Some(format!(
            "⚠ TEST MODE — testconfig.toml active ({svc_count} services, poll every {interval}s)"
        )),
    })
}

/// Low-level loader used internally and in tests.
pub fn load(path: &str) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read config file '{path}'"))?;
    toml::from_str(&raw)
        .context("Failed to parse config — check syntax and required fields")
}
