//! Stub coder git operations and PTY accessors.

use super::*;

impl BackendGit for StubBackend {
    fn git_status(&self, workspace: &str) -> BridgeResult<crate::work::GitState> {
        halo_backend::commands::coder_git::status(workspace)
            .map(crate::work::GitState::from_backend)
            .map_err(BridgeError::message)
    }

    fn git_log(&self, workspace: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::log(workspace).map_err(BridgeError::message)
    }

    fn git_stage(&self, workspace: &str, paths: &[String]) -> BridgeResult<()> {
        halo_backend::commands::coder_git::stage(workspace, paths).map_err(BridgeError::message)
    }

    fn git_unstage(&self, workspace: &str, paths: &[String]) -> BridgeResult<()> {
        halo_backend::commands::coder_git::unstage(workspace, paths).map_err(BridgeError::message)
    }

    fn git_stage_all(&self, workspace: &str) -> BridgeResult<()> {
        halo_backend::commands::coder_git::stage_all(workspace).map_err(BridgeError::message)
    }

    fn git_unstage_all(&self, workspace: &str) -> BridgeResult<()> {
        halo_backend::commands::coder_git::unstage_all(workspace).map_err(BridgeError::message)
    }

    fn git_discard(&self, workspace: &str, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder_git::discard(workspace, path).map_err(BridgeError::message)
    }

    fn git_diff(&self, workspace: &str, path: &str, staged: bool) -> BridgeResult<String> {
        halo_backend::commands::coder_git::diff(workspace, path, staged)
            .map_err(BridgeError::message)
    }

    fn git_apply_hunk(&self, workspace: &str, patch: &str, reverse: bool) -> BridgeResult<String> {
        halo_backend::commands::coder_git::apply_patch(workspace, patch, reverse)
            .map_err(BridgeError::message)
    }

    fn git_commit(&self, workspace: &str, message: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::commit(workspace, message).map_err(BridgeError::message)
    }

    fn git_amend(&self, workspace: &str, message: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::amend(workspace, message).map_err(BridgeError::message)
    }

    fn git_checkout(&self, workspace: &str, branch: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::checkout(workspace, branch).map_err(BridgeError::message)
    }

    fn git_stash_and_checkout(&self, workspace: &str, branch: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::stash_and_checkout(workspace, branch)
            .map_err(BridgeError::message)
    }

    fn git_create_branch(&self, workspace: &str, name: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::create_branch(workspace, name)
            .map_err(BridgeError::message)
    }

    fn git_create_branch_from(
        &self,
        workspace: &str,
        name: &str,
        base: &str,
    ) -> BridgeResult<String> {
        halo_backend::commands::coder_git::create_branch_from(workspace, name, base)
            .map_err(BridgeError::message)
    }

    fn git_rename_branch(&self, workspace: &str, from: &str, to: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::rename_branch(workspace, from, to)
            .map_err(BridgeError::message)
    }

    fn git_push(&self, workspace: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::push(workspace).map_err(BridgeError::message)
    }

    fn git_push_to(&self, workspace: &str, remote: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::push_to(workspace, remote).map_err(BridgeError::message)
    }

    fn git_push_to_branch(
        &self,
        workspace: &str,
        remote: &str,
        branch: &str,
    ) -> BridgeResult<String> {
        halo_backend::commands::coder_git::push_to_branch(workspace, remote, branch)
            .map_err(BridgeError::message)
    }

    fn git_pull(&self, workspace: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::pull(workspace).map_err(BridgeError::message)
    }

    fn git_fetch(&self, workspace: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::fetch(workspace).map_err(BridgeError::message)
    }

    fn git_sync(&self, workspace: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::sync(workspace).map_err(BridgeError::message)
    }

    fn git_graph_log(
        &self,
        workspace: &str,
        max_count: usize,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitCommitDto>> {
        halo_backend::commands::coder_git::graph_log(workspace, max_count)
            .map_err(BridgeError::message)
    }

    fn git_list_branches(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitBranchInfoDto>> {
        halo_backend::commands::coder_git::list_branches(workspace).map_err(BridgeError::message)
    }

    fn git_list_remotes(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitRemoteInfoDto>> {
        halo_backend::commands::coder_git::list_remotes(workspace).map_err(BridgeError::message)
    }

    fn git_list_tags(&self, workspace: &str) -> BridgeResult<Vec<String>> {
        halo_backend::commands::coder_git::list_tags(workspace).map_err(BridgeError::message)
    }

    fn git_list_stashes(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitStashDto>> {
        halo_backend::commands::coder_git::list_stashes(workspace).map_err(BridgeError::message)
    }

    fn git_list_worktrees(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitWorktreeDto>> {
        halo_backend::commands::coder_git::list_worktrees(workspace).map_err(BridgeError::message)
    }

    fn git_commit_files(
        &self,
        workspace: &str,
        sha: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitCommitFileDto>> {
        halo_backend::commands::coder_git::commit_files(workspace, sha)
            .map_err(BridgeError::message)
    }

    fn git_commit_file_diff(&self, workspace: &str, sha: &str, path: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_git::commit_file_diff(workspace, sha, path)
            .map_err(BridgeError::message)
    }

    fn git_delete_branch(
        &self,
        workspace: &str,
        branch: &str,
        force: bool,
    ) -> BridgeResult<String> {
        halo_backend::commands::coder_git::delete_branch(workspace, branch, force)
            .map_err(BridgeError::message)
    }

    fn coder_pty_start(
        &self,
        _session_id: &str,
        _cwd: &str,
        _shell: Option<&str>,
        _rows: u16,
        _cols: u16,
    ) -> BridgeResult<u64> {
        // Stub has no AppHandle event loop — UI still opens; no real shell.
        Ok(0)
    }

    fn coder_pty_write(&self, _session_id: &str, _data: &[u8]) -> BridgeResult<()> {
        Ok(())
    }

    fn coder_pty_resize(&self, _session_id: &str, _rows: u16, _cols: u16) -> BridgeResult<()> {
        Ok(())
    }

    fn coder_pty_kill(&self, _session_id: &str) -> BridgeResult<()> {
        Ok(())
    }
}
