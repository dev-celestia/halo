//! Stub halo-agent harness and session accessors.

use super::*;

impl BackendAgent for StubBackend {
    fn halo_agent_list_harnesses(
        &self,
        pi_custom: Option<&str>,
        opencode_custom: Option<&str>,
        antigravity_custom: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::HarnessInfo>> {
        Ok(halo_backend::commands::halo_agent::list_harnesses(
            pi_custom,
            opencode_custom,
            antigravity_custom,
        ))
    }

    fn halo_agent_list_models(
        &self,
        harness: &str,
        custom_executable: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::ModelInfo>> {
        Ok(halo_backend::commands::halo_agent::list_models_with(
            harness,
            custom_executable,
        ))
    }

    fn halo_agent_is_installed(
        &self,
        harness: &str,
        custom_executable: Option<&str>,
    ) -> BridgeResult<bool> {
        Ok(halo_backend::commands::halo_agent::is_harness_installed(
            harness,
            custom_executable,
        ))
    }

    fn halo_agent_browser_status(
        &self,
    ) -> BridgeResult<halo_backend::commands::halo_agent::BrowserStatus> {
        Ok(halo_backend::commands::halo_agent::browser_status())
    }

    fn halo_agent_browser_close(&self, session: &str) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::browser_close(session).map_err(BridgeError::message)
    }

    fn halo_agent_browser_forget(&self, session: &str) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::browser_forget(session).map_err(BridgeError::message)
    }

    fn halo_agent_browser_close_all(&self) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::browser_close_all().map_err(BridgeError::message)
    }

    fn halo_agent_probe_pi(
        &self,
        custom_executable: Option<&str>,
    ) -> BridgeResult<halo_backend::commands::halo_agent::PiProbeStatus> {
        Ok(halo_backend::commands::halo_agent::probe_pi(
            custom_executable,
        ))
    }

    fn halo_agent_probe_opencode(
        &self,
        custom_executable: Option<&str>,
    ) -> BridgeResult<halo_backend::commands::halo_agent::OpenCodeProbeStatus> {
        Ok(halo_backend::commands::halo_agent::probe_opencode(
            custom_executable,
        ))
    }

    fn halo_agent_probe_antigravity(
        &self,
        custom_executable: Option<&str>,
    ) -> BridgeResult<halo_backend::commands::halo_agent::AntigravityProbeStatus> {
        Ok(halo_backend::commands::halo_agent::probe_antigravity(
            custom_executable,
        ))
    }

    fn halo_agent_install_antigravity(&self) -> BridgeResult<String> {
        Ok(String::new())
    }

    fn halo_agent_run(
        &self,
        _request: &halo_backend::commands::halo_agent::RunRequest,
        _launch: &halo_backend::commands::halo_agent::AgentLaunchContext,
    ) -> BridgeResult<String> {
        Err(self.agent_unavailable())
    }

    fn halo_agent_set_live_approval(
        &self,
        _chat_id: &str,
        _run_id: &str,
        _mode: &str,
    ) -> BridgeResult<()> {
        Err(self.agent_unavailable())
    }

    fn halo_agent_respond_input(
        &self,
        _chat_id: &str,
        _run_id: &str,
        _request_id: &str,
        _answers: &[halo_backend::commands::halo_agent::UserInputAnswer],
    ) -> BridgeResult<()> {
        Err(self.agent_unavailable())
    }

    fn halo_agent_steer(
        &self,
        _chat_id: &str,
        _run_id: &str,
        _command_id: &str,
        _prompt: &str,
        _attachments: &[String],
    ) -> BridgeResult<()> {
        Err(self.agent_unavailable())
    }

    fn halo_agent_can_steer(&self, _chat_id: &str) -> BridgeResult<bool> {
        Ok(false)
    }

    fn halo_agent_interrupt_chat(&self, _chat_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    fn halo_agent_forget_harness_session(&self, _chat_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    fn halo_agent_interrupt(&self) -> BridgeResult<()> {
        Ok(())
    }

    fn halo_agent_search_files(
        &self,
        _root: &str,
        _query: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::FileSearchMatch>> {
        Ok(Vec::new())
    }

    fn halo_agent_list_skills(
        &self,
        _root: &str,
        _harness: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::SkillInfo>> {
        Ok(Vec::new())
    }

    fn halo_agent_discover_skills(
        &self,
        _root: &str,
        _harness: &str,
        _opencode_executable: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::SkillInfo>> {
        Ok(Vec::new())
    }

    fn halo_agent_list_sessions(&self) -> BridgeResult<Vec<halo_backend::HaloAgentSessionRecord>> {
        Ok(lock_err(self.agent_sessions.lock()).clone())
    }

    fn halo_agent_upsert_session(
        &self,
        session: &halo_backend::HaloAgentSessionRecord,
    ) -> BridgeResult<()> {
        let mut list = lock_err(self.agent_sessions.lock());
        if let Some(existing) = list.iter_mut().find(|s| s.id == session.id) {
            *existing = session.clone();
        } else {
            list.push(session.clone());
        }
        Ok(())
    }

    fn halo_agent_delete_persisted_session(&self, id: &str) -> BridgeResult<()> {
        lock_err(self.agent_sessions.lock()).retain(|s| s.id != id);
        Ok(())
    }
}
