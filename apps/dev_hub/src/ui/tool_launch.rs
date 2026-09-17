use std::path::{Path, PathBuf};

pub(crate) fn launch_npc_lab() -> Result<(), String> {
    let root = workspace_root()?;
    let launcher = root.join("tools").join("npc_lab").join("run.ps1");
    launch_hidden_powershell(
        &root,
        &launcher,
        &[],
        "npc-lab.log",
        LogMode::Append,
    )
}

pub(crate) fn launch_quality_gate() -> Result<(), String> {
    let root = workspace_root()?;
    let script = root.join("scripts").join("check.ps1");
    launch_hidden_powershell(
        &root,
        &script,
        &[],
        "quality-gate.log",
        LogMode::Truncate,
    )
}

fn workspace_root() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|err| format!("current directory: {err}"))
}

#[derive(Clone, Copy)]
enum LogMode {
    Append,
    Truncate,
}

fn launch_hidden_powershell(
    root: &Path,
    script: &Path,
    extra_args: &[&str],
    log_name: &str,
    mode: LogMode,
) -> Result<(), String> {
    if !script.is_file() {
        return Err(format!("launcher not found: {}", script.display()));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

        let log_dir = root.join("logs").join("dev-tools");
        std::fs::create_dir_all(&log_dir)
            .map_err(|err| format!("create {}: {err}", log_dir.display()))?;
        let log_path = log_dir.join(log_name);
        let mut options = std::fs::OpenOptions::new();
        options.create(true).write(true);
        match mode {
            LogMode::Append => {
                options.append(true);
            }
            LogMode::Truncate => {
                options.truncate(true);
            }
        }
        let stdout = options
            .open(&log_path)
            .map_err(|err| format!("open {}: {err}", log_path.display()))?;
        let stderr = stdout
            .try_clone()
            .map_err(|err| format!("clone {}: {err}", log_path.display()))?;

        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script)
            .args(extra_args)
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr))
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
            .map_err(|err| format!("launch {}: {err}", script.display()))?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (root, extra_args, log_name, mode);
        Err("Developer tool launch currently supports Windows only".to_owned())
    }
}
