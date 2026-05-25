// SPDX-License-Identifier: GPL-2.0-only
//! Data model: Host, Job, SchedulerState, MonitorEvent.

use compact_str::CompactString;
use std::time::Instant;

/// Event sent from the monitor task to the TUI.
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    Connected {
        scheduler_name: CompactString,
        netname: CompactString,
    },
    Disconnected,
    HostStats {
        host_id: u32,
        attrs: Vec<(CompactString, CompactString)>,
    },
    JobPending {
        job_id: u32,
        client_id: u32,
        filename: String,
    },
    JobBegin {
        job_id: u32,
        host_id: u32,
    },
    LocalJobBegin {
        job_id: u32,
        host_id: u32,
        filename: String,
    },
    JobDone {
        job_id: u32,
    },
}

#[derive(Debug, Clone)]
pub struct Host {
    pub id: u32,
    pub name: CompactString,
    pub max_jobs: u32,
    pub speed: u32,
    pub platform: CompactString,
    pub no_remote: bool,
    pub ip: CompactString,
    pub attrs: Vec<(CompactString, CompactString)>,
    pub total_in: u32,
    pub total_out: u32,
    pub total_local: u32,
    pub color_idx: u8,
}

impl Host {
    pub fn new(id: u32, color_idx: u8) -> Self {
        Self {
            id,
            name: CompactString::new(""),
            max_jobs: 0,
            speed: 0,
            platform: CompactString::new(""),
            no_remote: false,
            ip: CompactString::new(""),
            attrs: Vec::new(),
            total_in: 0,
            total_out: 0,
            total_local: 0,
            color_idx,
        }
    }

    pub fn update_attrs(&mut self, attrs: Vec<(CompactString, CompactString)>) {
        for (k, v) in &attrs {
            if k == "Name" {
                self.name = v.clone();
            } else if k == "MaxJobs" {
                self.max_jobs = v.parse().unwrap_or(0);
            } else if k == "Speed" {
                self.speed = v.parse::<f64>().unwrap_or(0.0) as u32;
            } else if k == "Platform" {
                self.platform = v.clone();
            } else if k == "NoRemote" {
                self.no_remote = v == "true" || v == "1";
            } else if k == "IP" {
                self.ip = v.clone();
            }
        }
        self.attrs = attrs;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    RemoteActive,
    LocalActive,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub client_id: u32,
    pub host_id: u32,
    pub filename: String,
    pub state: JobState,
    pub start_time: Instant,
}

/// Aggregate state of the scheduler.
#[derive(Debug)]
pub struct SchedulerState {
    pub hosts: std::collections::HashMap<u32, Host>,
    pub jobs: std::collections::HashMap<u32, Job>,
    pub scheduler_name: CompactString,
    pub netname: CompactString,
    pub connected: bool,
    pub total_remote: u64,
    pub total_local: u64,
    next_color: u8,
}

impl Default for SchedulerState {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            hosts: std::collections::HashMap::new(),
            jobs: std::collections::HashMap::new(),
            scheduler_name: CompactString::new(""),
            netname: CompactString::new(""),
            connected: false,
            total_remote: 0,
            total_local: 0,
            next_color: 0,
        }
    }

    pub fn apply_event(&mut self, event: MonitorEvent) {
        match event {
            MonitorEvent::Connected {
                scheduler_name,
                netname,
            } => {
                self.scheduler_name = scheduler_name;
                self.netname = netname;
                self.connected = true;
            }
            MonitorEvent::Disconnected => {
                self.connected = false;
                self.hosts.clear();
                self.jobs.clear();
            }
            MonitorEvent::HostStats { host_id, attrs } => {
                // No Name → host is dead, remove
                if !attrs.iter().any(|(k, _)| k == "Name") {
                    self.hosts.remove(&host_id);
                    return;
                }
                let color = self.next_color;
                let host = self.hosts.entry(host_id).or_insert_with(|| {
                    self.next_color = self.next_color.wrapping_add(1);
                    Host::new(host_id, color)
                });
                host.update_attrs(attrs);
            }
            MonitorEvent::JobPending {
                job_id,
                client_id,
                filename,
            } => {
                self.jobs.insert(
                    job_id,
                    Job {
                        client_id,
                        host_id: 0,
                        filename,
                        state: JobState::Pending,
                        start_time: Instant::now(),
                    },
                );
            }
            MonitorEvent::JobBegin { job_id, host_id } => {
                if let Some(job) = self.jobs.get_mut(&job_id) {
                    job.host_id = host_id;
                    job.state = JobState::RemoteActive;
                    job.start_time = Instant::now();
                    if let Some(host) = self.hosts.get_mut(&host_id) {
                        host.total_in += 1;
                    }
                    if let Some(host) = self.hosts.get_mut(&job.client_id) {
                        host.total_out += 1;
                    }
                    self.total_remote += 1;
                }
            }
            MonitorEvent::LocalJobBegin {
                job_id,
                host_id,
                filename,
            } => {
                self.jobs.insert(
                    job_id,
                    Job {
                        client_id: host_id,
                        host_id,
                        filename,
                        state: JobState::LocalActive,
                        start_time: Instant::now(),
                    },
                );
                if let Some(host) = self.hosts.get_mut(&host_id) {
                    host.total_local += 1;
                }
                self.total_local += 1;
            }
            MonitorEvent::JobDone { job_id } => {
                self.jobs.remove(&job_id);
            }
        }
    }

    /// Active jobs on a given host (executing there).
    pub fn active_jobs_on_host(&self, host_id: u32) -> impl Iterator<Item = &Job> {
        self.jobs.values().filter(move |j| {
            j.host_id == host_id
                && matches!(j.state, JobState::RemoteActive | JobState::LocalActive)
        })
    }

    /// Pending jobs from a given client.
    #[allow(dead_code)]
    pub fn pending_jobs_from_client(&self, client_id: u32) -> impl Iterator<Item = &Job> {
        self.jobs
            .values()
            .filter(move |j| j.client_id == client_id && j.state == JobState::Pending)
    }

    /// Total active (non-pending) job count.
    pub fn active_job_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| j.state != JobState::Pending)
            .count()
    }

    /// Total pending job count.
    pub fn pending_job_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| j.state == JobState::Pending)
            .count()
    }

    /// Total local active job count.
    pub fn local_job_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| j.state == JobState::LocalActive)
            .count()
    }

    /// Sorted host IDs for display.
    pub fn sorted_host_ids(&self, sort_col: SortColumn, reverse: bool) -> Vec<u32> {
        let mut ids: Vec<u32> = self.hosts.keys().copied().collect();
        ids.sort_by(|a, b| {
            let ha = &self.hosts[a];
            let hb = &self.hosts[b];
            let cmp = match sort_col {
                SortColumn::Id => ha.id.cmp(&hb.id),
                SortColumn::Name => ha.name.cmp(&hb.name),
                SortColumn::In => ha.total_in.cmp(&hb.total_in),
                SortColumn::Current => self
                    .active_jobs_on_host(*a)
                    .count()
                    .cmp(&self.active_jobs_on_host(*b).count()),
                SortColumn::MaxJobs => ha.max_jobs.cmp(&hb.max_jobs),
                SortColumn::Out => ha.total_out.cmp(&hb.total_out),
                SortColumn::Local => ha.total_local.cmp(&hb.total_local),
                SortColumn::Speed => ha.speed.cmp(&hb.speed),
            };
            if reverse { cmp.reverse() } else { cmp }
        });
        ids
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Id,
    Name,
    In,
    Current,
    MaxJobs,
    Out,
    Local,
    Speed,
}

impl SortColumn {
    pub const ALL: &'static [SortColumn] = &[
        SortColumn::Id,
        SortColumn::Name,
        SortColumn::In,
        SortColumn::Current,
        SortColumn::MaxJobs,
        SortColumn::Out,
        SortColumn::Local,
        SortColumn::Speed,
    ];

    pub fn header(&self) -> &'static str {
        match self {
            SortColumn::Id => "ID",
            SortColumn::Name => "NAME",
            SortColumn::In => "IN",
            SortColumn::Current => "CUR",
            SortColumn::MaxJobs => "MAX",
            SortColumn::Out => "OUT",
            SortColumn::Local => "LOCAL",
            SortColumn::Speed => "SPEED",
        }
    }

    pub fn next(&self) -> SortColumn {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> SortColumn {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_attrs(pairs: &[(&str, &str)]) -> Vec<(CompactString, CompactString)> {
        pairs
            .iter()
            .map(|(k, v)| (CompactString::new(k), CompactString::new(v)))
            .collect()
    }

    #[test]
    fn test_apply_host_stats() {
        let mut state = SchedulerState::new();
        let attrs = make_attrs(&[("Name", "host1"), ("MaxJobs", "4"), ("Speed", "100")]);

        state.apply_event(MonitorEvent::HostStats { host_id: 1, attrs });

        assert_eq!(state.hosts.len(), 1);
        assert_eq!(state.hosts[&1].name, "host1");
        assert_eq!(state.hosts[&1].max_jobs, 4);
    }

    #[test]
    fn test_host_removed_when_no_name() {
        let mut state = SchedulerState::new();
        let attrs = make_attrs(&[("Name", "host1")]);
        state.apply_event(MonitorEvent::HostStats { host_id: 1, attrs });
        assert_eq!(state.hosts.len(), 1);

        // No Name → remove
        state.apply_event(MonitorEvent::HostStats {
            host_id: 1,
            attrs: Vec::new(),
        });
        assert_eq!(state.hosts.len(), 0);
    }

    #[test]
    fn test_job_lifecycle() {
        let mut state = SchedulerState::new();
        // Add host
        let attrs = make_attrs(&[("Name", "h1")]);
        state.apply_event(MonitorEvent::HostStats { host_id: 1, attrs });

        // Pending
        state.apply_event(MonitorEvent::JobPending {
            job_id: 10,
            client_id: 1,
            filename: "test.c".into(),
        });
        assert_eq!(state.pending_job_count(), 1);

        // Begin
        state.apply_event(MonitorEvent::JobBegin {
            job_id: 10,
            host_id: 1,
        });
        assert_eq!(state.pending_job_count(), 0);
        assert_eq!(state.active_job_count(), 1);

        // Done
        state.apply_event(MonitorEvent::JobDone { job_id: 10 });
        assert_eq!(state.active_job_count(), 0);
    }

    #[test]
    fn test_local_job() {
        let mut state = SchedulerState::new();
        let attrs = make_attrs(&[("Name", "h1")]);
        state.apply_event(MonitorEvent::HostStats { host_id: 1, attrs });

        state.apply_event(MonitorEvent::LocalJobBegin {
            job_id: 20,
            host_id: 1,
            filename: "main.c".into(),
        });
        assert_eq!(state.local_job_count(), 1);
        assert_eq!(state.hosts[&1].total_local, 1);
        assert_eq!(state.total_local, 1);
    }

    #[test]
    fn test_disconnect_clears_state() {
        let mut state = SchedulerState::new();
        let attrs = make_attrs(&[("Name", "h1")]);
        state.apply_event(MonitorEvent::HostStats { host_id: 1, attrs });
        state.apply_event(MonitorEvent::Disconnected);
        assert!(state.hosts.is_empty());
        assert!(!state.connected);
    }

    #[test]
    fn test_sort_columns() {
        assert_eq!(SortColumn::Id.next(), SortColumn::Name);
        assert_eq!(SortColumn::Speed.next(), SortColumn::Id);
        assert_eq!(SortColumn::Id.prev(), SortColumn::Speed);
    }
}
