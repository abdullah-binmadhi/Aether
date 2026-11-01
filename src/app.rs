//! Application state shared between the input loop and the renderer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::core::collector::Collector;
use crate::core::hops::ScopeSnapshot;
use crate::core::model::Telemetry;
use crate::core::procmon::ProcMonitor;
use crate::core::PingScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard = 1,
    Spectrum = 2,
    Processes = 3,
}

impl Tab {
    pub fn all() -> [Tab; 3] {
        [Tab::Dashboard, Tab::Spectrum, Tab::Processes]
    }
    pub fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "1:COCKPIT",
            Tab::Spectrum => "2:SPECTRUM",
            Tab::Processes => "3:PROCESSES",
        }
    }
    pub fn next(self) -> Tab {
        match self {
            Tab::Dashboard => Tab::Spectrum,
            Tab::Spectrum => Tab::Processes,
            Tab::Processes => Tab::Dashboard,
        }
    }
    pub fn prev(self) -> Tab {
        match self {
            Tab::Dashboard => Tab::Processes,
            Tab::Spectrum => Tab::Dashboard,
            Tab::Processes => Tab::Spectrum,
        }
    }
}

pub struct App {
    pub collector: Collector,
    pub scope: PingScope,
    pub procmon: ProcMonitor,

    pub telemetry: Telemetry,
    pub scope_snap: ScopeSnapshot,

    pub active_tab: Tab,
    pub proc_cursor: usize,
    pub status_msg: String,
    pub should_quit: bool,
    pub tick: u64,

    /// Shared with the OS signal handler so Ctrl-C/SIGTERM break the loop
    /// and trigger the same cleanup (resume frozen processes) as [Q].
    pub stop_flag: Arc<AtomicBool>,
}

impl App {
    pub fn new(interface: &str, stop_flag: Arc<AtomicBool>) -> Self {
        let mut collector = Collector::new(interface);
        let scope = PingScope::new(interface);
        let telemetry = collector.collect(0.0, 0.0);
        let scope_snap = scope.empty_snapshot();
        App {
            collector,
            scope,
            procmon: ProcMonitor::new(),
            telemetry,
            scope_snap,
            active_tab: Tab::Dashboard,
            proc_cursor: 0,
            status_msg: "Ready. [Tab] switch view  [K] CAM pacer  [Space] pause process  [Q] quit"
                .into(),
            should_quit: false,
            tick: 0,
            stop_flag,
        }
    }

    /// One background refresh cycle: probe hops, recollect telemetry, sample procs.
    pub fn refresh(&mut self) {
        self.tick += 1;
        self.scope_snap = self.scope.probe();
        self.telemetry = self
            .collector
            .collect(self.scope_snap.avg_ms, self.scope_snap.loss_pct);
        if self.active_tab == Tab::Processes {
            self.procmon.sample();
            let n = self.procmon.last.len();
            if n == 0 {
                self.proc_cursor = 0;
            } else if self.proc_cursor >= n {
                self.proc_cursor = n - 1;
            }
        }
    }

    pub fn signalled_to_stop(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    /// Cleanup that MUST run on every exit path (quit key, Ctrl-C, SIGTERM).
    pub fn teardown(&mut self) {
        let resumed = self.procmon.resume_all();
        if resumed > 0 {
            self.status_msg = format!("Resumed {resumed} frozen process(es) on exit.");
        }
    }
}
