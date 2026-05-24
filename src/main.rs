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
    Resize,
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
                Ok(Event::Resize(..)) if tx.send(UiEvent::Resize).is_err() => {
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

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    UiEvent::Key(key) => {
                        if key.code == KeyCode::Char('q') {
                            break;
                        }
                        app.handle_key(key);
                    }
                    UiEvent::Monitor(event) => {
                        app.log_messages.push(format_log(&event));
                        if app.log_messages.len() > 1000 {
                            app.log_messages.remove(0);
                        }
                        app.state.apply_event(event);
                    }
                    UiEvent::Resize => {}
                }
            }
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;
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
            let name = attrs.get("Name").map_or("?", |s| s.as_str());
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
