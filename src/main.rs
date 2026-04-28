// Gatekeeper-RS — infrastructure health monitor with TUI + persistent logging
//
// Data flow:
//   sysinfo thread  ─────────────────────────────────────────────────────────┐
//   checker tasks (tokio::spawn × N services)  ──────── mpsc::Sender<UiUpdate>
//   ──────────────────────────────────────────────────────────────────────────┘
//                                                               ↓
//   TUI event loop (tokio::select!):
//     ├─ crossterm EventStream   → keyboard / resize
//     ├─ mpsc rx                 → service checks, system stats, log lines
//     └─ 1-second tick           → header clock refresh
//
// Shutdown sequence (Q / Ctrl+C / SIGTERM):
//   break loop → Discord "offline" alert → terminal restore → log flush → exit

mod checker;
mod config;
mod healer;
mod notifier;
mod tui;

use anyhow::{Context, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

// ── MPSC message types ────────────────────────────────────────────────────────

enum UiUpdate {
    ServiceChecked { name: String, healthy: bool, latency_ms: Option<u64> },
    Log(String),
    // Sent by the sysinfo thread every ~2 seconds
    SystemStats { cpu_pct: f32, ram_used_mb: u64, ram_total_mb: u64 },
}

#[derive(Debug, Clone, PartialEq)]
enum ServiceState { Up, Down }

type StateMap = Arc<Mutex<HashMap<String, ServiceState>>>;

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // ── Persistent rolling log files ──────────────────────────────────────────
    // Design Decision: tracing-appender's non_blocking writer puts log events
    // into an in-memory queue flushed by a background thread, so the async
    // runtime is never stalled by disk I/O. The _guard *must* stay alive until
    // the end of main — dropping it triggers the final flush to disk.
    std::fs::create_dir_all("logs").context("Failed to create logs/ directory")?;

    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("gatekeeper")
        .filename_suffix("log")
        .build("logs")
        .context("Failed to create rolling log appender")?;

    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking_writer)
        .with_ansi(false) // plain text — no ANSI codes in log files
        // Design Decision: scope the default filter to our own crate only.
        // "gatekeeper_rs=info" means:  our code  → INFO and above  (transitions, heals, alerts)
        //                              everything else → off (no reqwest/hyper/h2 noise)
        // Override at runtime:  RUST_LOG=gatekeeper_rs=debug  for verbose detail
        //                       RUST_LOG=debug               for full third-party traces
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("gatekeeper_rs=info")),
        )
        .init();

    // ── CLI args ──────────────────────────────────────────────────────────────
    let cli_args: Vec<String> = std::env::args().collect();
    let dry_run = cli_args.iter().any(|a| a == "--dry-run");
    let config_path = cli_args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "config.toml".to_string());

    // ── Config ────────────────────────────────────────────────────────────────
    let load_result = config::load_final(&config_path)
        .with_context(|| format!("Could not load config from '{config_path}'"))?;
    let cfg            = load_result.config;
    let startup_banner = load_result.banner;

    info!(services = cfg.services.len(), "Gatekeeper-RS starting");
    if dry_run {
        info!("Dry-run mode active — heal commands will be simulated, not executed");
    }

    // ── Terminal setup ────────────────────────────────────────────────────────
    // Design Decision: panic hook runs *before* the default hook so the terminal
    // is restored to a usable state before the panic message is printed.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;
    terminal.clear().context("Failed to clear terminal")?;

    // Always restore the terminal, even if run_app returns an error.
    let result = run_app(&mut terminal, cfg, startup_banner, config_path, dry_run).await;

    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    // _guard is dropped here — this blocks briefly while the logging thread
    // flushes remaining buffered events to disk before the process exits.
    info!("Gatekeeper-RS shutdown complete");
    drop(_guard);

    println!("✓ Cleanup complete. Goodbye!");
    result
}

// ── Application loop ──────────────────────────────────────────────────────────

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    cfg: config::Config,
    startup_banner: Option<String>,
    config_path: String,
    dry_run: bool,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<UiUpdate>(512);

    let service_names: Vec<String> = cfg.services.iter().map(|s| s.name.clone()).collect();
    let mut app = tui::AppState::new(&service_names, dry_run);

    if let Some(banner) = startup_banner {
        app.push_log(banner);
    }

    let state_map: StateMap = Arc::new(Mutex::new(HashMap::new()));
    // Shared config — updated by the 'r' hot-reload handler, read by the checker task
    let shared_cfg = Arc::new(RwLock::new(cfg));

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("Failed to build HTTP client")?;

    // ── System-stats thread ───────────────────────────────────────────────────
    // Design Decision: sysinfo::System is !Send on macOS (uses Mach/IOKit
    // internals). We run it on a dedicated OS thread via std::thread::spawn
    // rather than tokio::spawn to satisfy the Send bound. The tokio mpsc
    // Sender is Send, so blocking_send() bridges the std thread to the async
    // world cleanly without an extra channel layer.
    {
        let tx_sys = tx.clone();
        std::thread::spawn(move || {
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                // Two refresh calls with a short gap give an accurate CPU %.
                // A single call returns the delta since the *last* refresh;
                // on the very first call there is no prior sample, so we take
                // two quickly to bootstrap an accurate reading.
                sys.refresh_cpu_usage();
                std::thread::sleep(std::time::Duration::from_millis(200));
                sys.refresh_cpu_usage();
                sys.refresh_memory();

                let update = UiUpdate::SystemStats {
                    cpu_pct:      sys.global_cpu_info().cpu_usage(),
                    ram_used_mb:  sys.used_memory()  / 1_048_576,
                    ram_total_mb: sys.total_memory() / 1_048_576,
                };
                if tx_sys.blocking_send(update).is_err() {
                    break; // receiver dropped — TUI has exited
                }
            }
        });
    }

    // ── Background health-check task ──────────────────────────────────────────
    // Reads config through shared_cfg so a hot-reload is picked up on the next tick.
    {
        let tx = tx.clone();
        let state_map = Arc::clone(&state_map);
        let http_client = http_client.clone();
        let shared_cfg_checker = Arc::clone(&shared_cfg);
        tokio::spawn(async move {
            let mut current_interval = shared_cfg_checker.read().await.interval_secs;
            let mut tick = interval(Duration::from_secs(current_interval));
            loop {
                tick.tick().await;
                let cfg = shared_cfg_checker.read().await.clone();
                // Recreate the tick timer when interval_secs changes after a reload
                if cfg.interval_secs != current_interval {
                    current_interval = cfg.interval_secs;
                    tick = interval(Duration::from_secs(current_interval));
                }
                let mut handles = Vec::with_capacity(cfg.services.len());
                for svc in &cfg.services {
                    let svc = svc.clone();
                    let client = http_client.clone();
                    let states = Arc::clone(&state_map);
                    let webhook = cfg.discord_webhook_url.clone();
                    let tx = tx.clone();
                    handles.push(tokio::spawn(async move {
                        if let Err(e) = run_check(svc, client, states, webhook, tx, dry_run).await {
                            error!("Check error: {e:#}");
                        }
                    }));
                }
                for h in handles {
                    if let Err(e) = h.await { error!("Task panicked: {e}"); }
                }
            }
        });
    }

    // ── TUI event loop ────────────────────────────────────────────────────────
    let mut crossterm_events = EventStream::new();
    let mut clock_tick = interval(Duration::from_secs(1));

    // Design Decision: tokio::select! does not support #[cfg(...)] on individual
    // branches — the macro parser sees it as a syntax error. Instead we build a
    // single boxed future *before* the loop that either resolves on SIGTERM (Unix)
    // or stays pending forever (Windows/WASM). The select! branch is then plain
    // Rust with no conditional compilation inside the macro.
    let mut sigterm: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> = {
        #[cfg(unix)]
        {
            let mut sig = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::terminate()
            ).context("Failed to install SIGTERM handler")?;
            Box::pin(async move { sig.recv().await; })
        }
        #[cfg(not(unix))]
        {
            Box::pin(std::future::pending::<()>())
        }
    };

    terminal.draw(|f| tui::draw(f, &app))?;

    loop {
        tokio::select! {
            // ── Keyboard / terminal resize ────────────────────────────────────
            maybe_ev = crossterm_events.next() => {
                match maybe_ev {
                    Some(Ok(Event::Key(key))) => {
                        let quit =
                            key.code == KeyCode::Char('q') ||
                            key.code == KeyCode::Char('Q') ||
                            (key.code == KeyCode::Char('c') &&
                             key.modifiers.contains(KeyModifiers::CONTROL));
                        if quit {
                            info!("User requested shutdown");
                            break;
                        }
                        if key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R') {
                            match config::load_final(&config_path) {
                                Ok(load_result) => {
                                    let new_cfg = load_result.config;
                                    let new_names: Vec<String> = new_cfg.services.iter()
                                        .map(|s| s.name.clone()).collect();
                                    app.reload_services(&new_names);
                                    // Drop stale state-map entries so removed services
                                    // don't trigger spurious transitions on re-add
                                    {
                                        let name_set: std::collections::HashSet<&str> =
                                            new_names.iter().map(String::as_str).collect();
                                        state_map.lock().await
                                            .retain(|k, _| name_set.contains(k.as_str()));
                                    }
                                    *shared_cfg.write().await = new_cfg;
                                    app.push_log("Configuration reloaded successfully!".to_string());
                                    if let Some(banner) = load_result.banner {
                                        app.push_log(banner);
                                    }
                                    info!("Config hot-reloaded by user");
                                }
                                Err(e) => {
                                    let msg = format!("Config reload FAILED: {e:#}");
                                    app.push_log(msg.clone());
                                    error!("{msg}");
                                }
                            }
                        }
                    }
                    Some(Err(e)) => error!("Terminal event error: {e}"),
                    None => break,
                    _ => {}
                }
                terminal.draw(|f| tui::draw(f, &app))?;
            }

            // ── MPSC updates from checker / sysinfo tasks ─────────────────────
            maybe_update = rx.recv() => {
                match maybe_update {
                    Some(UiUpdate::ServiceChecked { name, healthy, latency_ms }) => {
                        app.update_service(&name, healthy, latency_ms);
                    }
                    Some(UiUpdate::Log(msg)) => {
                        app.push_log(msg);
                    }
                    Some(UiUpdate::SystemStats { cpu_pct, ram_used_mb, ram_total_mb }) => {
                        app.update_system_stats(cpu_pct, ram_used_mb, ram_total_mb);
                    }
                    None => break,
                }
                terminal.draw(|f| tui::draw(f, &app))?;
            }

            // ── SIGTERM — resolves on Unix, stays pending on other platforms ───
            _ = &mut sigterm => {
                info!("Received SIGTERM — initiating graceful shutdown");
                break;
            }

            // ── 1-second tick — keep header clock current ─────────────────────
            _ = clock_tick.tick() => {
                terminal.draw(|f| tui::draw(f, &app))?;
            }
        }
    }

    // ── Graceful shutdown ─────────────────────────────────────────────────────
    // Send a final Discord alert so the ops team knows monitoring has stopped.
    let discord_url = shared_cfg.read().await.discord_webhook_url.clone();
    if let Some(ref url) = discord_url {
        info!("Sending offline notification to Discord");
        let msg = ":skull: **Gatekeeper-RS** is going **OFFLINE**. Services are no longer monitored.";
        if let Err(e) = notifier::send_discord(url, msg).await {
            error!("Offline Discord notification failed: {e:#}");
        }
    }

    Ok(())
}

// ── Per-service health check + heal orchestration ─────────────────────────────

async fn run_check(
    svc: config::ServiceConfig,
    client: reqwest::Client,
    state_map: StateMap,
    webhook: Option<String>,
    tx: mpsc::Sender<UiUpdate>,
    dry_run: bool,
) -> Result<()> {
    let result = checker::check(&svc, &client).await;

    // Single observation point for raw check results.
    // debug! is filtered out by the default "gatekeeper_rs=info" filter, so
    // this only appears when the operator sets RUST_LOG=gatekeeper_rs=debug.
    // No logging of this data happens anywhere else — checker.rs is silent.
    debug!(
        service = %svc.name,
        healthy = result.healthy,
        latency_ms = ?result.latency_ms,
        "poll result"
    );

    // Always push the raw result so latency and last-checked update every cycle
    let _ = tx.send(UiUpdate::ServiceChecked {
        name: svc.name.clone(),
        healthy: result.healthy,
        latency_ms: result.latency_ms,
    }).await;

    let new_state = if result.healthy { ServiceState::Up } else { ServiceState::Down };

    // Design Decision: Lock → read prev → write new → release *before* any await.
    // Holding a tokio::Mutex across an await serialises every other task that
    // needs the state map, turning concurrent checks into a serial queue.
    let (transitioned, prev_state) = {
        let mut map = state_map.lock().await;
        let prev = map.get(&svc.name).cloned();
        let changed = prev.as_ref() != Some(&new_state);
        if changed { map.insert(svc.name.clone(), new_state.clone()); }
        (changed, prev)
    };

    if !transitioned { return Ok(()); }

    match new_state {
        ServiceState::Down => {
            warn!(service = %svc.name, cmd = %svc.restart_command, "Service DOWN — triggering auto-heal");
            let _ = tx.send(UiUpdate::Log(format!("{} → DOWN  heal: {}", svc.name, svc.restart_command))).await;

            if let Some(ref url) = webhook {
                let msg = format!(":red_circle: **{}** is DOWN. Auto-heal triggered. (`{}`)", svc.name, svc.restart_command);
                match notifier::send_discord(url, &msg).await {
                    Ok(_) => {
                        info!(service = %svc.name, "Discord DOWN alert sent");
                        let _ = tx.send(UiUpdate::Log(format!("Discord notified: {} DOWN", svc.name))).await;
                    }
                    Err(e) => error!(service = %svc.name, "Discord notify failed: {e:#}"),
                }
            }
            if dry_run {
                info!(service = %svc.name, cmd = %svc.restart_command, "[DRY-RUN] heal skipped");
                let _ = tx.send(UiUpdate::Log(
                    format!("[DRY-RUN] Would have executed: {}", svc.restart_command)
                )).await;
            } else {
                healer::heal(&svc.name, &svc.restart_command).await?;
            }
        }

        ServiceState::Up => {
            let latency = result.latency_ms.map(|ms| format!(" ({ms}ms)")).unwrap_or_default();
            if prev_state.is_some() {
                info!(service = %svc.name, "Service RECOVERED");
                let _ = tx.send(UiUpdate::Log(format!("{} → RECOVERED{}", svc.name, latency))).await;

                if let Some(ref url) = webhook {
                    let msg = format!(":green_circle: **{}** has RECOVERED.", svc.name);
                    match notifier::send_discord(url, &msg).await {
                        Ok(_) => {
                            info!(service = %svc.name, "Discord RECOVERED alert sent");
                            let _ = tx.send(UiUpdate::Log(format!("Discord notified: {} RECOVERED", svc.name))).await;
                        }
                        Err(e) => error!(service = %svc.name, "Discord notify failed: {e:#}"),
                    }
                }
            } else {
                info!(service = %svc.name, "Initial check: UP");
                let _ = tx.send(UiUpdate::Log(format!("{} → UP (initial){}", svc.name, latency))).await;
            }
        }
    }

    Ok(())
}
