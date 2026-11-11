//! Browser tooling for the agent surface: status, refresh, teardown.
//!
//! Resolution spawns a process (`agent-browser --version`), so it runs through
//! [`Bridge::run_blocking`] like the harness catalogs — never from paint.

use super::super::*;

impl Bridge {
    /// Resolve the browser CLI off the UI thread and publish the result.
    pub fn halo_agent_refresh_browser(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| s.coder.agent_browser_loading = true);
        Self::run_blocking(
            cx,
            move || backend.halo_agent_browser_status(),
            move |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.coder.agent_browser_loading = false;
                    if let Ok(status) = result {
                        s.coder.agent_browser = status;
                    }
                });
            },
        );
        Ok(())
    }

    /// Tear down one chat's browser session. Safe to call when none exists.
    ///
    /// Spawns `agent-browser close`, so it runs off the UI thread; the row keeps
    /// its "Ready" state either way — closing a tab does not uninstall a binary.
    pub fn halo_agent_browser_close(cx: &mut App, session: &str) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let session = session.to_string();
        Self::run_blocking(
            cx,
            move || backend.halo_agent_browser_close(&session),
            |_result, _cx| {},
        );
        Ok(())
    }

    /// Close a deleted chat's session and erase the state it accumulated.
    pub fn halo_agent_browser_forget(cx: &mut App, session: &str) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let session = session.to_string();
        Self::run_blocking(
            cx,
            move || backend.halo_agent_browser_forget(&session),
            |_result, _cx| {},
        );
        Ok(())
    }

    /// Close every session, in one worker.
    ///
    /// `agent-browser close --all` is a single process, so this is cheap enough
    /// to run on quit as well as from the settings row.
    pub fn halo_agent_browser_close_all(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::run_blocking(
            cx,
            move || backend.halo_agent_browser_close_all(),
            |_result, _cx| {},
        );
        Ok(())
    }

    /// Close every session and wait, briefly, for the daemon to answer.
    ///
    /// Quit cannot hand this to the executor — the app is going away — so it
    /// runs on a thread with a deadline. A daemon that does not answer in time
    /// is left to its own hour-long idle timeout rather than holding the quit.
    pub fn halo_agent_browser_close_all_blocking(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("halo-browser-quit".into())
            .spawn(move || {
                let _ = backend.halo_agent_browser_close_all();
                let _ = tx.send(());
            })
            .is_ok();
        if spawned {
            let _ = rx.recv_timeout(std::time::Duration::from_secs(3));
        }
    }
}
