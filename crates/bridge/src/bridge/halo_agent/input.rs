//! Human approval / AskUserQuestion input wizard bridge methods.

use super::super::*;

impl Bridge {
    pub fn halo_agent_respond_input(
        request_id: &str,
        answers: &[(String, Vec<String>)],
        cx: &mut App,
    ) -> BridgeResult<()> {
        let (chat_id, run_id) = {
            let s = Self::state(cx).read(cx);
            let Some(pending) = s
                .coder
                .agent_pending_input
                .as_ref()
                .filter(|p| p.request_id == request_id)
            else {
                return Ok(());
            };
            (pending.chat_id.clone(), pending.run_id.clone())
        };
        Self::halo_agent_respond_input_for_run(&chat_id, &run_id, request_id, answers, cx)
    }

    /// Use the identity captured with the approval UI, including for delayed callbacks.
    pub fn halo_agent_respond_input_for_run(
        chat_id: &str,
        run_id: &str,
        request_id: &str,
        answers: &[(String, Vec<String>)],
        cx: &mut App,
    ) -> BridgeResult<()> {
        let valid = {
            let s = Self::state(cx).read(cx);
            s.coder_work_for_chat(chat_id)
                .and_then(|w| w.agent_sessions.iter().find(|x| x.id == chat_id))
                .is_some_and(|session| session.approval_matches(run_id, request_id))
        };
        if !valid {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let payload: Vec<halo_backend::commands::halo_agent::UserInputAnswer> = answers
            .iter()
            .map(
                |(question_id, labels)| halo_backend::commands::halo_agent::UserInputAnswer {
                    question_id: question_id.clone(),
                    labels: labels.clone(),
                },
            )
            .collect();
        let result = backend.halo_agent_respond_input(chat_id, run_id, request_id, &payload);
        if let Err(err) = &result {
            // Click handlers discard BridgeResult. Without this a failed
            // Allow/Deny leaves the card frozen with no feedback; the card
            // stays up so the answer can be retried.
            Self::state(cx).update(cx, |s| {
                s.apply_err(BridgeError::message(err.to_string()));
            });
            return Err(BridgeError::message(err.to_string()));
        }
        Self::state(cx).update(cx, |s| {
            s.apply_event(BackendEvent::HaloAgentEvent {
                run_id: run_id.to_string(),
                chat_id: chat_id.to_string(),
                event: AgentEvent::InputResolved {
                    request_id: request_id.to_string(),
                },
            });
        });
        Ok(())
    }

    pub fn halo_agent_input_select(option_ix: usize, cx: &mut App) -> BridgeResult<()> {
        // Selection only — never auto-submit. The user must click Allow / Continue
        // (or Deny) so they can review the choice first.
        Self::state(cx).update(cx, |s| {
            let Some(pending) = s.coder.agent_pending_input.as_mut() else {
                return;
            };
            let page = pending.page;
            let Some(question) = pending.questions.get(page) else {
                return;
            };
            if option_ix >= question.options.len() {
                return;
            }
            if pending.picked.len() <= page {
                pending.picked.resize(page + 1, Vec::new());
            }
            let multi = question.multi_select;
            let picked = &mut pending.picked[page];
            if multi {
                match picked.iter().position(|&p| p == option_ix) {
                    Some(at) => {
                        picked.remove(at);
                    }
                    None => picked.push(option_ix),
                }
            } else {
                *picked = vec![option_ix];
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn halo_agent_input_advance(cx: &mut App) -> BridgeResult<()> {
        let (request_id, answers, done) = Self::state(cx).update(cx, |s| {
            let Some(pending) = s.coder.agent_pending_input.as_mut() else {
                return (String::new(), Vec::new(), false);
            };
            if pending.page + 1 < pending.questions.len() {
                pending.page += 1;
                s.commit_active_tab();
                return (String::new(), Vec::new(), false);
            }
            let answers: Vec<(String, Vec<String>)> = pending
                .questions
                .iter()
                .enumerate()
                .map(|(ix, q)| {
                    let labels = pending
                        .picked
                        .get(ix)
                        .map(|picked| {
                            picked
                                .iter()
                                .filter_map(|&p| q.options.get(p).cloned())
                                .collect()
                        })
                        .unwrap_or_default();
                    (q.id.clone(), labels)
                })
                .collect();
            let request_id = pending.request_id.clone();
            s.commit_active_tab();
            (request_id, answers, true)
        });
        if done && !request_id.is_empty() {
            Self::halo_agent_respond_input(&request_id, &answers, cx)?;
        }
        Ok(())
    }

    pub fn halo_agent_input_dismiss(cx: &mut App) -> BridgeResult<()> {
        let request_id = Self::state(cx).update(cx, |s| {
            let id = s
                .coder
                .agent_pending_input
                .as_ref()
                .map(|p| p.request_id.clone());
            s.commit_active_tab();
            id
        });
        if let Some(request_id) = request_id {
            Self::halo_agent_respond_input(&request_id, &[], cx)?;
        }
        Ok(())
    }
}
