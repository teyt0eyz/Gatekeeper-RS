// config_wizard.rs — Menu-driven state machine for the Setup Wizard.
//
// Screen flow:
//
//   MainMenu ──► GlobalSettings  (Edit Global Settings)
//            ──► ServiceList     (Manage Services)
//                  └──► ServiceEdit(i)  (Enter / edit a specific service)
//            ──► ServiceEdit(new)       (Add New Service → jumps straight to edit)
//            ──► save + quit            (Save & Exit)
//
// Navigation is handled per-screen so each sub-form fits in a 24-row terminal.

use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ── Service form ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ServiceForm {
    pub name:            String,
    pub url:             String,
    /// Always "HTTP" or "TCP" (uppercase).
    pub method:          String,
    pub restart_command: String,
}

impl Default for ServiceForm {
    fn default() -> Self {
        Self {
            name:            String::new(),
            url:             String::new(),
            method:          "HTTP".into(),
            restart_command: String::new(),
        }
    }
}

impl ServiceForm {
    pub fn toggle_method(&mut self) {
        self.method = if self.method == "HTTP" { "TCP".into() } else { "HTTP".into() };
    }
}

// ── Service-discovery types ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Full systemd unit name including ".service" suffix, e.g. "nginx.service".
    pub name:        String,
    pub description: String,
}

// ── Screen / focus types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    GlobalSettings,
    ServiceList,
    ServiceEdit(usize),
    Scanner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalField { DiscordWebhook, Interval }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceField { Name, Url, Method, RestartCommand }

// ── Wizard status ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum WizardStatus {
    Saved,
    Info(String),
    Error(String),
}

// ── Main menu item labels (index is the canonical ID) ─────────────────────────

pub const MENU_ITEMS: &[&str] = &[
    "Edit Global Settings",
    "Manage Services",
    "Add New Service",
    "Save & Exit",
];

// ── Main wizard state ─────────────────────────────────────────────────────────

pub struct ConfigWizard {
    // ── Config data ───────────────────────────────────────────────────────────
    pub discord_webhook: String,
    pub interval:        String,
    pub services:        Vec<ServiceForm>,

    // ── Navigation state ──────────────────────────────────────────────────────
    pub screen:       Screen,
    pub menu_index:   usize,       // 0 .. MENU_ITEMS.len()-1
    pub list_index:   usize,       // cursor in ServiceList
    pub global_focus: GlobalField, // active field in GlobalSettings
    pub edit_focus:   ServiceField,// active field in ServiceEdit

    // ── Scanner state ─────────────────────────────────────────────────────────
    pub scan_results:    Vec<ScanResult>,
    pub scan_index:      usize,
    /// Where to return on Esc from the Scanner screen.
    pub scanner_return:  Screen,

    // ── Feedback ──────────────────────────────────────────────────────────────
    pub errors:      HashMap<String, String>,
    pub status:      Option<WizardStatus>,
    pub should_quit: bool,
}

impl ConfigWizard {
    // ── Constructors ──────────────────────────────────────────────────────────

    pub fn new() -> Self {
        Self {
            discord_webhook: String::new(),
            interval:        "30".into(),
            services:        vec![ServiceForm::default()],
            screen:          Screen::MainMenu,
            menu_index:      0,
            list_index:      0,
            global_focus:    GlobalField::DiscordWebhook,
            edit_focus:      ServiceField::Name,
            scan_results:    Vec::new(),
            scan_index:      0,
            scanner_return:  Screen::MainMenu,
            errors:          HashMap::new(),
            status:          None,
            should_quit:     false,
        }
    }

    /// Pre-populate from an existing config file (edit-in-place workflow).
    pub fn load_from_file(path: &str) -> Self {
        let mut w = Self::new();
        let Ok(cfg) = crate::config::load(path) else { return w; };
        w.interval        = cfg.interval_secs.to_string();
        w.discord_webhook = cfg.discord_webhook_url.unwrap_or_default();
        if !cfg.services.is_empty() {
            w.services = cfg.services.into_iter().map(|s| ServiceForm {
                name:            s.name,
                url:             s.url,
                method:          match s.method {
                    crate::config::CheckMethod::Http => "HTTP".into(),
                    crate::config::CheckMethod::Tcp  => "TCP".into(),
                },
                restart_command: s.restart_command,
            }).collect();
        }
        w
    }

    // ── Public keyboard dispatcher ────────────────────────────────────────────

    /// Route every `KeyEvent` from the TUI loop to the active screen handler.
    pub fn handle_input(&mut self, key: KeyEvent) {
        // Dismiss transient info/error banners on the next keypress
        if matches!(self.status, Some(WizardStatus::Error(_) | WizardStatus::Info(_))) {
            self.status = None;
        }

        let screen = self.screen.clone();
        match screen {
            Screen::MainMenu           => self.on_main_menu(key),
            Screen::GlobalSettings     => self.on_global_settings(key),
            Screen::ServiceList        => self.on_service_list(key),
            Screen::ServiceEdit(idx)   => self.on_service_edit(key, idx),
            Screen::Scanner            => self.on_scanner(key),
        }
    }

    // ── Screen: Main Menu ─────────────────────────────────────────────────────

    fn on_main_menu(&mut self, key: KeyEvent) {
        let last = MENU_ITEMS.len() - 1;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu_index = self.menu_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.menu_index < last { self.menu_index += 1; }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                match self.menu_index {
                    0 => {
                        self.global_focus = GlobalField::DiscordWebhook;
                        self.screen = Screen::GlobalSettings;
                    }
                    1 => {
                        self.list_index = self.list_index
                            .min(self.services.len().saturating_sub(1));
                        self.screen = Screen::ServiceList;
                    }
                    2 => {
                        self.services.push(ServiceForm::default());
                        let new_idx = self.services.len() - 1;
                        self.list_index = new_idx;
                        self.edit_focus = ServiceField::Name;
                        self.screen = Screen::ServiceEdit(new_idx);
                    }
                    3 => self.save_action(),
                    _ => {}
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') => self.start_scan(),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    // ── Screen: Global Settings ───────────────────────────────────────────────

    fn on_global_settings(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.validate_interval();
                self.screen = Screen::MainMenu;
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.global_focus = GlobalField::DiscordWebhook;
                } else {
                    self.global_focus = GlobalField::Interval;
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.global_focus = GlobalField::DiscordWebhook;
            }
            KeyCode::Down => {
                self.global_focus = GlobalField::Interval;
            }
            KeyCode::Enter => match self.global_focus {
                GlobalField::DiscordWebhook => {
                    self.global_focus = GlobalField::Interval;
                }
                GlobalField::Interval => {
                    self.validate_interval();
                    self.screen = Screen::MainMenu;
                }
            },
            KeyCode::Backspace => match self.global_focus {
                GlobalField::DiscordWebhook => { self.discord_webhook.pop(); }
                GlobalField::Interval       => { self.interval.pop(); }
            },
            KeyCode::Char(c) => match self.global_focus {
                GlobalField::DiscordWebhook => self.discord_webhook.push(c),
                GlobalField::Interval => {
                    if c.is_ascii_digit() { self.interval.push(c); }
                }
            },
            _ => {}
        }
    }

    // ── Screen: Service List ──────────────────────────────────────────────────

    fn on_service_list(&mut self, key: KeyEvent) {
        let n = self.services.len();
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::MainMenu;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.list_index = self.list_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if n > 0 && self.list_index < n - 1 { self.list_index += 1; }
            }
            KeyCode::Enter => {
                if n > 0 {
                    self.edit_focus = ServiceField::Name;
                    self.screen = Screen::ServiceEdit(self.list_index);
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if n > 1 {
                    self.services.remove(self.list_index);
                    if self.list_index >= self.services.len() {
                        self.list_index = self.services.len() - 1;
                    }
                    self.status = Some(WizardStatus::Info("Service deleted.".into()));
                } else {
                    self.status = Some(WizardStatus::Error(
                        "Cannot delete the last service.".into(),
                    ));
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') => self.start_scan(),
            _ => {}
        }
    }

    // ── Screen: Service Edit ──────────────────────────────────────────────────

    fn on_service_edit(&mut self, key: KeyEvent, idx: usize) {
        match key.code {
            KeyCode::Esc => {
                self.errors.retain(|k, _| !k.starts_with(&format!("svc_{idx}_")));
                self.screen = Screen::ServiceList;
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.edit_focus = prev_svc_field(&self.edit_focus);
                } else {
                    self.edit_focus = next_svc_field(&self.edit_focus);
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.edit_focus = prev_svc_field(&self.edit_focus);
            }
            KeyCode::Down => {
                self.edit_focus = next_svc_field(&self.edit_focus);
            }
            KeyCode::Enter => match self.edit_focus {
                ServiceField::RestartCommand => {
                    self.validate_service(idx);
                    if !self.errors.keys().any(|k| k.starts_with(&format!("svc_{idx}_"))) {
                        self.screen = Screen::ServiceList;
                    }
                }
                _ => self.edit_focus = next_svc_field(&self.edit_focus),
            },
            KeyCode::Left | KeyCode::Right => {
                if self.edit_focus == ServiceField::Method && idx < self.services.len() {
                    self.services[idx].toggle_method();
                }
            }
            KeyCode::Char(' ') => {
                if self.edit_focus == ServiceField::Method && idx < self.services.len() {
                    self.services[idx].toggle_method();
                } else {
                    self.push_svc_char(idx, ' ');
                }
            }
            KeyCode::Backspace => self.pop_svc_char(idx),
            KeyCode::Char(c)   => self.push_svc_char(idx, c),
            _ => {}
        }
    }

    fn push_svc_char(&mut self, idx: usize, c: char) {
        let Some(svc) = self.services.get_mut(idx) else { return; };
        match self.edit_focus {
            ServiceField::Name           => svc.name.push(c),
            ServiceField::Url            => svc.url.push(c),
            ServiceField::RestartCommand => svc.restart_command.push(c),
            ServiceField::Method         => {}
        }
    }

    fn pop_svc_char(&mut self, idx: usize) {
        let Some(svc) = self.services.get_mut(idx) else { return; };
        match self.edit_focus {
            ServiceField::Name           => { svc.name.pop(); }
            ServiceField::Url            => { svc.url.pop(); }
            ServiceField::RestartCommand => { svc.restart_command.pop(); }
            ServiceField::Method         => {}
        }
    }

    // ── Screen: Scanner (local service discovery) ────────────────────────────

    /// Run `systemctl list-units …` and, if it succeeds, switch to Scanner.
    /// Falls back to a friendly status message on non-Linux/missing systemctl.
    fn start_scan(&mut self) {
        self.scanner_return = self.screen.clone();
        self.scan_results.clear();
        self.scan_index = 0;

        let result = Command::new("systemctl")
            .args([
                "list-units",
                "--type=service",
                "--state=running",
                "--no-pager",
                "--plain",
            ])
            .output();

        match result {
            // Binary not found / not executable → almost certainly macOS or Windows
            Err(_) => {
                self.status = Some(WizardStatus::Error(
                    "Scanner only available on Linux/Systemd.".into(),
                ));
            }
            Ok(out) if !out.status.success() => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let trimmed = stderr.trim();
                self.status = Some(WizardStatus::Error(if trimmed.is_empty() {
                    "Scanner only available on Linux/Systemd.".into()
                } else {
                    format!("systemctl failed: {trimmed}")
                }));
            }
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                self.scan_results = parse_systemctl_output(&stdout);
                if self.scan_results.is_empty() {
                    self.status = Some(WizardStatus::Info(
                        "No running .service units discovered.".into(),
                    ));
                } else {
                    self.screen = Screen::Scanner;
                }
            }
        }
    }

    fn on_scanner(&mut self, key: KeyEvent) {
        let n = self.scan_results.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.screen = self.scanner_return.clone();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scan_index = self.scan_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if n > 0 && self.scan_index < n - 1 {
                    self.scan_index += 1;
                }
            }
            KeyCode::Enter => self.pick_scanned_service(),
            _ => {}
        }
    }

    /// Append a new service pre-filled from the highlighted scan result and
    /// jump straight into its edit form (URL focused — the only field the
    /// scanner can't infer).
    fn pick_scanned_service(&mut self) {
        let Some(picked) = self.scan_results.get(self.scan_index).cloned() else {
            return;
        };
        let display_name = picked
            .name
            .strip_suffix(".service")
            .unwrap_or(&picked.name)
            .to_string();

        let form = ServiceForm {
            name:            display_name.clone(),
            url:             String::new(),
            method:          "HTTP".into(),
            restart_command: format!("sudo systemctl restart {}", picked.name),
        };
        self.services.push(form);
        let new_idx = self.services.len() - 1;
        self.list_index = new_idx;
        self.edit_focus = ServiceField::Url;
        self.screen = Screen::ServiceEdit(new_idx);
        self.status = Some(WizardStatus::Info(format!(
            "Imported '{display_name}' — fill in URL/Address.",
        )));
    }

    // ── Validation ────────────────────────────────────────────────────────────

    pub fn validate(&mut self) -> bool {
        self.errors.clear();
        self.validate_interval();
        let n = self.services.len();
        for i in 0..n { self.validate_service(i); }
        self.errors.is_empty()
    }

    fn validate_interval(&mut self) {
        match self.interval.trim().parse::<u64>() {
            Ok(n) if n > 0 => { self.errors.remove("interval"); }
            _ => {
                self.errors.insert(
                    "interval".into(),
                    "Must be a positive integer (e.g. 30)".into(),
                );
            }
        }
    }

    fn validate_service(&mut self, idx: usize) {
        let Some(svc) = self.services.get(idx).cloned() else { return; };
        let set = |map: &mut HashMap<String, String>, key: &str, msg: &str| {
            map.insert(key.to_string(), msg.to_string());
        };
        let rm = |map: &mut HashMap<String, String>, key: &str| { map.remove(key); };

        if svc.name.trim().is_empty() { set(&mut self.errors, &format!("svc_{idx}_name"), "Required"); }
        else { rm(&mut self.errors, &format!("svc_{idx}_name")); }

        if svc.url.trim().is_empty() { set(&mut self.errors, &format!("svc_{idx}_url"), "Required"); }
        else { rm(&mut self.errors, &format!("svc_{idx}_url")); }

        if svc.restart_command.trim().is_empty() { set(&mut self.errors, &format!("svc_{idx}_cmd"), "Required"); }
        else { rm(&mut self.errors, &format!("svc_{idx}_cmd")); }
    }

    // ── Save ──────────────────────────────────────────────────────────────────

    fn save_action(&mut self) {
        if !self.validate() {
            let n = self.errors.len();
            self.status = Some(WizardStatus::Error(
                format!("{n} field(s) need attention — check each sub-screen"),
            ));
            return;
        }
        match self.write_config("config.toml") {
            Ok(()) => {
                self.status = Some(WizardStatus::Saved);
                self.should_quit = true;
            }
            Err(e) => {
                self.status = Some(WizardStatus::Error(format!("Write failed: {e:#}")));
            }
        }
    }

    pub fn write_config(&self, path: &str) -> Result<()> {
        #[derive(serde::Serialize)]
        struct ServiceOut {
            name: String, url: String, method: String, restart_command: String,
        }
        #[derive(serde::Serialize)]
        struct ConfigOut {
            interval_secs: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            discord_webhook_url: Option<String>,
            services: Vec<ServiceOut>,
        }

        let interval_secs: u64 = self.interval.trim()
            .parse().context("interval must be a positive integer")?;

        let discord_webhook_url = if self.discord_webhook.trim().is_empty() {
            None
        } else {
            Some(self.discord_webhook.trim().to_string())
        };

        let services = self.services.iter().map(|s| ServiceOut {
            name:            s.name.trim().to_string(),
            url:             s.url.trim().to_string(),
            method:          s.method.to_uppercase(),
            restart_command: s.restart_command.trim().to_string(),
        }).collect();

        let out = ConfigOut { interval_secs, discord_webhook_url, services };
        let text = toml::to_string_pretty(&out)
            .context("Failed to serialise config to TOML")?;
        std::fs::write(path, text)
            .with_context(|| format!("Failed to write '{path}'"))
    }

    // ── Renderer helpers ──────────────────────────────────────────────────────

    pub fn field_error(&self, key: &str) -> Option<&str> {
        self.errors.get(key).map(String::as_str)
    }

    pub fn has_errors(&self) -> bool { !self.errors.is_empty() }

    pub fn is_saved(&self) -> bool { matches!(self.status, Some(WizardStatus::Saved)) }
}

// ── Focus-order helpers ───────────────────────────────────────────────────────

fn next_svc_field(f: &ServiceField) -> ServiceField {
    match f {
        ServiceField::Name           => ServiceField::Url,
        ServiceField::Url            => ServiceField::Method,
        ServiceField::Method         => ServiceField::RestartCommand,
        ServiceField::RestartCommand => ServiceField::RestartCommand, // Enter exits
    }
}

fn prev_svc_field(f: &ServiceField) -> ServiceField {
    match f {
        ServiceField::Name           => ServiceField::Name,
        ServiceField::Url            => ServiceField::Name,
        ServiceField::Method         => ServiceField::Url,
        ServiceField::RestartCommand => ServiceField::Method,
    }
}

// ── systemctl output parsing ──────────────────────────────────────────────────
//
// Expected `systemctl list-units --plain` columns:
//   UNIT                LOAD   ACTIVE SUB     DESCRIPTION
//   nginx.service       loaded active running A high performance web server …
//
// We discard header / footer noise by validating the row shape:
//   col 0 ends in ".service" AND col 1 == "loaded"

fn parse_systemctl_output(stdout: &str) -> Vec<ScanResult> {
    stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5            { return None; }
            if !parts[0].ends_with(".service") { return None; }
            if parts[1] != "loaded"       { return None; }
            Some(ScanResult {
                name:        parts[0].to_string(),
                description: parts[4..].join(" "),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }
    fn enter() -> KeyEvent { KeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()) }

    // ── 1.1 Wizard input validation ─────────────────────────────────────────

    #[test]
    fn interval_rejects_zero() {
        let mut w = ConfigWizard::new();
        w.interval = "0".into();
        w.validate_interval();
        assert!(w.errors.contains_key("interval"));
    }

    #[test]
    fn interval_rejects_non_numeric() {
        let mut w = ConfigWizard::new();
        w.interval = "abc".into();
        w.validate_interval();
        assert!(w.errors.contains_key("interval"));
    }

    #[test]
    fn interval_rejects_empty() {
        let mut w = ConfigWizard::new();
        w.interval.clear();
        w.validate_interval();
        assert!(w.errors.contains_key("interval"));
    }

    #[test]
    fn interval_accepts_positive() {
        let mut w = ConfigWizard::new();
        w.interval = "30".into();
        w.validate_interval();
        assert!(!w.errors.contains_key("interval"));
    }

    #[test]
    fn interval_keystroke_filters_letters() {
        // In GlobalSettings/Interval, only digits should append to the buffer.
        let mut w = ConfigWizard::new();
        w.screen        = Screen::GlobalSettings;
        w.global_focus  = GlobalField::Interval;
        w.interval.clear();
        w.handle_input(key('a'));
        assert_eq!(w.interval, "");
        w.handle_input(key('5'));
        assert_eq!(w.interval, "5");
        w.handle_input(key('q'));
        assert_eq!(w.interval, "5");
    }

    #[test]
    fn empty_service_fields_flagged() {
        let mut w = ConfigWizard::new();
        // default ServiceForm has all empty strings
        w.validate_service(0);
        assert!(w.errors.contains_key("svc_0_name"));
        assert!(w.errors.contains_key("svc_0_url"));
        assert!(w.errors.contains_key("svc_0_cmd"));
    }

    #[test]
    fn filled_service_passes_validation() {
        let mut w = ConfigWizard::new();
        w.services[0].name            = "nginx".into();
        w.services[0].url             = "http://localhost".into();
        w.services[0].restart_command = "echo ok".into();
        w.validate_service(0);
        assert!(!w.errors.contains_key("svc_0_name"));
        assert!(!w.errors.contains_key("svc_0_url"));
        assert!(!w.errors.contains_key("svc_0_cmd"));
    }

    // ── Menu navigation ──────────────────────────────────────────────────────

    #[test]
    fn add_service_menu_creates_new_form() {
        let mut w = ConfigWizard::new();
        let before = w.services.len();
        w.menu_index = 2; // "Add New Service"
        w.handle_input(enter());
        assert_eq!(w.services.len(), before + 1);
        assert!(matches!(w.screen, Screen::ServiceEdit(_)));
    }

    #[test]
    fn delete_keeps_at_least_one_service() {
        let mut w = ConfigWizard::new();
        w.screen = Screen::ServiceList;
        // Default has 1 service. D should refuse and emit an Error status.
        w.handle_input(key('d'));
        assert_eq!(w.services.len(), 1);
        assert!(matches!(w.status, Some(WizardStatus::Error(_))));
    }

    // ── 2.3 / 4.x Scanner ────────────────────────────────────────────────────

    #[test]
    fn parse_systemctl_extracts_name_and_description() {
        let stdout = "\
UNIT                    LOAD   ACTIVE SUB     DESCRIPTION
nginx.service           loaded active running A high performance web server
redis.service           loaded active running Advanced key-value store
3 loaded units listed.";
        let out = parse_systemctl_output(stdout);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "nginx.service");
        assert!(out[0].description.contains("high performance"));
        assert_eq!(out[1].name, "redis.service");
    }

    #[test]
    fn parse_systemctl_skips_header_and_footer() {
        // Header rows lack ".service" + "loaded" → should be filtered.
        let stdout = "\
UNIT  LOAD  ACTIVE  SUB  DESCRIPTION
0 loaded units listed.";
        assert_eq!(parse_systemctl_output(stdout).len(), 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn scanner_shows_friendly_error_when_systemctl_missing() {
        // On macOS Command::new("systemctl") fails to spawn, which must
        // surface as a friendly status, NOT a panic / crash.
        let mut w = ConfigWizard::new();
        w.start_scan();
        match &w.status {
            Some(WizardStatus::Error(msg)) => {
                assert!(
                    msg.contains("Linux") || msg.contains("Systemd"),
                    "unexpected error: {msg}"
                );
            }
            other => panic!("expected Error status, got {other:?}"),
        }
        // Must also NOT switch into the Scanner screen
        assert!(!matches!(w.screen, Screen::Scanner));
    }
}
