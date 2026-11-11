//! Message queue, attachments, and transcript toggle bridge methods.

use std::io::Cursor;
use std::path::PathBuf;

use super::super::*;
use crate::work::HaloAgentDelivery;

/// Persist a clipboard image under a temp dir so the existing path-based
/// attachment pipeline can read it. Unsupported formats (SVG) are skipped;
/// raster formats the agent does not accept are re-encoded as PNG.
fn persist_clipboard_image(image: &Image) -> Option<PathBuf> {
    const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
    if image.bytes().len() > MAX_IMAGE_BYTES {
        return None;
    }
    let (ext, bytes) = match image.format() {
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::Webp => {
            (image.format().extension(), image.bytes().to_vec())
        }
        ImageFormat::Tiff | ImageFormat::Bmp | ImageFormat::Ico | ImageFormat::Pnm => {
            let decoded = image::load_from_memory(image.bytes()).ok()?;
            let mut out = Vec::new();
            decoded
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .ok()?;
            if out.len() > MAX_IMAGE_BYTES {
                return None;
            }
            ("png", out)
        }
        ImageFormat::Svg => return None,
    };
    let dir = std::env::temp_dir().join("halo-agent-paste");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("paste-{}.{}", uuid::Uuid::new_v4(), ext));
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

fn stage_attachment_paths(
    work: &mut CoderWork,
    chat_id: &str,
    paths: impl IntoIterator<Item = String>,
) {
    // Preserve edits made while a picker/dialog was open, or while paste ran.
    work.agent_flush_active_session();
    let Some(session) = work
        .agent_sessions
        .iter_mut()
        .find(|x| x.id == chat_id && !x.archived)
    else {
        return;
    };
    for path in paths {
        if !halo_backend::commands::halo_agent::is_supported_image_path(&path) {
            continue;
        }
        if !session.attachments.contains(&path) {
            session.attachments.push(path);
        }
    }
}

/// Why images cannot be attached to the current chat, if the selected model
/// is known to be text-only. `None` means attach (unknown capability is not a
/// reason to block).
fn attachment_block_reason(work: &CoderWork) -> Option<String> {
    let model_id = work.agent_model.as_deref()?;
    let model = work
        .agent_model_catalog
        .iter()
        .find(|model| model.id == model_id)?;
    if model.supports_images != Some(false) {
        return None;
    }
    let alternative = work
        .agent_model_catalog
        .iter()
        .find(|model| model.supports_images == Some(true))
        .map(|model| model.display_name.clone());
    Some(match alternative {
        Some(alternative) => format!(
            "{} can't read images. Switch to {alternative} (or another vision-capable model) in Coder Settings, then attach again.",
            model.display_name
        ),
        None => format!(
            "{} can't read images. Pick a vision-capable model in Coder Settings, then attach again.",
            model.display_name
        ),
    })
}

/// Check the active chat and surface the reason through the normal error toast.
fn block_attachments_if_model_cannot_see(cx: &mut App) -> Option<String> {
    Bridge::state(cx).update(cx, |s| {
        s.coder.agent_ensure_session();
        let chat_id = s.coder.agent_active_session_id.clone();
        let reason = s
            .coder_work_for_chat_mut(&chat_id)
            .as_deref()
            .and_then(attachment_block_reason);
        if let Some(reason) = &reason {
            s.apply_err(BridgeError::message(reason.clone()));
        }
        reason
    })
}

impl Bridge {
    /// Open a multi-select image picker and stage paths for the next turn.
    pub fn halo_agent_attach_images(cx: &mut App) -> BridgeResult<()> {
        if block_attachments_if_model_cannot_see(cx).is_some() {
            return Ok(());
        }
        let chat_id = Self::state(cx).update(cx, |s| {
            s.coder.agent_ensure_session();
            s.coder.agent_active_session_id.clone()
        });
        let dialog = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach images".into()),
        });
        cx.spawn(async move |cx: &mut AsyncApp| {
            let paths = match dialog.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(err)) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| {
                            s.apply_err(BridgeError::message(format!(
                                "Attach images dialog failed: {err}"
                            )));
                        });
                    });
                    return;
                }
            };
            let paths: Vec<String> = paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let _ = cx.update(|cx| {
                // The model may have changed while the picker was open.
                if block_attachments_if_model_cannot_see(cx).is_some() {
                    return;
                }
                Self::state(cx).update(cx, |s| {
                    let Some(work) = s.coder_work_for_chat_mut(&chat_id) else {
                        return;
                    };
                    stage_attachment_paths(work, &chat_id, paths);
                    s.finish_agent_batch(&[chat_id.clone()], true);
                });
            });
        })
        .detach();
        Ok(())
    }

    /// Stage images from a paste payload (⌘V). Returns `true` when the paste
    /// was consumed (images / image files attached) so the textarea should not
    /// also insert path text.
    pub fn halo_agent_attach_paste(item: &ClipboardItem, cx: &mut App) -> bool {
        let mut ready_paths = Vec::new();
        let mut encode_jobs = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    // PNG/JPEG/GIF/WebP can land as a quick write; formats that
                    // need re-encode leave the UI thread via run_blocking.
                    match image.format() {
                        ImageFormat::Png
                        | ImageFormat::Jpeg
                        | ImageFormat::Gif
                        | ImageFormat::Webp => {
                            if let Some(path) = persist_clipboard_image(image) {
                                ready_paths.push(path.to_string_lossy().into_owned());
                            }
                        }
                        ImageFormat::Tiff
                        | ImageFormat::Bmp
                        | ImageFormat::Ico
                        | ImageFormat::Pnm => {
                            encode_jobs.push(image.clone());
                        }
                        ImageFormat::Svg => {}
                    }
                }
                ClipboardEntry::ExternalPaths(external) => {
                    for path in external.paths() {
                        let path = path.to_string_lossy().into_owned();
                        if halo_backend::commands::halo_agent::is_supported_image_path(&path) {
                            ready_paths.push(path);
                        }
                    }
                }
                ClipboardEntry::String(_) => {}
            }
        }
        let consumed = !ready_paths.is_empty() || !encode_jobs.is_empty();
        if consumed && block_attachments_if_model_cannot_see(cx).is_some() {
            // Swallow the paste so its temp path is not typed into the draft.
            return true;
        }
        if !ready_paths.is_empty() {
            Self::state(cx).update(cx, |s| {
                s.coder.agent_ensure_session();
                let chat_id = s.coder.agent_active_session_id.clone();
                let Some(work) = s.coder_work_for_chat_mut(&chat_id) else {
                    return;
                };
                stage_attachment_paths(work, &chat_id, ready_paths);
                s.finish_agent_batch(&[chat_id], true);
            });
        }
        if !encode_jobs.is_empty() {
            Self::run_blocking(
                cx,
                move || {
                    let mut paths = Vec::new();
                    for image in encode_jobs {
                        if let Some(path) = persist_clipboard_image(&image) {
                            paths.push(path.to_string_lossy().into_owned());
                        }
                    }
                    Ok(paths)
                },
                |result, cx| {
                    let Ok(paths) = result else {
                        return;
                    };
                    if paths.is_empty() {
                        return;
                    }
                    // Re-check: the model may have changed during encode.
                    if block_attachments_if_model_cannot_see(cx).is_some() {
                        return;
                    }
                    Self::state(cx).update(cx, |s| {
                        s.coder.agent_ensure_session();
                        let chat_id = s.coder.agent_active_session_id.clone();
                        let Some(work) = s.coder_work_for_chat_mut(&chat_id) else {
                            return;
                        };
                        stage_attachment_paths(work, &chat_id, paths);
                        s.finish_agent_batch(&[chat_id], true);
                    });
                },
            );
        }
        consumed
    }

    /// Drop one staged attachment by path.
    pub fn halo_agent_remove_attachment(path: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_attachments.retain(|p| p != path);
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_queue_remove(id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            // Flush first so flat `agent_queue` matches session.queue before retain.
            s.coder.agent_flush_active_session();
            // Only an in-flight Sending row is protected; Rejected / Uncertain /
            // Pending can always be dismissed from the tray.
            s.coder
                .agent_queue
                .retain(|m| m.id != id || matches!(m.delivery, HaloAgentDelivery::Sending { .. }));
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_queue_send_now(id: &str, cx: &mut App) -> BridgeResult<()> {
        let (chat_id, steer) = Self::state(cx).update(cx, |s| {
            // Flat tray edits must land on the session before dispatch reads it.
            s.coder.agent_flush_active_session();
            (
                s.coder.agent_active_session_id.clone(),
                s.coder.agent_running,
            )
        });
        // Steer only while a run is live; idle / errored retries are fresh Runs.
        Self::halo_agent_dispatch_queue_item(&chat_id, &id, steer, cx)
    }

    pub fn halo_agent_toggle_todo_expanded(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_todo_expanded = !s.coder.agent_todo_expanded;
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_dismiss_todos(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_todo_dismissed = true;
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_toggle_tool_chip(
        message_id: &str,
        tool_id: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            // Flip on the live flat transcript first (what the UI paints), then
            // flush into the session so a later refresh cannot wipe the flag.
            if let Some(row) = s
                .coder
                .agent_messages
                .iter()
                .position(|m| m.id == message_id)
            {
                let msg = s.coder.agent_messages.get_mut(row).unwrap();
                for part in &mut msg.parts {
                    if let HaloAgentPart::ToolCall { id, expanded, .. } = part {
                        if id == tool_id {
                            *expanded = !*expanded;
                        }
                    }
                }
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_toggle_reasoning(
        message_id: &str,
        part_index: usize,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(row) = s
                .coder
                .agent_messages
                .iter()
                .position(|m| m.id == message_id)
            {
                let msg = s.coder.agent_messages.get_mut(row).unwrap();
                if let Some(HaloAgentPart::Reasoning { expanded, .. }) =
                    msg.parts.get_mut(part_index)
                {
                    *expanded = !*expanded;
                }
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Open (or toggle) a child agent's transcript drawer for the active chat.
    pub fn halo_agent_open_subagent(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if s.coder
                .agent_subagents
                .iter()
                .any(|child| child.session_id == session_id)
            {
                s.coder.agent_open_subagent = Some(session_id.to_string());
                s.commit_active_tab();
            }
        });
        Ok(())
    }

    pub fn halo_agent_toggle_subagent(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if s.coder.agent_open_subagent.as_deref() == Some(session_id) {
                s.coder.agent_open_subagent = None;
            } else if s
                .coder
                .agent_subagents
                .iter()
                .any(|child| child.session_id == session_id)
            {
                s.coder.agent_open_subagent = Some(session_id.to_string());
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_close_subagent(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if s.coder.agent_open_subagent.take().is_some() {
                s.commit_active_tab();
            }
        });
        Ok(())
    }

    /// Toggle a child transcript's tool chip (same interaction as the parent).
    pub fn halo_agent_toggle_subagent_tool_chip(
        session_id: &str,
        message_id: &str,
        tool_id: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(child) = s
                .coder
                .agent_subagents
                .iter_mut()
                .find(|c| c.session_id == session_id)
            {
                if let Some(row) = child.messages.iter().position(|m| m.id == message_id) {
                    let msg = child.messages.get_mut(row).unwrap();
                    for part in &mut msg.parts {
                        if let HaloAgentPart::ToolCall { id, expanded, .. } = part {
                            if id == tool_id {
                                *expanded = !*expanded;
                            }
                        }
                    }
                }
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_toggle_subagent_reasoning(
        session_id: &str,
        message_id: &str,
        part_index: usize,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(child) = s
                .coder
                .agent_subagents
                .iter_mut()
                .find(|c| c.session_id == session_id)
            {
                if let Some(row) = child.messages.iter().position(|m| m.id == message_id) {
                    let msg = child.messages.get_mut(row).unwrap();
                    if let Some(HaloAgentPart::Reasoning { expanded, .. }) =
                        msg.parts.get_mut(part_index)
                    {
                        *expanded = !*expanded;
                    }
                }
            }
            s.commit_active_tab();
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_backend::commands::halo_agent::ModelInfo;

    fn work_with(supports_images: Option<bool>) -> CoderWork {
        let mut work = CoderWork::default();
        work.agent_model = Some("opencode/beam-free".into());
        work.agent_model_catalog = vec![
            ModelInfo {
                id: "opencode/beam-free".into(),
                display_name: "Beam Free".into(),
                supports_images,
            },
            ModelInfo {
                id: "opencode/nemotron-vl".into(),
                display_name: "Nemotron VL".into(),
                supports_images: Some(true),
            },
        ];
        work
    }

    #[test]
    fn blocks_known_text_only_models_and_names_a_vision_alternative() {
        let reason = attachment_block_reason(&work_with(Some(false))).expect("blocked");
        assert!(reason.contains("Beam Free can't read images"));
        assert!(reason.contains("Nemotron VL"));
    }

    #[test]
    fn unknown_or_vision_models_stay_attachable() {
        assert!(attachment_block_reason(&work_with(None)).is_none());
        assert!(attachment_block_reason(&work_with(Some(true))).is_none());
    }
}
