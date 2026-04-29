// Design Decision: This module owns *all* rendering logic and the TUI data
// model. It has zero I/O side effects — it only transforms AppState into
// pixels. Keeping it side-effect free makes it easy to test and impossible
// for a render bug to crash the monitor.

use std::collections::VecDeque;
use std::time::Instant;

use chrono::Local;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table},
    Frame,
};

const MAX_LOGS: usize = 200;
const LOG_PANEL_LINES: u16 = 6;
// Ring-buffer depth for per-service latency history powering the sparklines
const LATENCY_HISTORY: usize = 20;
// Fixed width for the System Health sidebar (chars)
const SIDEBAR_WIDTH: u16 = 34;

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus { Unknown, Up, Down }

#[derive(Debug, Clone)]
pub struct ServiceRow {
    pub name: String,
    pub status: HealthStatus,
    pub latency_ms: Option<u64>,
    /// Rolling window of the last LATENCY_HISTORY response-time samples (ms).
    /// Consumed by the Sparkline widget each render cycle.
    pub latency_history: VecDeque<u64>,
    pub last_checked: String,
}

pub struct AppState {
    pub services: Vec<ServiceRow>,
    pub logs: VecDeque<String>,
    // System metrics — populated by the sysinfo background thread
    pub cpu_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub dry_run: bool,
    /// When true, monitoring continues but heals + Discord alerts are suppressed.
    /// Authoritative copy lives in main.rs as Arc<AtomicBool>; this mirrors it
    /// for rendering purposes.
    pub maintenance_mode: bool,
    /// Wall-clock instant of the next scheduled poll cycle. Updated by the
    /// checker task each tick; powers the "next Xs" countdown shown in the
    /// LATENCY column for DOWN services.
    pub next_poll_at: Option<Instant>,
    started_at: Instant,
}

impl AppState {
    pub fn new(names: &[String], dry_run: bool) -> Self {
        Self {
            services: names.iter().map(|n| ServiceRow {
                name: n.clone(),
                status: HealthStatus::Unknown,
                latency_ms: None,
                latency_history: VecDeque::with_capacity(LATENCY_HISTORY),
                last_checked: "–".to_string(),
            }).collect(),
            logs: VecDeque::with_capacity(MAX_LOGS),
            cpu_pct: 0.0,
            ram_used_mb: 0,
            ram_total_mb: 0,
            dry_run,
            maintenance_mode: false,
            next_poll_at: None,
            started_at: Instant::now(),
        }
    }

    pub fn set_next_poll(&mut self, at: Instant) {
        self.next_poll_at = Some(at);
    }

    /// Whole seconds remaining until the next poll cycle, rounded UP so the
    /// countdown never displays "0s" while a poll is still pending.
    /// Returns None if no schedule is known yet (first tick) or the deadline
    /// has already passed (probes are in flight).
    fn seconds_to_next_poll(&self) -> Option<u64> {
        let at  = self.next_poll_at?;
        let now = Instant::now();
        if at <= now { return None; }
        let d   = at - now;
        Some(d.as_secs() + if d.subsec_nanos() > 0 { 1 } else { 0 })
    }

    pub fn reload_services(&mut self, new_names: &[String]) {
        // Remove rows for services no longer in config
        self.services.retain(|row| new_names.contains(&row.name));
        // Add rows for newly configured services (Unknown status — no prior data)
        for name in new_names {
            if !self.services.iter().any(|r| &r.name == name) {
                self.services.push(ServiceRow {
                    name: name.clone(),
                    status: HealthStatus::Unknown,
                    latency_ms: None,
                    latency_history: VecDeque::with_capacity(LATENCY_HISTORY),
                    last_checked: "–".to_string(),
                });
            }
        }
        // Restore the order defined in the new config
        let order: std::collections::HashMap<&str, usize> =
            new_names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        self.services.sort_by_key(|r| order.get(r.name.as_str()).copied().unwrap_or(usize::MAX));
    }

    pub fn update_service(&mut self, name: &str, healthy: bool, latency_ms: Option<u64>) {
        if let Some(row) = self.services.iter_mut().find(|r| r.name == name) {
            row.status = if healthy { HealthStatus::Up } else { HealthStatus::Down };
            row.latency_ms = latency_ms;
            row.last_checked = Local::now().format("%H:%M:%S").to_string();
            // Append to latency ring buffer — only valid (connected) samples
            if let Some(ms) = latency_ms {
                if row.latency_history.len() >= LATENCY_HISTORY {
                    row.latency_history.pop_front();
                }
                row.latency_history.push_back(ms);
            }
        }
    }

    pub fn update_system_stats(&mut self, cpu_pct: f32, ram_used_mb: u64, ram_total_mb: u64) {
        self.cpu_pct = cpu_pct;
        self.ram_used_mb = ram_used_mb;
        self.ram_total_mb = ram_total_mb;
    }

    pub fn push_log(&mut self, msg: impl Into<String>) {
        if self.logs.len() >= MAX_LOGS {
            self.logs.pop_front();
        }
        self.logs.push_back(format!(" {} │ {}", Local::now().format("%H:%M:%S"), msg.into()));
    }

    fn health_ratio(&self) -> f64 {
        if self.services.is_empty() { return 1.0; }
        let up = self.services.iter().filter(|r| r.status == HealthStatus::Up).count();
        up as f64 / self.services.len() as f64
    }

    fn health_pct(&self) -> u16 { (self.health_ratio() * 100.0) as u16 }

    fn uptime(&self) -> String {
        let s = self.started_at.elapsed().as_secs();
        format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    }
}

// ── Layout ────────────────────────────────────────────────────────────────────
//
//  ┌─ Header ───────────────────────────────────────────────────────────────────┐
//  │ ┌─ Services Table (Fill) ─────────────────────┐ ┌─ System Health (34) ──┐ │
//  │ │ NAME | STATUS | LATENCY | LAST CHECK         │ │ CPU  ████░░  42%      │ │
//  │ └─────────────────────────────────────────────┘ │ RAM  ██████  68%      │ │
//  │ ┌─ Latency Sparklines ────────────────────────┐ └───────────────────────┘ │
//  │ │ Auth  ▁▂▃▄▃▂▁▂                              │                           │
//  │ │ VPN   ──────────                             │                           │
//  │ └─────────────────────────────────────────────┘                           │
//  ├─ Infrastructure Health gauge ──────────────────────────────────────────────┤
//  ├─ Events log ───────────────────────────────────────────────────────────────┤
//  └─ Status bar ───────────────────────────────────────────────────────────────┘

pub fn draw(frame: &mut Frame, state: &AppState) {
    // Sparkline panel height: 1 row per service + 2 for the block border
    let spark_height = state.services.len().max(1) as u16 + 2;

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                   // header
            Constraint::Min(5),                      // table + health sidebar
            Constraint::Length(spark_height),         // sparklines
            Constraint::Length(3),                   // infrastructure gauge
            Constraint::Length(LOG_PANEL_LINES + 2), // events
            Constraint::Length(1),                   // status bar
        ])
        .split(frame.area());

    // Middle row: table left, system health right
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(SIDEBAR_WIDTH)])
        .split(outer[1]);

    draw_header(frame, outer[0], state);
    draw_table(frame, middle[0], state);
    draw_system_health(frame, middle[1], state);
    draw_sparklines(frame, outer[2], state);
    draw_gauge(frame, outer[3], state);
    draw_logs(frame, outer[4], state);
    draw_statusbar(frame, outer[5]);
}

// ── Header ────────────────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let now = Local::now().format("%Y-%m-%d  %H:%M:%S").to_string();
    let mut spans = vec![
        Span::styled("  ● GATEKEEPER-RS", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("  ╱  ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} services", state.services.len()), Style::default().fg(Color::Gray)),
        Span::styled("  ╱  uptime ", Style::default().fg(Color::DarkGray)),
        Span::styled(state.uptime(), Style::default().fg(Color::Yellow)),
        Span::styled("  ╱  ", Style::default().fg(Color::DarkGray)),
        Span::styled(now, Style::default().fg(Color::White)),
    ];
    if state.dry_run {
        spans.push(Span::styled("  ╱  ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            "⚠ DRY RUN ACTIVE",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }
    if state.maintenance_mode {
        // Phase from uptime gives a 1Hz flash that works even on terminals that
        // ignore the SLOW_BLINK modifier (most modern emulators do). Alternates
        // bg between solid Yellow and bordered "off" each second.
        let on = state.started_at.elapsed().as_secs() % 2 == 0;
        let style = if on {
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK)
        };
        spans.push(Span::styled("  ╱  ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(" 🔧 MAINTENANCE MODE ACTIVE ", style));
    }

    // Border colour signals state at a glance: yellow during maintenance.
    let border_color = if state.maintenance_mode { Color::Yellow } else { Color::DarkGray };

    let line = Line::from(spans);
    frame.render_widget(
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color)),
            )
            .alignment(Alignment::Left),
        area,
    );
}

// ── Service table ─────────────────────────────────────────────────────────────

fn draw_table(frame: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec![
        Cell::from(" SERVICE").style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Cell::from("STATUS").style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Cell::from("LATENCY").style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Cell::from("LAST CHECK").style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]).height(1);

    let countdown = state.seconds_to_next_poll();

    let rows: Vec<Row> = state.services.iter().map(|svc| {
        let (status_text, status_color) = match svc.status {
            HealthStatus::Up      => ("● UP   ", Color::Green),
            HealthStatus::Down    => ("● DOWN ", Color::Red),
            HealthStatus::Unknown => ("○ ??   ", Color::DarkGray),
        };
        // For DOWN rows, the latency column is empty anyway — repurpose it as
        // a "next probe in Xs" countdown so the operator can see the polling
        // rhythm. heal fires inside that next probe if the state cycle calls
        // for it (UP→DOWN transition); otherwise it's just a re-check.
        let (latency_text, latency_color) = match svc.status {
            HealthStatus::Up => (
                svc.latency_ms.map(|ms| format!(" {ms}ms"))
                    .unwrap_or_else(|| "  –".to_string()),
                Color::Gray,
            ),
            HealthStatus::Down => (
                match countdown {
                    Some(s) => format!(" next {s}s"),
                    None    => " checking".to_string(),
                },
                Color::Yellow,
            ),
            HealthStatus::Unknown => ("  –".to_string(), Color::DarkGray),
        };
        Row::new(vec![
            Cell::from(format!(" {}", svc.name)),
            Cell::from(status_text).style(Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            Cell::from(latency_text).style(Style::default().fg(latency_color)),
            Cell::from(format!(" {}", svc.last_checked)).style(Style::default().fg(Color::DarkGray)),
        ])
    }).collect();

    frame.render_widget(
        Table::new(rows, [Constraint::Fill(1), Constraint::Length(10), Constraint::Length(10), Constraint::Length(14)])
            .header(header)
            .column_spacing(1)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(" Services ", Style::default().fg(Color::Cyan)))
            ),
        area,
    );
}

// ── Latency sparklines ────────────────────────────────────────────────────────

fn draw_sparklines(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Latency Sparklines (ms) ", Style::default().fg(Color::Cyan)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.services.is_empty() || inner.height == 0 { return; }

    let n = state.services.len();
    // One row per service — if more services than rows, clip silently
    let row_constraints: Vec<Constraint> = std::iter::repeat(Constraint::Length(1)).take(n).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    for (i, svc) in state.services.iter().enumerate() {
        if i >= rows.len() { break; }

        // Split each row: fixed-width name label | growing sparkline bar
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Fill(1)])
            .split(rows[i]);

        let color = match svc.status {
            HealthStatus::Up      => Color::Green,
            HealthStatus::Down    => Color::Red,
            HealthStatus::Unknown => Color::DarkGray,
        };

        frame.render_widget(
            Paragraph::new(format!(" {:<18}", &svc.name)).style(Style::default().fg(color)),
            cols[0],
        );

        // Design Decision: Collect into a Vec so we can hand a &[u64] to
        // Sparkline::data(). The borrow ends when render_widget consumes the
        // widget, before history is dropped at the end of this iteration.
        let history: Vec<u64> = svc.latency_history.iter().copied().collect();
        frame.render_widget(
            Sparkline::default().data(&history).style(Style::default().fg(color)),
            cols[1],
        );
    }
}

// ── System health sidebar ─────────────────────────────────────────────────────

fn draw_system_health(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Host Health ", Style::default().fg(Color::Cyan)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 { return; }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // CPU label
            Constraint::Length(1), // CPU gauge
            Constraint::Length(1), // spacer
            Constraint::Length(1), // RAM label
            Constraint::Length(1), // RAM gauge
            Constraint::Fill(1),   // padding
        ])
        .split(inner);

    // ── CPU ──────────────────────────────────────────────────────────────────
    let cpu_color = if state.cpu_pct > 80.0 { Color::Red }
        else if state.cpu_pct > 50.0 { Color::Yellow }
        else { Color::Green };

    frame.render_widget(
        Paragraph::new(" CPU").style(Style::default().fg(Color::Gray)),
        rows[0],
    );
    frame.render_widget(
        Gauge::default()
            .ratio((state.cpu_pct as f64 / 100.0).clamp(0.0, 1.0))
            .gauge_style(Style::default().fg(cpu_color).bg(Color::DarkGray))
            .label(Span::styled(
                format!("{:.1}%", state.cpu_pct),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
        rows[1],
    );

    // ── RAM ──────────────────────────────────────────────────────────────────
    let ram_ratio = if state.ram_total_mb > 0 {
        (state.ram_used_mb as f64 / state.ram_total_mb as f64).clamp(0.0, 1.0)
    } else { 0.0 };

    let ram_color = if ram_ratio > 0.85 { Color::Red }
        else if ram_ratio > 0.60 { Color::Yellow }
        else { Color::Green };

    frame.render_widget(
        Paragraph::new(format!(" RAM ({} MB total)", state.ram_total_mb))
            .style(Style::default().fg(Color::Gray)),
        rows[3],
    );
    frame.render_widget(
        Gauge::default()
            .ratio(ram_ratio)
            .gauge_style(Style::default().fg(ram_color).bg(Color::DarkGray))
            .label(Span::styled(
                format!("{} MB", state.ram_used_mb),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
        rows[4],
    );
}

// ── Infrastructure health gauge ───────────────────────────────────────────────

fn draw_gauge(frame: &mut Frame, area: Rect, state: &AppState) {
    let pct = state.health_pct();
    let color = match pct { 80..=100 => Color::Green, 40..=79 => Color::Yellow, _ => Color::Red };
    frame.render_widget(
        Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(" Infrastructure Health ", Style::default().fg(Color::Cyan)))
            )
            .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
            .ratio(state.health_ratio())
            .label(Span::styled(format!("{pct}%"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
        area,
    );
}

// ── Event log ─────────────────────────────────────────────────────────────────

fn draw_logs(frame: &mut Frame, area: Rect, state: &AppState) {
    let visible: Vec<ListItem> = state.logs.iter()
        .rev().take(LOG_PANEL_LINES as usize).rev()
        .map(|s| ListItem::new(s.as_str()).style(Style::default().fg(Color::Gray)))
        .collect();
    frame.render_widget(
        List::new(visible).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Events ", Style::default().fg(Color::Cyan)))
        ),
        area,
    );
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_statusbar(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled("Q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" quit   ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" force quit   ", Style::default().fg(Color::DarkGray)),
            Span::styled("R", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" reload config   ", Style::default().fg(Color::DarkGray)),
            Span::styled("M", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" toggle maintenance   ", Style::default().fg(Color::DarkGray)),
            Span::styled("logs/gatekeeper.*.log", Style::default().fg(Color::DarkGray)),
        ])),
        area,
    );
}
