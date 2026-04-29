// Design Decision: We split the restart_command string on whitespace rather
// than passing it through a shell. Shell execution (sh -c "...") is simpler
// but opens command-injection risk if the config file is ever world-writable.
// Splitting avoids that while still supporting "systemctl restart foo".

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{debug, error};

pub async fn heal(service_name: &str, restart_command: &str) -> Result<()> {
    // Note: we do NOT log the start of execution at INFO — main.rs already
    // emits a `WARN Service DOWN — triggering auto-heal` line with both the
    // service name and the command, immediately before calling us. Logging
    // the same fact here doubled every outage entry in gatekeeper.log.

    let mut parts = restart_command.split_whitespace();
    let program = parts
        .next()
        .context("restart_command in config is empty")?;
    let args: Vec<&str> = parts.collect();

    let output = Command::new(program)
        .args(&args)
        .output()
        .await
        .with_context(|| format!("Failed to spawn '{restart_command}'"))?;

    if output.status.success() {
        // Successful heal is silent at INFO — the matching `Service RECOVERED`
        // line from the next poll cycle is the real signal that the heal worked.
        // Available at RUST_LOG=gatekeeper_rs=debug if you need to verify.
        debug!(service = service_name, "Heal command exited successfully");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Log but don't propagate — a failed heal is recoverable; we'll retry
        // next cycle. Propagating would kill the monitoring task for this service.
        error!(
            service = service_name,
            exit_code = ?output.status.code(),
            stderr = %stderr,
            "Heal command exited with error"
        );
    }

    Ok(())
}
