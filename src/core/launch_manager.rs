#![allow(dead_code)]
use crate::core::instance::{Instance, ModpackOrigin, MissingModsState};
pub use crate::core::launch::{LaunchProgress, ProcessState};
use crate::core::curseforge_modpack::SkippedMod;
use crate::core::update::UpdatedModpackMeta;
use crate::core::MutexExt;
use eframe::egui;
use std::sync::{Arc, Mutex};

/// A background task (modpack install, update, etc.) — NOT a running game process.
/// Displayed in the sidebar task tray, not the Console.
pub struct BackgroundTask {
    pub id: String,
    pub label: String,
    pub progress: Arc<Mutex<LaunchProgress>>,
    /// Slot for a newly created instance (modpack install).
    pub instance_slot: Option<Arc<Mutex<Option<Instance>>>>,
    /// Slot for an in-place modpack update result.
    pub update_slot: Option<Arc<Mutex<Option<(String, ModpackOrigin, UpdatedModpackMeta)>>>>,
    /// Slot for mods that were skipped due to distribution restrictions.
    pub skipped_slot: Option<Arc<Mutex<Vec<SkippedMod>>>>,
}

impl BackgroundTask {
    pub fn is_done(&self) -> bool {
        let p = self.progress.lock_or_recover();
        p.done
    }

    pub fn error(&self) -> Option<String> {
        self.progress.lock_or_recover().error.clone()
    }
}

/// Tracks a single running (or recently-exited) instance process.
pub struct RunningProcess {
    pub instance_id: String,
    pub instance_name: String,
    pub progress: Arc<Mutex<LaunchProgress>>,
    #[allow(clippy::type_complexity)]
    pub pending_process: Arc<Mutex<Option<Arc<Mutex<ProcessState>>>>>,
    pub process: Option<Arc<Mutex<ProcessState>>>,
    pub auto_scroll: bool,
    pub line_wrap: bool,
}

impl RunningProcess {
    /// Returns true while the instance is preparing or the child process is still running.
    pub fn is_alive(&self) -> bool {
        if let Some(proc) = &self.process
            && proc.lock_or_recover().running
        {
            return true;
        }
        let p = self.progress.lock_or_recover();
        !p.done || p.error.is_none() && self.process.is_none()
    }
}

pub enum LaunchEvent {
    InstanceInstalled(Instance, Option<Vec<SkippedMod>>),
    ModpackUpdated(String, ModpackOrigin, UpdatedModpackMeta),
    MissingMods(MissingModsState),
    Error(String, String), // (label, error_message)
}

pub struct LaunchManager {
    pub running_processes: Vec<RunningProcess>,
    pub background_tasks: Vec<BackgroundTask>,
}

impl LaunchManager {
    pub fn new() -> Self {
        Self {
            running_processes: Vec::new(),
            background_tasks: Vec::new(),
        }
    }

    pub fn poll(&mut self, _ctx: &egui::Context) -> Vec<LaunchEvent> {
        // To be implemented in Task 2
        Vec::new()
    }
}
