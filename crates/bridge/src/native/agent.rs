//! Halo-agent harness, session, and browser accessors.

use super::*;

impl BackendAgent for NativeBackend {
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
        halo_backend::commands::halo_agent::install_antigravity().map_err(BridgeError::message)
    }

    fn halo_agent_run(
        &self,
        request: &halo_backend::commands::halo_agent::RunRequest,
        launch: &halo_backend::commands::halo_agent::AgentLaunchContext,
    ) -> BridgeResult<String> {
        halo_backend::commands::halo_agent::run(&self.runtime.app, request.clone(), launch.clone())
            .map_err(BridgeError::message)
    }

    fn halo_agent_set_live_approval(
        &self,
        chat_id: &str,
        run_id: &str,
        mode: &str,
    ) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::set_live_approval_mode(
            &self.runtime.app,
            chat_id,
            run_id,
            mode,
        )
        .map_err(BridgeError::message)
    }

    fn halo_agent_respond_input(
        &self,
        chat_id: &str,
        run_id: &str,
        request_id: &str,
        answers: &[halo_backend::commands::halo_agent::UserInputAnswer],
    ) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::respond_input(
            &self.runtime.app,
            chat_id,
            run_id,
            request_id,
            answers.to_vec(),
        )
        .map_err(BridgeError::message)
    }

    fn halo_agent_steer(
        &self,
        chat_id: &str,
        run_id: &str,
        command_id: &str,
        prompt: &str,
        attachments: &[String],
    ) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::steer(
            &self.runtime.app,
            chat_id,
            run_id,
            command_id,
            prompt,
            attachments,
        )
        .map_err(BridgeError::message)
    }

    fn halo_agent_can_steer(&self, chat_id: &str) -> BridgeResult<bool> {
        Ok(halo_backend::commands::halo_agent::can_steer(
            &self.runtime.app,
            chat_id,
        ))
    }

    fn halo_agent_interrupt_chat(&self, chat_id: &str) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::interrupt_chat(&self.runtime.app, chat_id);
        Ok(())
    }

    fn halo_agent_forget_harness_session(&self, chat_id: &str) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::forget_harness_session(&self.runtime.app, chat_id);
        Ok(())
    }

    fn halo_agent_interrupt(&self) -> BridgeResult<()> {
        halo_backend::commands::halo_agent::interrupt(&self.runtime.app);
        Ok(())
    }

    fn halo_agent_search_files(
        &self,
        root: &str,
        query: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::FileSearchMatch>> {
        halo_backend::commands::halo_agent::search_files(root, query).map_err(BridgeError::message)
    }

    fn halo_agent_list_skills(
        &self,
        root: &str,
        harness: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::SkillInfo>> {
        halo_backend::commands::halo_agent::list_skills(root, harness).map_err(BridgeError::message)
    }

    fn halo_agent_discover_skills(
        &self,
        root: &str,
        harness: &str,
        opencode_executable: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::SkillInfo>> {
        halo_backend::commands::halo_agent::discover_skills(root, harness, opencode_executable)
            .map_err(BridgeError::message)
    }

    fn halo_agent_list_sessions(&self) -> BridgeResult<Vec<halo_backend::HaloAgentSessionRecord>> {
        self.runtime
            .history
            .list_halo_agent_sessions()
            .map_err(BridgeError::message)
    }

    fn halo_agent_upsert_session(
        &self,
        session: &halo_backend::HaloAgentSessionRecord,
    ) -> BridgeResult<()> {
        self.runtime
            .history
            .upsert_halo_agent_session(session)
            .map_err(BridgeError::message)
    }

    fn halo_agent_delete_persisted_session(&self, id: &str) -> BridgeResult<()> {
        self.runtime
            .history
            .delete_halo_agent_session(id)
            .map_err(BridgeError::message)
    }
}
