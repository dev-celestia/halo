//! Intruder (Invoker) attack control and result selection.

use super::*;

impl Bridge {
    pub fn start_intruder(draft: IntruderDraft, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.intruder_start(draft) {
            Ok(status) => {
                Self::state(cx).update(cx, |s| {
                    let config = s.intruder.config.clone();
                    s.intruder = IntruderStatus {
                        running: status.running,
                        attack_id: status.attack_id.clone(),
                        message: status.message.clone(),
                        progress_current: 0,
                        progress_total: 0,
                        results: Vec::new(),
                        selected_result_id: None,
                        config,
                    };
                    s.apply_ok_msg(
                        status
                            .message
                            .unwrap_or_else(|| "Intruder attack started".into()),
                    );
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn stop_intruder(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.intruder_stop() {
            Ok(status) => {
                Self::state(cx).update(cx, |s| {
                    s.intruder.running = status.running;
                    s.intruder.attack_id = status.attack_id;
                    s.intruder.message = status.message;
                    s.apply_ok_msg("Intruder attack stopped".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn select_intruder_result(id: Option<String>, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| s.intruder.selected_result_id = id);
        Ok(())
    }

    pub fn clear_intruder_results(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.intruder.results.clear();
            s.intruder.selected_result_id = None;
            s.intruder.progress_current = 0;
            s.intruder.progress_total = 0;
            s.apply_ok_msg("Intruder results cleared".into());
        });
        Ok(())
    }
}
