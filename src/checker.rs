// Design Decision: checker is a pure I/O function — it probes a service and
// returns the result. It has zero logging or observability concerns.
// All logging (debug!, info!, warn!) lives in run_check() in main.rs,
// which owns the state machine and therefore knows when a result is meaningful.

use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::config::{CheckMethod, ServiceConfig};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct CheckResult {
    pub healthy: bool,
    pub latency_ms: Option<u64>,
}

pub async fn check(service: &ServiceConfig, client: &reqwest::Client) -> CheckResult {
    match service.method {
        CheckMethod::Http => check_http(&service.url, client).await,
        CheckMethod::Tcp  => check_tcp(&service.url).await,
    }
}

// We consider any 2xx response healthy. 401/403 still means the process is
// alive; callers can tighten this by checking the status field if needed.
async fn check_http(url: &str, client: &reqwest::Client) -> CheckResult {
    let t = Instant::now();
    match client.get(url).send().await {
        Ok(resp) => CheckResult {
            healthy:    resp.status().is_success(),
            latency_ms: Some(t.elapsed().as_millis() as u64),
        },
        Err(_) => CheckResult { healthy: false, latency_ms: None },
    }
}

// For TCP we only need a successful connect — no bytes sent.
// This covers DB ports, VPN ports, and any raw TCP listener.
async fn check_tcp(addr: &str) -> CheckResult {
    let t = Instant::now();
    match timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => CheckResult {
            healthy:    true,
            latency_ms: Some(t.elapsed().as_millis() as u64),
        },
        _ => CheckResult { healthy: false, latency_ms: None },
    }
}
