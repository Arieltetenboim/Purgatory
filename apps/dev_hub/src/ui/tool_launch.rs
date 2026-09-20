use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn launch_npc_lab() -> Result<(), String> {
    let root = workspace_root()?;
    let launcher = root.join("tools").join("npc_lab").join("run.ps1");
    launch_hidden_powershell(&root, &launcher, &[], "npc-lab.log", LogMode::Append)
}

pub(crate) fn launch_mob_lab() -> Result<(), String> {
    let root = workspace_root()?;
    let launcher = root.join("tools").join("mob_lab").join("run.ps1");
    launch_visible_powershell(&root, &launcher, &[])
}

pub(crate) fn launch_character_lab() -> Result<(), String> {
    let root = workspace_root()?;
    crate::authoring_template::export_character_lab_contract()?;
    crate::authoring_template::export_character_base_template_v1()?;
    let tool = root
        .join("tools")
        .join("Character part lab")
        .join("purgatory_character_part_tool_step1.html");

    if !tool.is_file() {
        return Err(format!("Character Lab not found: {}", tool.display()));
    }

    #[cfg(windows)]
    {
        use std::time::{SystemTime, UNIX_EPOCH};

        let cache_bust = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("character lab launch clock: {err}"))?
            .as_millis();
        let file_url = format!(
            "file:///{}?purgatory_reload={cache_bust}",
            tool.to_string_lossy().replace('\\', "/").replace(' ', "%20")
        );

        std::process::Command::new("explorer.exe")
            .arg(&file_url)
            .current_dir(&root)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("launch {file_url}: {err}"))
    }

    #[cfg(not(windows))]
    {
        Err("Character Lab launch currently supports Windows only".to_owned())
    }
}

pub(crate) fn launch_quality_gate() -> Result<(), String> {
    let root = workspace_root()?;
    let script = root.join("scripts").join("check.ps1");
    let log_path = root.join("logs").join("dev-tools").join("quality-gate.log");

    match launch_hidden_powershell(&root, &script, &[], "quality-gate.log", LogMode::Truncate) {
        Ok(()) => Ok(()),
        Err(err) => {
            record_quality_gate_launch_failure(&log_path, &err);
            Err(err)
        }
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    purgatory_dev_runtime::WorkspacePaths::detect().map(|paths| paths.root)
}

#[derive(Clone, Copy)]
enum LogMode {
    Append,
    Truncate,
}

#[cfg(windows)]
fn open_log_pair(log_path: &Path, mode: LogMode) -> Result<(std::fs::File, std::fs::File), String> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {}: {err}", parent.display()))?;
    }

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
        .open(log_path)
        .map_err(|err| format!("open {}: {err}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .map_err(|err| format!("clone {}: {err}", log_path.display()))?;
    Ok((stdout, stderr))
}

fn launch_visible_powershell(
    root: &Path,
    script: &Path,
    extra_args: &[&str],
) -> Result<(), String> {
    if !script.is_file() {
        return Err(format!("launcher not found: {}", script.display()));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script)
            .args(extra_args)
            .current_dir(root)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("launch {}: {err}", script.display()))
    }

    #[cfg(not(windows))]
    {
        let _ = (root, extra_args);
        Err("Developer tool launch currently supports Windows only".to_owned())
    }
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

        let log_path = root.join("logs").join("dev-tools").join(log_name);
        let attempt = |breakaway: bool| -> Result<(), String> {
            let (stdout, stderr) = open_log_pair(&log_path, mode)?;
            let mut flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
            if breakaway {
                flags |= CREATE_BREAKAWAY_FROM_JOB;
            }

            let mut command = std::process::Command::new("powershell.exe");
            command
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(script)
                .args(extra_args)
                .current_dir(root)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::from(stdout))
                .stderr(std::process::Stdio::from(stderr))
                .creation_flags(flags)
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("launch {}: {err}", script.display()))
        };

        match attempt(true) {
            Ok(()) => Ok(()),
            Err(first) => attempt(false).map_err(|second| {
                format!("{first}; fallback without job breakaway also failed: {second}")
            }),
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (root, extra_args, log_name, mode);
        Err("Developer tool launch currently supports Windows only".to_owned())
    }
}

fn record_quality_gate_launch_failure(log_path: &Path, err: &str) {
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    else {
        return;
    };
    let _ = writeln!(file, "HUB_GATE|LAUNCH_FAIL|quality|Quality Gate");
    let _ = writeln!(file, "Quality Gate launch failed: {err}");
}
