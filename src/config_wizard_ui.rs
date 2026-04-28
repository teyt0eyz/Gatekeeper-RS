// config_wizard_ui.rs — Ratatui rendering for the menu-driven Setup Wizard.
// Pure read-only view over ConfigWizard state; zero side effects.
//
// Screen dispatch:
//   MainMenu       — List widget, 4 items, fits in 24 rows (11 min)
//   GlobalSettings — 2 input fields, fits in 24 rows (11 min)
//   ServiceList    — scrollable List, fits in 24 rows (6 min)
//   ServiceEdit(i) — 4 stacked fields, fits in 24 rows (17 min)
//
// Shared chrome per screen: header (3) + content + status (1) + footer (1).

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::config_wizard::{
    ConfigWizard, GlobalField, Screen, ServiceField, WizardStatus, MENU_ITEMS,
};

// ── Row-height constant ───────────────────────────────────────────────────────

/// Any bordered input block (top border + 1 content row + bottom border).
const FIELD_H: u16 = 3;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn render_wizard(frame: &mut Frame, wizard: &ConfigWizard) {
    match &wizard.screen {
        Screen::MainMenu           => render_main_menu(frame, frame.area(), wizard),
        Screen::GlobalSettings     => render_global_settings(frame, frame.area(), wizard),
        Screen::ServiceList        => render_service_list(frame, frame.area(), wizard),
        Screen::ServiceEdit(idx)   => render_service_edit(frame, frame.area(), wizard, *idx),
        Screen::Scanner            => render_scanner(frame, frame.area(), wizard),
    }
}

// ── Shared chrome helpers ─────────────────────────────────────────────────────

/// Screen-specific header banner (3 rows).
fn render_screen_header(frame: &mut Frame, area: Rect, screen_title: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  GATEKEEPER-RS",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  /  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                screen_title,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

/// Status strip (1 row, no border) — reads from wizard.status.
fn render_status_bar(frame: &mut Frame, area: Rect, wizard: &ConfigWizard) {
    let (icon, msg, color) = match &wizard.status {
        Some(WizardStatus::Saved) => (
            "✓",
            " Configuration saved to config.toml".to_string(),
            Color::Green,
        ),
        Some(WizardStatus::Error(e)) => ("✗", format!(" {e}"), Color::Red),
        Some(WizardStatus::Info(i))  => ("·", format!(" {i}"), Color::Yellow),
        None if wizard.has_errors()  => (
            "!",
            " Some fields need attention (highlighted in red)".to_string(),
            Color::Yellow,
        ),
        None => (
            "·",
            " Fill all required fields, then choose Save & Exit".to_string(),
            Color::DarkGray,
        ),
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(msg, Style::default().fg(color)),
        ])),
        area,
    );
}

/// Key-hint footer (1 row, no border).
/// `hints`: slice of (key, description) pairs — rendered as "KEY desc  │  KEY desc …"
fn render_footer_keys(frame: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(
            *key,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {desc}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Screen: Main Menu ─────────────────────────────────────────────────────────
//
//  ┌─ Header ──────────────────────────────────────────────── 3 ─┐
//  ┌─ Setup Wizard ────────────────────────────────────────── ? ─┐
//  │  > Edit Global Settings                                      │
//  │    Manage Services                                           │
//  │    Add New Service                                           │
//  │    Save & Exit                                               │
//  └──────────────────────────────────────────────────────────── ┘
//  · status ─────────────────────────────────────────────────── 1 │
//    hints ─────────────────────────────────────────────────── 1 ─┘
//  min rows: 3 + 6 + 1 + 1 = 11

fn render_main_menu(frame: &mut Frame, area: Rect, wizard: &ConfigWizard) {
    let n_items = MENU_ITEMS.len() as u16;
    let list_h  = n_items + 2; // border top + items + border bottom

    let min_rows = FIELD_H + list_h + 1 + 1;
    if area.height < min_rows {
        frame.render_widget(
            too_small_msg(area.height, min_rows),
            area,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(FIELD_H),  // header
            Constraint::Length(list_h),   // menu list
            Constraint::Min(0),           // absorb leftover
            Constraint::Length(1),        // status
            Constraint::Length(1),        // footer
        ])
        .split(area);

    render_screen_header(frame, chunks[0], "Setup Wizard");

    let items: Vec<ListItem> = MENU_ITEMS
        .iter()
        .map(|label| ListItem::new(format!("  {label}")))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Main Menu ",
                    Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default().with_selected(Some(wizard.menu_index));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    render_status_bar(frame, chunks[3], wizard);
    render_footer_keys(
        frame,
        chunks[4],
        &[
            ("↑/↓", "navigate"),
            ("Enter", "open"),
            ("S", "scan local"),
            ("Esc/q", "quit"),
        ],
    );
}

// ── Screen: Global Settings ───────────────────────────────────────────────────
//
//  ┌─ Header ──────────────────────────────────────────────── 3 ─┐
//  ┌─ Discord Webhook URL (optional) ──────────────────────── 3 ─┐
//  ┌─ Polling Interval (seconds) ──────────────────────────── 3 ─┐
//  [leftover]
//  · status ─────────────────────────────────────────────────── 1
//    hints ──────────────────────────────────────────────────── 1
//  min rows: 3 + 3 + 3 + 1 + 1 = 11

fn render_global_settings(frame: &mut Frame, area: Rect, wizard: &ConfigWizard) {
    let min_rows = FIELD_H * 3 + 1 + 1;
    if area.height < min_rows {
        frame.render_widget(too_small_msg(area.height, min_rows), area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(FIELD_H), // header
            Constraint::Length(FIELD_H), // discord webhook
            Constraint::Length(FIELD_H), // interval
            Constraint::Min(0),          // leftover
            Constraint::Length(1),       // status
            Constraint::Length(1),       // footer
        ])
        .split(area);

    render_screen_header(frame, chunks[0], "Global Settings");

    render_input(
        frame, chunks[1],
        "Discord Webhook URL (optional)",
        &wizard.discord_webhook,
        wizard.global_focus == GlobalField::DiscordWebhook,
        wizard.field_error("discord_webhook"),
    );
    render_input(
        frame, chunks[2],
        "Polling Interval (seconds)",
        &wizard.interval,
        wizard.global_focus == GlobalField::Interval,
        wizard.field_error("interval"),
    );

    render_status_bar(frame, chunks[4], wizard);
    render_footer_keys(
        frame,
        chunks[5],
        &[
            ("Tab/↓", "next field"),
            ("Shift+Tab/↑", "prev field"),
            ("Enter", "confirm"),
            ("Esc", "back"),
        ],
    );
}

// ── Screen: Service List ──────────────────────────────────────────────────────
//
//  ┌─ Header ──────────────────────────────────────────────── 3 ─┐
//  ┌─ Services ─────────────────────────────────────────────  ? ─┐
//  │  > web-api                                                   │
//  │    worker                                                    │
//  └──────────────────────────────────────────────────────────── ┘
//  · status ─────────────────────────────────────────────────── 1
//    hints ──────────────────────────────────────────────────── 1
//  min rows: 3 + 3 + 1 + 1 = 8 (1 service)

fn render_service_list(frame: &mut Frame, area: Rect, wizard: &ConfigWizard) {
    let min_rows = FIELD_H + FIELD_H + 1 + 1; // header + 1-item list + status + footer
    if area.height < min_rows {
        frame.render_widget(too_small_msg(area.height, min_rows), area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(FIELD_H), // header
            Constraint::Min(1),          // list (grows)
            Constraint::Length(1),       // status
            Constraint::Length(1),       // footer
        ])
        .split(area);

    render_screen_header(frame, chunks[0], "Manage Services");

    let items: Vec<ListItem> = if wizard.services.is_empty() {
        vec![ListItem::new("  (no services yet)").style(Style::default().fg(Color::DarkGray))]
    } else {
        wizard.services.iter().enumerate().map(|(i, svc)| {
            let label = if svc.name.trim().is_empty() {
                format!("  #{} — (unnamed)", i + 1)
            } else {
                format!("  #{} — {}", i + 1, svc.name)
            };
            ListItem::new(label)
        }).collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Services  (Enter=edit  D=delete) ",
                    Style::default().fg(Color::Gray),
                )),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let selected = if wizard.services.is_empty() { None } else { Some(wizard.list_index) };
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, chunks[1], &mut state);

    render_status_bar(frame, chunks[2], wizard);
    render_footer_keys(
        frame,
        chunks[3],
        &[
            ("↑/↓", "navigate"),
            ("Enter", "edit"),
            ("D", "delete"),
            ("S", "scan local"),
            ("Esc", "back"),
        ],
    );
}

// ── Screen: Service Edit ──────────────────────────────────────────────────────
//
//  ┌─ Header ──────────────────────────────────────────────── 3 ─┐
//  ┌─ Svc #N — Name ───────────────────────────────────────── 3 ─┐
//  ┌─ URL / Address ───────────────────────────────────────── 3 ─┐
//  ┌─ Method  ←/→ ─────────────────────────────────────────── 3 ─┐
//  ┌─ Restart Command ─────────────────────────────────────── 3 ─┐
//  [leftover]
//  · status ─────────────────────────────────────────────────── 1
//    hints ──────────────────────────────────────────────────── 1
//  min rows: 3 + 3×4 + 1 + 1 = 17

fn render_service_edit(frame: &mut Frame, area: Rect, wizard: &ConfigWizard, idx: usize) {
    let min_rows = FIELD_H + FIELD_H * 4 + 1 + 1;
    if area.height < min_rows {
        frame.render_widget(too_small_msg(area.height, min_rows), area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(FIELD_H), // header
            Constraint::Length(FIELD_H), // name
            Constraint::Length(FIELD_H), // url
            Constraint::Length(FIELD_H), // method
            Constraint::Length(FIELD_H), // restart command
            Constraint::Min(0),          // leftover
            Constraint::Length(1),       // status
            Constraint::Length(1),       // footer
        ])
        .split(area);

    let title = format!("Edit Service #{}", idx + 1);
    render_screen_header(frame, chunks[0], &title);

    // Guard against out-of-bounds (can happen briefly during deletion)
    if idx >= wizard.services.len() {
        return;
    }
    let svc = &wizard.services[idx];

    render_input(
        frame, chunks[1],
        &format!("Name  (svc #{})", idx + 1),
        &svc.name,
        wizard.edit_focus == ServiceField::Name,
        wizard.field_error(&format!("svc_{idx}_name")),
    );
    render_input(
        frame, chunks[2],
        "URL / Address",
        &svc.url,
        wizard.edit_focus == ServiceField::Url,
        wizard.field_error(&format!("svc_{idx}_url")),
    );
    render_method(
        frame, chunks[3],
        &svc.method,
        wizard.edit_focus == ServiceField::Method,
    );
    render_input(
        frame, chunks[4],
        "Restart Command",
        &svc.restart_command,
        wizard.edit_focus == ServiceField::RestartCommand,
        wizard.field_error(&format!("svc_{idx}_cmd")),
    );

    render_status_bar(frame, chunks[6], wizard);
    render_footer_keys(
        frame,
        chunks[7],
        &[
            ("Tab/↓", "next field"),
            ("Shift+Tab/↑", "prev field"),
            ("←/→/Space", "toggle method"),
            ("Enter", "confirm"),
            ("Esc", "back"),
        ],
    );
}

// ── Generic text-input block ──────────────────────────────────────────────────
//
// Colours:
//   focused + no error  → Cyan  border & title
//   error present       → Red   border & title (error appended to title)
//   unfocused, valid    → DarkGray border, Gray title

fn render_input(
    frame:   &mut Frame,
    area:    Rect,
    title:   &str,
    value:   &str,
    focused: bool,
    error:   Option<&str>,
) {
    let (border_col, title_col) = field_colors(focused, error.is_some());

    let full_title = match error {
        Some(msg) => format!(" {title}  ✗ {msg} "),
        None      => format!(" {title} "),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(
            full_title,
            Style::default().fg(title_col).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Clip from the left so the tail of a long string is always visible
    let max_w = inner.width as usize;
    let start = value.len().saturating_sub(max_w);
    let shown = &value[start..];

    frame.render_widget(
        Paragraph::new(shown).style(Style::default().fg(Color::White)),
        inner,
    );

    // Place the real terminal cursor after the last character when focused
    if focused {
        let cx = (inner.x + shown.len() as u16).min(inner.x + inner.width.saturating_sub(1));
        frame.set_cursor_position(Position { x: cx, y: inner.y });
    }
}

// ── Method toggle (HTTP ↔ TCP) ────────────────────────────────────────────────

fn render_method(frame: &mut Frame, area: Rect, method: &str, focused: bool) {
    let border_col = if focused { Color::Cyan } else { Color::DarkGray };
    let title_col  = if focused { Color::Cyan } else { Color::Gray };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Span::styled(
            " Method  ←/→ ",
            Style::default().fg(title_col).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let radio = |label: &str, active: bool| -> Span<'static> {
        let (bullet, color) = if active {
            ("◉", Color::Green)
        } else {
            ("○", Color::DarkGray)
        };
        Span::styled(
            format!("{bullet} {label}  "),
            Style::default()
                .fg(color)
                .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() }),
        )
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            radio("HTTP", method == "HTTP"),
            radio("TCP",  method == "TCP"),
        ])),
        inner,
    );
}

// ── Screen: Service Scanner ───────────────────────────────────────────────────
//
//  ┌─ Header ──────────────────────────────────────────────── 3 ─┐
//  ┌─ Discovered Services (N) ─────────────────────────────── ? ─┐
//  │  > nginx.service   — A high performance web server …        │
//  │    redis.service   — Advanced key-value store              │
//  │    …                                                         │
//  └──────────────────────────────────────────────────────────── ┘
//  · status ─────────────────────────────────────────────────── 1
//    hints ──────────────────────────────────────────────────── 1
//  min rows: 3 + 3 + 1 + 1 = 8

fn render_scanner(frame: &mut Frame, area: Rect, wizard: &ConfigWizard) {
    let min_rows = FIELD_H + FIELD_H + 1 + 1;
    if area.height < min_rows {
        frame.render_widget(too_small_msg(area.height, min_rows), area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(FIELD_H), // header
            Constraint::Min(1),          // list (grows)
            Constraint::Length(1),       // status
            Constraint::Length(1),       // footer
        ])
        .split(area);

    render_screen_header(frame, chunks[0], "Local Service Scanner");

    let items: Vec<ListItem> = wizard
        .scan_results
        .iter()
        .map(|r| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<32}", r.name),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  — {}", r.description),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let title = format!(
        " Discovered Services ({})  Enter=pick ",
        wizard.scan_results.len()
    );

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    title,
                    Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let selected = if wizard.scan_results.is_empty() {
        None
    } else {
        Some(wizard.scan_index)
    };
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, chunks[1], &mut state);

    render_status_bar(frame, chunks[2], wizard);
    render_footer_keys(
        frame,
        chunks[3],
        &[
            ("↑/↓", "navigate"),
            ("Enter", "pick"),
            ("Esc/q", "cancel"),
        ],
    );
}

// ── Colour helpers ────────────────────────────────────────────────────────────

fn field_colors(focused: bool, has_error: bool) -> (Color, Color) {
    match (focused, has_error) {
        (_, true)     => (Color::Red,      Color::Red),
        (true, false) => (Color::Cyan,     Color::Cyan),
        _             => (Color::DarkGray, Color::Gray),
    }
}

// ── Terminal-too-small fallback ───────────────────────────────────────────────

fn too_small_msg(current: u16, needed: u16) -> Paragraph<'static> {
    Paragraph::new(format!(
        " Terminal too small ({current} rows). Please resize to at least {needed} rows."
    ))
    .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
}
