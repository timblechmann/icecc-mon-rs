// SPDX-License-Identifier: GPL-2.0-only
mod model;
mod monitor;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use std::io;
use std::net::SocketAddr;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(
    name = "icecc-mon",
    version,
    about = "TUI monitor for icecc distributed compilation"
)]
struct Args {
    #[arg(short, long)]
    scheduler: Option<String>,
    #[arg(short, long, default_value = "ICECREAM")]
    netname: String,
    #[arg(long)]
    anonymize: bool,
}

enum UiEvent {
    Key(crossterm::event::KeyEvent),
    Redraw,
    Monitor(model::MonitorEvent),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let scheduler_addr: Option<SocketAddr> = args
        .scheduler
        .as_deref()
        .map(|s| {
            if s.contains(':') {
                s.parse()
            } else {
                format!("{}:{}", s, icecc_protocol::DEFAULT_PORT).parse()
            }
        })
        .transpose()?;

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<UiEvent>();

    let (monitor_tx, mut monitor_rx) = mpsc::unbounded_channel::<model::MonitorEvent>();
    let netname = args.netname.clone();
    tokio::spawn(async move {
        monitor::run_monitor(scheduler_addr, &netname, monitor_tx).await;
    });

    let tx = event_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = monitor_rx.recv().await {
            if tx.send(UiEvent::Monitor(event)).is_err() {
                break;
            }
        }
    });

    let tx = event_tx.clone();
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(Event::Key(key))
                    if key.kind == KeyEventKind::Press && tx.send(UiEvent::Key(key)).is_err() =>
                {
                    break;
                }
                Ok(Event::Resize(..)) if tx.send(UiEvent::Redraw).is_err() => {
                    break;
                }
                _ => {}
            }
        }
    });

    drop(event_tx);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = ui::App::new(args.anonymize);
    terminal.draw(|f| ui::draw(f, &mut app))?;
    let mut last_redraw = tokio::time::Instant::now();
    let mut redraw_pending = false;
    let min_interval = tokio::time::Duration::from_millis(100);

    let process_event = |app: &mut ui::App, event: UiEvent| -> Result<bool> {
        match event {
            UiEvent::Key(key) => {
                if key.code == KeyCode::Char('q') {
                    return Ok(true);
                }
                app.handle_key(key);
            }
            UiEvent::Monitor(event) => {
                app.log_messages.push_back(format_log(&event));
                if app.log_messages.len() > 1000 {
                    app.log_messages.pop_front();
                }
                app.state.apply_event(event);
            }
            UiEvent::Redraw => {}
        }
        Ok(false)
    };

    loop {
        if redraw_pending {
            let elapsed = last_redraw.elapsed();
            let remaining = min_interval.saturating_sub(elapsed);
            tokio::select! {
                _ = tokio::time::sleep(remaining) => {
                    terminal.draw(|f| ui::draw(f, &mut app))?;
                    last_redraw = tokio::time::Instant::now();
                    redraw_pending = false;
                }
                Some(event) = event_rx.recv() => {
                    let is_immediate = matches!(event, UiEvent::Key(_) | UiEvent::Redraw);
                    if process_event(&mut app, event)? {
                        break;
                    }
                    if is_immediate {
                        terminal.draw(|f| ui::draw(f, &mut app))?;
                        last_redraw = tokio::time::Instant::now();
                        redraw_pending = false;
                    }
                }
            }
        } else {
            let Some(event) = event_rx.recv().await else {
                break;
            };
            let is_immediate = matches!(event, UiEvent::Key(_) | UiEvent::Redraw);
            let is_monitor = matches!(event, UiEvent::Monitor(_));
            if process_event(&mut app, event)? {
                break;
            }
            if is_immediate {
                terminal.draw(|f| ui::draw(f, &mut app))?;
                last_redraw = tokio::time::Instant::now();
            } else if is_monitor && last_redraw.elapsed() >= min_interval {
                terminal.draw(|f| ui::draw(f, &mut app))?;
                last_redraw = tokio::time::Instant::now();
            } else if is_monitor {
                redraw_pending = true;
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn format_log(event: &model::MonitorEvent) -> String {
    use model::MonitorEvent;
    match event {
        MonitorEvent::Connected {
            scheduler_name,
            netname,
        } => {
            format!(
                "Connected: scheduler={} netname={}",
                scheduler_name, netname
            )
        }
        MonitorEvent::Disconnected => "Disconnected".into(),
        MonitorEvent::HostStats { host_id, attrs } => {
            let name = attrs
                .iter()
                .find(|(k, _)| k == "Name")
                .map_or("?", |(_, v)| v.as_str());
            format!("HostStats: host={} name={}", host_id, name)
        }
        MonitorEvent::JobPending {
            job_id,
            client_id,
            filename,
        } => {
            format!(
                "JobPending: id={} client={} file={}",
                job_id, client_id, filename
            )
        }
        MonitorEvent::JobBegin { job_id, host_id } => {
            format!("JobBegin: id={} host={}", job_id, host_id)
        }
        MonitorEvent::LocalJobBegin {
            job_id,
            host_id,
            filename,
        } => {
            format!(
                "LocalJobBegin: id={} host={} file={}",
                job_id, host_id, filename
            )
        }
        MonitorEvent::JobDone { job_id } => {
            format!("JobDone: id={}", job_id)
        }
    }
}
