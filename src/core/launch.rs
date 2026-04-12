use crate::core::account::Account;
use super::{CommandHideConsole, MutexExt};
use crate::core::instance::{Instance, ModLoader};
use crate::core::java::{self, JavaInstall};
use crate::core::version::{self, ArgumentValue, VersionInfo};
use crate::core::loader_profiles;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

// ── Launch context ───────────────────────────────────────────────────────────

/// Everything needed to build and execute a launch command
pub struct LaunchContext {
    pub instance: Instance,
    pub version_info: VersionInfo,
    pub java: JavaInstall,
    pub account: Account,
    pub client_jar: PathBuf,
    pub library_paths: Vec<PathBuf>,
    pub assets_dir: PathBuf,
    pub natives_dir: PathBuf,
}

/// The result of building a launch command
pub struct LaunchCommand {
    pub java_path: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub env_vars: Vec<(String, String)>,
}

// ── Command builder ──────────────────────────────────────────────────────────

impl LaunchContext {
    pub fn build_command(&self) -> anyhow::Result<LaunchCommand> {
        let mut args = Vec::new();

        // Memory args
        args.push(format!("-Xms{}m", self.instance.min_memory_mb));
        args.push(format!("-Xmx{}m", self.instance.max_memory_mb));

        // JVM args from version JSON
        if let Some(ref arguments) = self.version_info.arguments {
            for arg in &arguments.jvm {
                match arg {
                    ArgumentValue::Plain(s) => {
                        args.push(self.template_arg(s));
                    }
                    ArgumentValue::Conditional { rules, value } => {
                        if version::rules_match(rules) {
                            for s in value.as_vec() {
                                args.push(self.template_arg(&s));
                            }
                        }
                    }
                }
            }
        } else {
            // Legacy: add default JVM args for pre-1.13
            args.push(format!(
                "-Djava.library.path={}",
                self.natives_dir.display()
            ));
            args.push("-cp".to_string());
            args.push(self.build_classpath());
        }

        // Custom JVM args from instance
        for arg in &self.instance.jvm_args {
            args.push(arg.clone());
        }

        // Main class
        args.push(self.version_info.main_class.clone());

        // Game args from version JSON
        if let Some(ref arguments) = self.version_info.arguments {
            for arg in &arguments.game {
                match arg {
                    ArgumentValue::Plain(s) => {
                        args.push(self.template_arg(s));
                    }
                    ArgumentValue::Conditional { rules, value } => {
                        if version::rules_match(rules) {
                            for s in value.as_vec() {
                                args.push(self.template_arg(&s));
                            }
                        }
                    }
                }
            }
        } else if let Some(ref legacy_args) = self.version_info.minecraft_arguments {
            // Legacy argument string (pre-1.13)
            for arg in legacy_args.split_whitespace() {
                args.push(self.template_arg(arg));
            }
        }

        let working_dir = self.instance.minecraft_dir()?;

        // Parse instance env vars (lines of KEY=VALUE, # comments)
        let env_vars: Vec<(String, String)> = self
            .instance
            .env_vars
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let (k, v) = line.split_once('=')?;
                Some((k.trim().to_string(), v.trim().to_string()))
            })
            .collect();

        Ok(LaunchCommand {
            java_path: self.java.path.clone(),
            args,
            working_dir,
            env_vars,
        })
    }

    fn build_classpath(&self) -> String {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let mut parts: Vec<String> = self
            .library_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        parts.push(self.client_jar.display().to_string());
        parts.join(sep)
    }

    fn template_arg(&self, template: &str) -> String {
        let mc_dir = self
            .instance
            .minecraft_dir()
            .unwrap_or_else(|_| PathBuf::from("."));

        template
            .replace("${auth_player_name}", &self.account.username)
            .replace("${version_name}", &self.version_info.id)
            .replace("${game_directory}", &mc_dir.display().to_string())
            .replace("${assets_root}", &self.assets_dir.display().to_string())
            .replace("${assets_index_name}", &self.version_info.asset_index.id)
            .replace("${auth_uuid}", &self.account.uuid)
            .replace("${auth_access_token}", &self.account.access_token)
            .replace("${user_type}", if self.account.offline { "legacy" } else { "msa" })
            .replace("${clientid}", crate::core::account::MS_CLIENT_ID)
            .replace("${auth_xuid}", "0")
            .replace(
                "${auth_session}",
                &if self.account.access_token.is_empty() {
                    "-".to_string()
                } else {
                    format!(
                        "token:{}:{}",
                        self.account.access_token, self.account.uuid
                    )
                },
            )
            .replace(
                "${version_type}",
                &self.version_info.version_type.to_string(),
            )
            .replace(
                "${natives_directory}",
                &self.natives_dir.display().to_string(),
            )
            .replace("${classpath}", &self.build_classpath())
            .replace("${launcher_name}", "Lurch")
            .replace("${launcher_version}", env!("CARGO_PKG_VERSION"))
            .replace(
                "${classpath_separator}",
                if cfg!(windows) { ";" } else { ":" },
            )
            .replace(
                "${library_directory}",
                &version::libraries_dir()
                    .unwrap_or_default()
                    .display()
                    .to_string(),
            )
    }
}

// ── Process runner ───────────────────────────────────────────────────────────

/// Shared state for a running Minecraft process
pub struct ProcessState {
    pub log_lines: Vec<String>,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
}

impl ProcessState {
    pub fn new() -> Self {
        Self {
            log_lines: Vec::new(),
            running: true,
            exit_code: None,
            pid: None,
        }
    }

    pub fn kill(&mut self) -> bool {
        let Some(pid) = self.pid else { return false };
        let result = if cfg!(windows) {
            std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .no_console_window()
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        if result {
            self.log_lines.push("--- Process killed by user ---".to_string());
        }
        result
    }
}

/// Spawn Minecraft and capture output. Returns shared state for the log viewer.
pub fn spawn_minecraft(
    cmd: LaunchCommand,
    ctx: egui::Context,
) -> anyhow::Result<Arc<Mutex<ProcessState>>> {
    let state = Arc::new(Mutex::new(ProcessState::new()));

    std::fs::create_dir_all(&cmd.working_dir)?;

    let child = Command::new(&cmd.java_path)
        .args(&cmd.args)
        .envs(cmd.env_vars.iter().map(|(k, v)| (k, v)))
        .current_dir(&cmd.working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_console_window()
        .spawn()?;

    {
        let mut s = state.lock_or_recover();
        s.pid = Some(child.id());
    }

    // Background thread to read output
    {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            read_process_output(child, &state, &ctx);
        });
    }

    Ok(state)
}

fn read_process_output(
    mut child: std::process::Child,
    state: &Arc<Mutex<ProcessState>>,
    ctx: &egui::Context,
) {
    use std::io::BufRead;

    // Read stderr in a separate thread
    let stderr_state = Arc::clone(state);
    let stderr_ctx = ctx.clone();
    let stderr_thread = child.stderr.take().map(|stderr| std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().map_while(|l| l.ok()) {
                let mut s = stderr_state.lock_or_recover();
                s.log_lines.push(format!("[ERR] {}", crate::core::strip_ansi(&line)));
                drop(s);
                stderr_ctx.request_repaint();
            }
        }));

    // Read stdout on this thread
    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(|l| l.ok()) {
            let mut s = state.lock_or_recover();
            s.log_lines.push(crate::core::strip_ansi(&line));
            drop(s);
            ctx.request_repaint();
        }
    }

    // Wait for stderr thread
    if let Some(t) = stderr_thread {
        let _ = t.join();
    }

    // Wait for process to finish
    let exit_code = child.wait().ok().and_then(|s| s.code());
    let mut s = state.lock_or_recover();
    s.running = false;
    s.exit_code = exit_code;
    s.log_lines
        .push(format!("--- Process exited (code: {exit_code:?}) ---"));
    drop(s);
    ctx.request_repaint();
}

// ── Prepare & launch orchestrator ────────────────────────────────────────────

use eframe::egui;

/// Full prepare + launch flow. Downloads files if needed, builds command, spawns.
/// This is blocking and should run in a background thread.
/// Java is selected automatically based on version requirements.
pub fn prepare_and_launch(
    instance: &Instance,
    account: &Account,
    java_installs: &[JavaInstall],
    manifest_versions: &[(String, String)], // (id, url) pairs
    ctx: egui::Context,
    progress: Arc<Mutex<LaunchProgress>>,
) -> anyhow::Result<Arc<Mutex<ProcessState>>> {
    // Find version URL
    let version_url = manifest_versions
        .iter()
        .find(|(id, _)| *id == instance.mc_version)
        .map(|(_, url)| url.clone())
        .ok_or_else(|| anyhow::anyhow!("Version {} not found in manifest", instance.mc_version))?;

    // Create shared HTTP client
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    // Fetch version info
    set_progress(&progress, "Fetching version info...");
    ctx.request_repaint();
    let mut version_info = version::fetch_version_info(&client, &version_url)?;

    // If instance has a mod loader, fetch and merge the loader profile
    let resolved_loader_version = if instance.loader != ModLoader::Vanilla {
        set_progress(
            &progress,
            &format!("Resolving {} version...", instance.loader),
        );
        ctx.request_repaint();
        let lv = loader_profiles::resolve_loader_version(
            &client,
            &instance.loader,
            &instance.mc_version,
            &instance.loader_version,
        )?;

        set_progress(
            &progress,
            &format!("Fetching {} profile...", instance.loader),
        );
        ctx.request_repaint();
        loader_profiles::fetch_and_merge_loader_profile(
            &client,
            &instance.loader,
            &instance.mc_version,
            &lv,
            &mut version_info,
        )?;
        Some(lv)
    } else {
        None
    };

    // Select Java based on the version JSON's actual requirement
    let required_java = version_info
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or_else(|| java::recommended_java_version(&instance.mc_version));

    set_progress(&progress, &format!("Selecting Java {required_java}..."));
    ctx.request_repaint();

    let java = if let Some(ref custom_path) = instance.java_path {
        // Instance has a custom Java path — honour the user's choice
        java::probe_java(custom_path).ok_or_else(|| {
            anyhow::anyhow!("Custom Java path is not valid: {}", custom_path.display())
        })?
    } else {
        // Try to find an exact match from detected installations
        match select_java(java_installs, required_java) {
            Ok(j) if j.major == required_java => j,
            _ => {
                // No exact match — try Mojang first, then Adoptium
                let progress_for_dl = Arc::clone(&progress);
                let ctx_for_dl = ctx.clone();
                let progress_cb = move |msg: &str| {
                    let mut p = progress_for_dl.lock_or_recover();
                    p.message = msg.to_string();
                    drop(p);
                    ctx_for_dl.request_repaint();
                };

                // Determine Mojang component: from version manifest or heuristic
                let component = version_info
                    .java_version
                    .as_ref()
                    .and_then(|jv| jv.component.clone())
                    .or_else(|| java::major_to_mojang_component(required_java).map(String::from));

                let mojang_result = component
                    .as_deref()
                    .and_then(|comp| java::download_mojang_java(&client, comp, &progress_cb).ok());

                match mojang_result {
                    Some(inst) => inst,
                    None => {
                        // Fallback to Adoptium
                        java::download_java(&client, required_java, &progress_cb)?
                    }
                }
            }
        }
    };

    set_progress(
        &progress,
        &format!("Using Java {} ({})", java.version, java.path.display()),
    );
    ctx.request_repaint();

    // Download client JAR
    set_progress(&progress, "Downloading client...");
    ctx.request_repaint();
    let client_jar = version::download_client_jar(&client, &version_info)?;

    // Run Forge/NeoForge processors if needed (must happen after client JAR download)
    if matches!(instance.loader, ModLoader::Forge | ModLoader::NeoForge)
        && let Some(ref lv) = resolved_loader_version {
            set_progress(
                &progress,
                &format!("Running {} processors...", instance.loader),
            );
            ctx.request_repaint();
            let progress_for_proc = Arc::clone(&progress);
            let ctx_for_proc = ctx.clone();
            crate::core::forge::run_processors_if_needed(
                &client,
                &instance.loader,
                &instance.mc_version,
                lv,
                &java.path,
                &client_jar,
                |msg| {
                    let mut p = progress_for_proc.lock_or_recover();
                    p.message = msg.to_string();
                    drop(p);
                    ctx_for_proc.request_repaint();
                },
            )?;
        }

    // Download libraries
    set_progress(&progress, "Downloading libraries...");
    ctx.request_repaint();
    let library_paths = version::download_libraries(&client, &version_info)?;

    // Download assets
    set_progress(&progress, "Downloading assets (0/?)...");
    ctx.request_repaint();
    let progress_for_assets = Arc::clone(&progress);
    let ctx_for_assets = ctx.clone();
    version::download_assets(&version_info, &client, move |done, total| {
        let mut p = progress_for_assets.lock_or_recover();
        p.message = format!("Downloading assets ({done}/{total})...");
        drop(p);
        ctx_for_assets.request_repaint();
    })?;

    // Prepare natives directory
    let natives_dir = instance.instance_dir()?.join("natives");
    std::fs::create_dir_all(&natives_dir)?;

    let assets_dir = version::assets_dir()?;

    set_progress(&progress, "Launching...");
    ctx.request_repaint();

    let launch_ctx = LaunchContext {
        instance: instance.clone(),
        version_info,
        java: java.clone(),
        account: account.clone(),
        client_jar,
        library_paths,
        assets_dir,
        natives_dir,
    };

    let cmd = launch_ctx.build_command()?;
    let process = spawn_minecraft(cmd, ctx)?;
    Ok(process)
}

pub struct LaunchProgress {
    pub message: String,
    pub done: bool,
    pub error: Option<String>,
}

impl LaunchProgress {
    pub fn new() -> Self {
        Self {
            message: "Preparing...".to_string(),
            done: false,
            error: None,
        }
    }
}

/// Pick the best Java from detected installations for the required major version.
/// Prefers exact match, then closest version >= required. Always prefers managed installs.
fn select_java(installs: &[JavaInstall], required: u32) -> anyhow::Result<JavaInstall> {
    // 1. Exact match — prefer managed (Lurch-downloaded) installs
    if let Some(j) = installs.iter().find(|j| j.major == required && j.managed) {
        return Ok(j.clone());
    }
    if let Some(j) = installs.iter().find(|j| j.major == required) {
        return Ok(j.clone());
    }

    // 2. Closest version >= required — prefer managed
    let mut candidates: Vec<&JavaInstall> =
        installs.iter().filter(|j| j.major >= required).collect();
    candidates.sort_by_key(|j| j.major); // ascending — closest first

    if let Some(j) = candidates.iter().find(|j| j.managed) {
        return Ok((*j).clone());
    }
    if let Some(j) = candidates.first() {
        return Ok((*j).clone());
    }

    Err(anyhow::anyhow!(
        "No Java {required} (or newer) installation found. \
         Please install Java {required} or download it from the Settings page.",
        required = required
    ))
}

fn set_progress(progress: &Arc<Mutex<LaunchProgress>>, msg: &str) {
    let mut p = progress.lock_or_recover();
    p.message = msg.to_string();
}
