//! Process-level helpers: self-restart and opening the dashboard in a browser.

/// Environment variable used to carry the Web UI token across self-restarts,
/// so an already-open dashboard tab keeps working after a reboot.
pub const UI_TOKEN_ENV: &str = "POLYCOPIER_UI_TOKEN";

/// Build the argument list for a restarted instance: original args, with
/// `--ui` rewritten to `--ui-reboot` so the browser is not opened again.
pub fn restart_args(original: &[String]) -> Vec<String> {
    original
        .iter()
        .skip(1)
        .map(|a| {
            if a == "--ui" {
                "--ui-reboot".to_string()
            } else {
                a.clone()
            }
        })
        .collect()
}

/// Replace the current process with a fresh instance of itself (Unix: `exec`;
/// elsewhere: spawn + exit). `ui_token`, if given, is passed to the child.
/// Only returns on failure.
pub fn self_restart(ui_token: Option<&str>) -> anyhow::Error {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return anyhow::anyhow!("cannot resolve current executable: {e}"),
    };
    let args = restart_args(&std::env::args().collect::<Vec<_>>());
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args);
    if let Some(t) = ui_token {
        cmd.env(UI_TOKEN_ENV, t);
    }
    tracing::warn!("Restarting polycopier...");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        anyhow::anyhow!("exec failed: {err}")
    }
    #[cfg(not(unix))]
    {
        match cmd.spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => anyhow::anyhow!("spawn failed: {e}"),
        }
    }
}

/// Open `url` in the default browser (best effort, cross-platform).
pub fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    if let Err(e) = result {
        tracing::warn!("Could not open a browser automatically ({e}). Open {url} manually.");
    }
}
