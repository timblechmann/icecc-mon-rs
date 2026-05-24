// SPDX-License-Identifier: GPL-2.0-only
//! TUI rendering and input handling with ratatui.

use crate::model::{JobState, SchedulerState, SortColumn};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// Brightness-optimized grays (256-color). DarkGray(8) too dim on black.
const LABEL: Color = Color::Indexed(252); // labels like "Scheduler:"
const DIM: Color = Color::Indexed(245); // secondary values like pending count
const DETAIL: Color = Color::Indexed(248); // expanded host detail lines
const BAR_BG: Color = Color::Indexed(236); // row highlight bg, subtly off-black

/// Host color palette (16 distinct colors).
const HOST_COLORS: &[Color] = &[
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
    Color::Gray,
    Color::DarkGray,
    Color::Indexed(208), // orange
];

fn host_color(idx: u8) -> Color {
    HOST_COLORS[idx as usize % HOST_COLORS.len()]
}

/// App view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    Search,
    Log,
}

/// Application state for TUI.
pub struct App {
    pub state: SchedulerState,
    pub sort_col: SortColumn,
    pub sort_reverse: bool,
    pub selected: usize,
    pub expanded: std::collections::HashSet<u32>,
    pub expand_all: bool,
    pub anonymize: bool,
    pub view_mode: ViewMode,
    pub search_query: String,
    pub log_messages: Vec<String>,
    pub table_state: TableState,
}

impl App {
    pub fn new(anonymize: bool) -> Self {
        Self {
            state: SchedulerState::new(),
            sort_col: SortColumn::Name,
            sort_reverse: false,
            selected: 0,
            expanded: std::collections::HashSet::new(),
            expand_all: false,
            anonymize,
            view_mode: ViewMode::Table,
            search_query: String::new(),
            log_messages: Vec::new(),
            table_state: TableState::default(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.view_mode {
            ViewMode::Search => self.handle_search_key(key),
            ViewMode::Log => self.handle_log_key(key),
            ViewMode::Table => self.handle_table_key(key),
        }
    }

    #[allow(clippy::collapsible_if)]
    fn handle_table_key(&mut self, key: KeyEvent) {
        let host_count = self.state.hosts.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if host_count > 0 => {
                self.selected = (self.selected + 1).min(host_count - 1);
            }
            KeyCode::Char('k') | KeyCode::Up if self.selected > 0 => {
                self.selected -= 1;
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
                self.sort_col = self.sort_col.next();
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::BackTab => {
                self.sort_col = self.sort_col.prev();
            }
            KeyCode::Char('r') => {
                self.sort_reverse = !self.sort_reverse;
            }
            KeyCode::Char(' ') => {
                let ids = self.state.sorted_host_ids(self.sort_col, self.sort_reverse);
                if let Some(&id) = ids.get(self.selected) {
                    if !self.expanded.remove(&id) {
                        self.expanded.insert(id);
                    }
                }
            }
            KeyCode::Char('a') => {
                self.expand_all = !self.expand_all;
                if !self.expand_all {
                    self.expanded.clear();
                }
            }
            KeyCode::Char('/') => {
                self.view_mode = ViewMode::Search;
                self.search_query.clear();
            }
            KeyCode::Char('L') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.view_mode = ViewMode::Log;
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected = 0;
            }
            KeyCode::End | KeyCode::Char('G') if host_count > 0 => {
                self.selected = host_count - 1;
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.view_mode = ViewMode::Table;
                self.search_query.clear();
            }
            KeyCode::Enter => {
                self.view_mode = ViewMode::Table;
            }
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
            }
            _ => {}
        }
    }

    fn handle_log_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('L') => {
                self.view_mode = ViewMode::Table;
            }
            _ => {}
        }
    }

    fn anonymize_str(&self, s: &str) -> String {
        if !self.anonymize {
            return s.to_string();
        }
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        format!("host_{:04x}", hasher.finish() & 0xFFFF)
    }
}

/// Main draw function.
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // header
            Constraint::Min(10),   // table
            Constraint::Length(1), // status bar
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);

    match app.view_mode {
        ViewMode::Table | ViewMode::Search => draw_table(f, app, chunks[1]),
        ViewMode::Log => draw_log(f, app, chunks[1]),
    }

    draw_status_bar(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let state = &app.state;
    let status = if state.connected {
        "Connected"
    } else {
        "Disconnected"
    };

    let text = vec![
        Line::from(vec![
            Span::styled("Scheduler: ", Style::default().fg(LABEL)),
            Span::styled(
                &state.scheduler_name,
                Style::default().fg(Color::White).bold(),
            ),
            Span::raw("  "),
            Span::styled("Netname: ", Style::default().fg(LABEL)),
            Span::styled(&state.netname, Style::default().fg(Color::White).bold()),
            Span::raw("  "),
            Span::styled(
                status,
                Style::default().fg(if state.connected {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Servers: ", Style::default().fg(LABEL)),
            Span::styled(
                format!("Total:{}", state.hosts.len()),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                format!(
                    "Active:{}",
                    state
                        .hosts
                        .values()
                        .filter(|h| { !app.state.active_jobs_on_host(h.id).is_empty() })
                        .count()
                ),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("Total: ", Style::default().fg(LABEL)),
            Span::styled(
                format!("Remote:{}", state.total_remote),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Local:{}", state.total_local),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Jobs: ", Style::default().fg(LABEL)),
            Span::styled(
                format!("Active:{}", state.active_job_count()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Local:{}", state.local_job_count()),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Pending:{}", state.pending_job_count()),
                Style::default().fg(DIM),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(LABEL));
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

/// Compute the table row index for the host at `host_index` in `host_ids`,
/// accounting for expanded detail rows that precede it.
fn host_to_table_row(
    host_ids: &[u32],
    host_index: usize,
    expanded: &HashSet<u32>,
    expand_all: bool,
    state: &SchedulerState,
) -> usize {
    #[allow(clippy::collapsible_if)]
    let mut row = 0;
    for (i, &id) in host_ids.iter().enumerate() {
        if i >= host_index {
            break;
        }
        row += 1; // host row
        if (expand_all || expanded.contains(&id))
            && let Some(host) = state.hosts.get(&id)
        {
            row += state.active_jobs_on_host(id).len() + host.attrs.len();
        }
    }
    row
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let host_ids = app.state.sorted_host_ids(app.sort_col, app.sort_reverse);

    let host_ids: Vec<u32> = if app.search_query.is_empty() {
        host_ids
    } else {
        let q = app.search_query.to_lowercase();
        host_ids
            .into_iter()
            .filter(|id| {
                app.state.hosts.get(id).is_some_and(|h| {
                    h.name.to_lowercase().contains(&q)
                        || h.ip.contains(&q)
                        || h.platform.to_lowercase().contains(&q)
                })
            })
            .collect()
    };

    if !host_ids.is_empty() && app.selected >= host_ids.len() {
        app.selected = host_ids.len() - 1;
    }
    let row_idx = if host_ids.is_empty() {
        None
    } else {
        Some(host_to_table_row(
            &host_ids,
            app.selected,
            &app.expanded,
            app.expand_all,
            &app.state,
        ))
    };
    app.table_state.select(row_idx);

    let headers: Vec<Cell> = SortColumn::ALL
        .iter()
        .map(|col| {
            let label = if *col == app.sort_col {
                let arrow = if app.sort_reverse { "▼" } else { "▲" };
                format!("{} {}", col.header(), arrow)
            } else {
                col.header().to_string()
            };
            Cell::from(label).style(Style::default().fg(Color::Cyan).bold())
        })
        .chain(std::iter::once(
            Cell::from("JOBS").style(Style::default().fg(Color::Cyan).bold()),
        ))
        .collect();

    let header = Row::new(headers).height(1);

    let mut rows: Vec<Row> = Vec::new();
    for &host_id in &host_ids {
        let host = match app.state.hosts.get(&host_id) {
            Some(h) => h,
            None => continue,
        };
        let active = app.state.active_jobs_on_host(host_id);
        let cur = active.len();
        let color = host_color(host.color_idx);
        let name = app.anonymize_str(&host.name);

        let mut name_style = Style::default().fg(color);
        if host.no_remote {
            name_style = name_style.add_modifier(Modifier::UNDERLINED);
        }

        let bar_width = 20usize;
        let filled = if host.max_jobs > 0 {
            (cur * bar_width) / host.max_jobs as usize
        } else {
            0
        };
        let bar: String = format!(
            "[{}{}]",
            "━".repeat(filled.min(bar_width)),
            " ".repeat(bar_width.saturating_sub(filled))
        );

        let cells = vec![
            Cell::from(host_id.to_string()),
            Cell::from(name).style(name_style),
            Cell::from(host.total_in.to_string()),
            Cell::from(cur.to_string()).style(if cur > 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }),
            Cell::from(host.max_jobs.to_string()),
            Cell::from(host.total_out.to_string()),
            Cell::from(host.total_local.to_string()),
            Cell::from(host.speed.to_string()),
            Cell::from(bar).style(Style::default().fg(color)),
        ];

        rows.push(Row::new(cells));

        let is_expanded = app.expand_all || app.expanded.contains(&host_id);
        if is_expanded {
            for job in &active {
                let elapsed = job.start_time.elapsed().as_secs();
                let fname = app.anonymize_str(&job.filename);
                let source_color = app
                    .state
                    .hosts
                    .get(&job.client_id)
                    .map(|h| host_color(h.color_idx))
                    .unwrap_or(DIM);
                rows.push(
                    Row::new(vec![
                        Cell::from(""),
                        Cell::from(Line::from(vec![
                            Span::raw("  └─ "),
                            Span::styled(
                                format!(
                                    "[{}] ",
                                    match job.state {
                                        JobState::RemoteActive => "remote",
                                        JobState::LocalActive => "local",
                                        JobState::Pending => "pending",
                                    }
                                ),
                                Style::default().fg(source_color),
                            ),
                            Span::styled(format!("{}s ", elapsed), Style::default().fg(DETAIL)),
                            Span::styled(fname, Style::default().fg(DETAIL)),
                        ])),
                    ])
                    .height(1),
                );
            }
            for (k, v) in &host.attrs {
                let attr_line = format!("     {} = {}", k, v);
                rows.push(
                    Row::new(vec![
                        Cell::from(""),
                        Cell::from(attr_line).style(Style::default().fg(DETAIL)),
                    ])
                    .height(1),
                );
            }
        }
    }

    let widths = [
        Constraint::Length(5),  // ID
        Constraint::Min(15),    // NAME
        Constraint::Length(5),  // IN
        Constraint::Length(5),  // CUR
        Constraint::Length(5),  // MAX
        Constraint::Length(5),  // OUT
        Constraint::Length(6),  // LOCAL
        Constraint::Length(6),  // SPEED
        Constraint::Length(22), // JOBS bar
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().bg(BAR_BG))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_log(f: &mut Frame, app: &App, area: Rect) {
    let max_lines = (area.height as usize).saturating_sub(2);
    let lines: Vec<Line> = app
        .log_messages
        .iter()
        .rev()
        .take(max_lines)
        .rev()
        .map(|s| Line::from(s.as_str()))
        .collect();
    let block = Block::default()
        .title("Log (press Esc to close)")
        .borders(Borders::ALL);
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let text = match app.view_mode {
        ViewMode::Search => {
            format!("Search: {}█", app.search_query)
        }
        ViewMode::Log => "Log view | Esc: back".into(),
        ViewMode::Table => {
            "j/k:nav  h/l:sort  r:reverse  space:expand  a:all  /:search  L:log  q:quit".into()
        }
    };
    let bar = Paragraph::new(text).style(Style::default().fg(DIM).bg(Color::Black));
    f.render_widget(bar, area);
}
