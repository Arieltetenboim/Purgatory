use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;

use crate::log_buffer::{IncomingLog, append_file_line};
use crate::paths::exe_name;
use crate::process::{DiscoveredProcess, ProcessLifetime};

#[derive(Clone, Debug)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub log_name: &'static str,
    /// Live UI pump (session jobs). Detached server uses file stdio instead.
    pub ui_pump: bool,
    pub lifetime: ProcessLifetime,
}

pub trait ProcessBackend {
    fn spawn(
        &mut self,
        spec: SpawnSpec,
        incoming: &IncomingLog,
        log_dir: &Path,
    ) -> Result<u32, String>;
    fn try_wait(&mut self, pid: u32) -> Option<i32>;
    fn is_alive(&mut self, pid: u32) -> bool;
    fn kill_tree(&mut self, pid: u32);
    fn discover_workspace(&mut self, stem: &str, target_prefix: &Path) -> Vec<DiscoveredProcess>;
    /// Drop the wait-handle without killing. Used so Hub exit does not reap a detached server.
    fn detach(&mut self, pid: u32);
    fn run_capture(&mut self, spec: SpawnSpec) -> Result<CapturedOutput, String>;
    /// Visible console (quality gate / analyze). Detached from Hub lifetime.
    fn spawn_visible(&mut self, spec: SpawnSpec) -> Result<u32, String>;
    /// Kill All only: cargo.exe whose command line contains this workspace root.
    fn kill_workspace_cargo(&mut self, root: &Path) -> usize;
}

#[derive(Clone, Debug, Default)]
pub struct CapturedOutput {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

enum OwnedChild {
    Session(Child),
    Detached(Child),
}

pub struct StdProcessBackend {
    children: HashMap<u32, OwnedChild>,
    reaped: HashMap<u32, i32>,
}

impl StdProcessBackend {
    pub fn new() -> Self {
        Self {
            children: HashMap::new(),
            reaped: HashMap::new(),
        }
    }

    fn reap_child(&mut self, pid: u32) -> Option<i32> {
        let child = match self.children.get_mut(&pid) {
            Some(OwnedChild::Session(c) | OwnedChild::Detached(c)) => c,
            None => return None,
        };
        match child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => {
                self.children.remove(&pid);
                Some(status.code().unwrap_or(1))
            }
            Err(_) => {
                self.children.remove(&pid);
                Some(1)
            }
        }
    }
}

impl Default for StdProcessBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for StdProcessBackend {
    fn drop(&mut self) {
        let pids: Vec<(u32, bool)> = self
            .children
            .iter()
            .map(|(pid, c)| (*pid, matches!(c, OwnedChild::Session(_))))
            .collect();
        for (pid, session) in pids {
            if session {
                kill_process_tree(pid);
            }
            self.children.remove(&pid);
        }
    }
}

impl ProcessBackend for StdProcessBackend {
    fn spawn(
        &mut self,
        spec: SpawnSpec,
        incoming: &IncomingLog,
        log_dir: &Path,
    ) -> Result<u32, String> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null());
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        let log_path = log_dir.join(format!("{}.log", spec.log_name));
        let mut child = if spec.lifetime == ProcessLifetime::Detached {
            spawn_detached(&spec, &log_path)?
        } else {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            apply_session_flags(&mut cmd);
            cmd.spawn()
                .map_err(|e| format!("failed to start {}: {e}", spec.program.display()))?
        };
        let pid = child.id();
        if spec.lifetime != ProcessLifetime::Detached && spec.ui_pump {
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            if let Some(out) = stdout {
                spawn_pump(spec.log_name, out, log_path.clone(), Some(incoming.clone()));
            }
            if let Some(err) = stderr {
                spawn_pump(spec.log_name, err, log_path, Some(incoming.clone()));
            }
        } else if spec.lifetime != ProcessLifetime::Detached {
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            if let Some(out) = stdout {
                spawn_pump(spec.log_name, out, log_path.clone(), None);
            }
            if let Some(err) = stderr {
                spawn_pump(spec.log_name, err, log_path, None);
            }
        }
        let owned = match spec.lifetime {
            ProcessLifetime::Session => OwnedChild::Session(child),
            ProcessLifetime::Detached => OwnedChild::Detached(child),
        };
        self.children.insert(pid, owned);
        Ok(pid)
    }

    fn try_wait(&mut self, pid: u32) -> Option<i32> {
        if let Some(code) = self.reaped.remove(&pid) {
            return Some(code);
        }
        if let Some(code) = self.reap_child(pid) {
            return Some(code);
        }
        None
    }

    fn is_alive(&mut self, pid: u32) -> bool {
        if self.reaped.contains_key(&pid) {
            return false;
        }
        if self.children.contains_key(&pid) {
            if let Some(code) = self.reap_child(pid) {
                self.reaped.insert(pid, code);
                return false;
            }
            return true;
        }
        pid_alive_external(pid)
    }

    fn kill_tree(&mut self, pid: u32) {
        kill_process_tree(pid);
        self.children.remove(&pid);
    }

    fn discover_workspace(&mut self, stem: &str, target_prefix: &Path) -> Vec<DiscoveredProcess> {
        discover_under_target(stem, target_prefix)
    }

    fn detach(&mut self, pid: u32) {
        if let Some(OwnedChild::Session(child)) = self.children.remove(&pid) {
            self.children.insert(pid, OwnedChild::Detached(child));
        }
    }

    fn run_capture(&mut self, spec: SpawnSpec) -> Result<CapturedOutput, String> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        apply_session_flags(&mut cmd);
        let output = cmd
            .output()
            .map_err(|e| format!("failed to run {}: {e}", spec.program.display()))?;
        Ok(CapturedOutput {
            exit: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn spawn_visible(&mut self, spec: SpawnSpec) -> Result<u32, String> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args).current_dir(&spec.cwd);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0000_0010 | CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP);
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("failed to start visible {}: {e}", spec.program.display()))?;
        let pid = child.id();
        self.children.insert(pid, OwnedChild::Detached(child));
        Ok(pid)
    }

    fn kill_workspace_cargo(&mut self, root: &Path) -> usize {
        let pids = discover_workspace_cargo(root);
        let n = pids.len();
        for pid in pids {
            self.kill_tree(pid);
        }
        n
    }
}

fn open_inheritable_append(path: &Path) -> Result<std::fs::File, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        opts.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    opts.open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))
}

fn spawn_pump(
    name: &'static str,
    stream: impl Read + Send + 'static,
    log_path: PathBuf,
    incoming: Option<IncomingLog>,
) {
    thread::Builder::new()
        .name(format!("purgatory-log-{name}"))
        .spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let line = line.replace('\u{0007}', "");
                if line.is_empty() {
                    continue;
                }
                if let Some(parent) = log_path.parent() {
                    let _ = append_file_line(
                        parent,
                        log_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("child.log"),
                        &line,
                    );
                }
                if let Some(ui) = &incoming {
                    ui.push(name, &line);
                }
            }
        })
        .ok();
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

fn apply_session_flags(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

fn apply_detached_flags(cmd: &mut Command, breakaway: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
        if breakaway {
            flags |= CREATE_BREAKAWAY_FROM_JOB;
        }
        cmd.creation_flags(flags);
    }
    let _ = (cmd, breakaway);
}

fn spawn_detached(spec: &SpawnSpec, log_path: &Path) -> Result<Child, String> {
    let attempt = |breakaway: bool| -> Result<Child, String> {
        let file = open_inheritable_append(log_path)?;
        let err_file = file
            .try_clone()
            .map_err(|e| format!("clone log {}: {e}", log_path.display()))?;
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null());
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::from(file));
        cmd.stderr(Stdio::from(err_file));
        apply_detached_flags(&mut cmd, breakaway);
        cmd.spawn()
            .map_err(|e| format!("failed to start {}: {e}", spec.program.display()))
    };
    match attempt(true) {
        Ok(child) => Ok(child),
        Err(first) => attempt(false).or(Err(first)),
    }
}

fn kill_process_tree(pid: u32) {
    if pid <= 4 {
        return;
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
}

fn pid_alive_external(pid: u32) -> bool {
    #[cfg(windows)]
    {
        discover_pid_exists(pid)
    }
    #[cfg(not(windows))]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

#[cfg(windows)]
fn discover_pid_exists(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    sys.process(Pid::from_u32(pid)).is_some()
}

#[cfg(windows)]
fn discover_workspace_cargo(root: &Path) -> Vec<u32> {
    use sysinfo::{ProcessesToUpdate, System};
    let root_needle = root.to_string_lossy().to_lowercase();
    if root_needle.is_empty() {
        return Vec::new();
    }
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut out = Vec::new();
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy();
        if !(name.eq_ignore_ascii_case("cargo.exe") || name.eq_ignore_ascii_case("cargo")) {
            continue;
        }
        let line = proc
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if line.contains(&root_needle) {
            out.push(pid.as_u32());
        }
    }
    out
}

#[cfg(not(windows))]
fn discover_workspace_cargo(_root: &Path) -> Vec<u32> {
    Vec::new()
}

#[cfg(windows)]
fn discover_under_target(stem: &str, target_prefix: &Path) -> Vec<DiscoveredProcess> {
    use sysinfo::{ProcessesToUpdate, System};
    let want = exe_name(stem);
    let prefix = target_prefix.to_string_lossy().to_lowercase();
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut out = Vec::new();
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy();
        if !name.eq_ignore_ascii_case(&want) {
            continue;
        }
        let Some(exe) = proc.exe() else { continue };
        let path_s = exe.to_string_lossy().to_lowercase();
        if !path_s.starts_with(&prefix) {
            continue;
        }
        out.push(DiscoveredProcess {
            pid: pid.as_u32(),
            exe_path: exe.to_path_buf(),
        });
    }
    out
}

#[cfg(not(windows))]
fn discover_under_target(_stem: &str, _target_prefix: &Path) -> Vec<DiscoveredProcess> {
    Vec::new()
}

/// Test double. Spawned PIDs are synthetic; discovery is injected.
#[derive(Default)]
pub struct FakeProcessBackend {
    next_pid: u32,
    pub alive: HashMap<u32, FakeProc>,
    pub discovered: Vec<DiscoveredProcess>,
    pub spawn_log: Vec<String>,
    pub lifetimes: Vec<ProcessLifetime>,
    pub probe_exit: Option<i32>,
    pub cargo_exit: i32,
    pub hold_cargo: bool,
    pub server_spawn_fail: bool,
    pub kill_log: Vec<u32>,
    pub detach_log: Vec<u32>,
    pub harness_exit: Option<i32>,
    pub print_env_exit: i32,
    pub print_env_stdout: String,
    pub print_env_stderr: String,
    pub extra_env_log: Vec<(String, String)>,
}

pub struct FakeProc {
    pub kind: FakeKind,
    pub pending_exit: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FakeKind {
    Cargo,
    Server,
    Probe,
    Harness,
    Other,
}

impl FakeProcessBackend {
    pub fn new() -> Self {
        Self {
            next_pid: 100,
            cargo_exit: 0,
            harness_exit: Some(0),
            print_env_exit: 0,
            print_env_stdout: r#"[["PURGATORY_ADMISSION_CAP","256"],["PURGATORY_METRICS_PORT","5002"],["PURGATORY_LOAD_VALIDATION","{}"]]"#.to_string(),
            ..Self::default()
        }
    }

    fn alloc(&mut self) -> u32 {
        self.next_pid += 1;
        self.next_pid
    }
}

impl ProcessBackend for FakeProcessBackend {
    fn spawn(
        &mut self,
        spec: SpawnSpec,
        _incoming: &IncomingLog,
        _log_dir: &Path,
    ) -> Result<u32, String> {
        let name = spec
            .program
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        self.spawn_log
            .push(format!("{} {}", name, spec.args.join(" ")));
        self.lifetimes.push(spec.lifetime);
        if name.contains("purgatory-server") {
            self.extra_env_log.extend(spec.env.iter().cloned());
        }
        if name.contains("purgatory-server") && self.server_spawn_fail {
            return Err("server executable failed to start".to_string());
        }
        let pid = self.alloc();
        let (kind, pending_exit) = if name.contains("cargo") {
            let pending = if self.hold_cargo {
                None
            } else {
                Some(self.cargo_exit)
            };
            (FakeKind::Cargo, pending)
        } else if spec.args.iter().any(|a| a == "--probe") {
            (FakeKind::Probe, self.probe_exit)
        } else if name.contains("purgatory-load") {
            (FakeKind::Harness, self.harness_exit)
        } else if name.contains("purgatory-client") {
            (FakeKind::Other, None)
        } else if name.contains("purgatory-server") {
            (FakeKind::Server, None)
        } else {
            (FakeKind::Other, None)
        };
        self.alive.insert(pid, FakeProc { kind, pending_exit });
        Ok(pid)
    }

    fn try_wait(&mut self, pid: u32) -> Option<i32> {
        let proc = self.alive.get_mut(&pid)?;
        if let Some(code) = proc.pending_exit {
            self.alive.remove(&pid);
            return Some(code);
        }
        None
    }

    fn is_alive(&mut self, pid: u32) -> bool {
        self.alive.contains_key(&pid)
    }

    fn kill_tree(&mut self, pid: u32) {
        self.kill_log.push(pid);
        self.alive.remove(&pid);
    }

    fn discover_workspace(&mut self, stem: &str, _target_prefix: &Path) -> Vec<DiscoveredProcess> {
        self.discovered
            .iter()
            .filter(|d| {
                d.exe_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem))
            })
            .cloned()
            .collect()
    }

    fn detach(&mut self, pid: u32) {
        self.detach_log.push(pid);
    }

    fn run_capture(&mut self, spec: SpawnSpec) -> Result<CapturedOutput, String> {
        self.spawn_log.push(format!(
            "capture {} {}",
            spec.program.display(),
            spec.args.join(" ")
        ));
        Ok(CapturedOutput {
            exit: self.print_env_exit,
            stdout: self.print_env_stdout.clone(),
            stderr: self.print_env_stderr.clone(),
        })
    }

    fn spawn_visible(&mut self, spec: SpawnSpec) -> Result<u32, String> {
        self.spawn_log.push(format!(
            "visible {} {}",
            spec.program.display(),
            spec.args.join(" ")
        ));
        self.lifetimes.push(ProcessLifetime::Detached);
        Ok(self.alloc())
    }

    fn kill_workspace_cargo(&mut self, _root: &Path) -> usize {
        let pids: Vec<u32> = self
            .alive
            .iter()
            .filter(|(_, p)| p.kind == FakeKind::Cargo)
            .map(|(pid, _)| *pid)
            .collect();
        let n = pids.len();
        for pid in pids {
            self.kill_tree(pid);
        }
        n
    }
}
