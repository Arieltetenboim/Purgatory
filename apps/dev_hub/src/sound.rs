/// Best-effort desktop cue for a server transition into Ready.
///
/// Keep audio outside the UI thread. On Windows we ask the OS for its short
/// notification sound through a hidden PowerShell process; failures are ignored
/// because lifecycle state must never depend on presentation audio.
pub fn server_ready() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "[System.Media.SystemSounds]::Asterisk.Play()",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}
