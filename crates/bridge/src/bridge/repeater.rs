//! Forge / Repeater: craft pipeline, folders, stashes, contexts and chronicle.

use super::util::*;
use super::*;

impl Bridge {
    pub fn send_repeater(request: RepeaterRequestDto, cx: &mut App) -> BridgeResult<()> {
        Self::send_craft_request(request, cx)
    }

    /// Forge craft pipeline: expand `{{vars}}` from the active context, dispatch via
    /// reqwest, append a chronicle record, and flip the panel to the response view.
    pub fn send_craft_request(request: RepeaterRequestDto, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.active_request.is_loading = true;
            s.active_request.error = None;
        });

        let vars = {
            let state = Self::state(cx).read(cx);
            let ctx_id = state.active_context_id.clone();
            state
                .contexts
                .iter()
                .find(|c| Some(&c.id) == ctx_id.as_ref())
                .map(|c| parse_context_variables(&c.variables))
                .unwrap_or_default()
        };

        let (method, url, headers, body) = expand_request(
            &request.method,
            &request.url,
            &request.headers,
            &request.body,
            &vars,
        );
        let expanded = RepeaterRequestDto {
            method,
            url,
            headers,
            body,
        };

        match backend.repeater_send(expanded.clone()) {
            Ok(response) => {
                let chronicle = ChronicleLogDto {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp: chrono_now(),
                    method: expanded.method.clone(),
                    url: expanded.url.clone(),
                    request_headers: serde_json::to_string(&expanded.headers).ok(),
                    request_body: Some(expanded.body.clone()),
                    response_status: Some(response.status),
                    response_status_text: Some(response.status_text.clone()),
                    response_headers: serde_json::to_string(&response.headers).ok(),
                    response_body: Some(response.body.clone()),
                    duration_ms: Some(response.time_ms as i64),
                };
                let _ = backend.add_chronicle_log(chronicle);
                let chronicle = backend.get_chronicle_logs(100).unwrap_or_default();
                Self::state(cx).update(cx, |s| {
                    s.repeater_response = Some(response);
                    s.last_sent_request = Some(expanded);
                    s.chronicle = chronicle;
                    s.forge_view = ForgeActiveView::Response;
                    s.active_request.is_loading = false;
                    s.apply_ok_msg("Forge response received".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| {
                    s.active_request.is_loading = false;
                    s.active_request.error = Some(err.to_string());
                    s.apply_err(err.clone());
                });
                Err(err)
            }
        }
    }

    pub fn refresh_folders(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::ensure_collections_migrated(&backend)?;
        let collections = backend.get_collections()?;
        let stashes = backend.get_stashes()?;
        let endpoints = backend.get_stash_endpoints()?;
        let contexts = backend.get_contexts()?;
        let chronicle = backend.get_chronicle_logs(100).unwrap_or_default();
        Self::state(cx).update(cx, |s| {
            s.collections = collections;
            s.stashes = stashes;
            s.endpoints = endpoints;
            s.contexts = contexts;
            s.chronicle = chronicle;
            if s.active_collection_id
                .as_ref()
                .is_none_or(|id| !s.collections.iter().any(|c| &c.id == id))
            {
                s.active_collection_id = s.collections.first().map(|c| c.id.clone());
            }
            if s.active_context_id.is_none() {
                s.active_context_id = s.contexts.first().map(|c| c.id.clone());
            }
        });
        Ok(())
    }

    /// Ensure at least one collection exists and every orphan root stash is
    /// assigned to "Default" (or the first collection).
    pub(crate) fn ensure_collections_migrated(
        backend: &std::sync::Arc<dyn Backend>,
    ) -> BridgeResult<()> {
        let mut collections = backend.get_collections()?;
        let mut stashes = backend.get_stashes()?;
        let now = chrono_now();

        if collections.is_empty() {
            let id = uuid::Uuid::new_v4().to_string();
            let default = CollectionDto {
                id: id.clone(),
                name: "Default".into(),
                sort_order: Some(0),
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            backend.save_collection(default.clone())?;
            collections.push(default);
        }

        let default_id = collections
            .iter()
            .find(|c| c.name == "Default")
            .or_else(|| collections.first())
            .map(|c| c.id.clone())
            .expect("collections non-empty after ensure");

        let mut dirty = false;
        for stash in &mut stashes {
            if stash.collection_id.is_none() {
                // Nested folders inherit via ancestors at render time, but we
                // still stamp roots (and orphans) so filtering by collection works.
                stash.collection_id = Some(default_id.clone());
                stash.updated_at = now.clone();
                backend.save_stash(stash.clone())?;
                dirty = true;
            }
        }
        let _ = dirty;
        Ok(())
    }

    pub fn set_active_collection(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.active_collection_id = id);
    }

    pub fn create_collection(name: &str, cx: &mut App) -> BridgeResult<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let now = chrono_now();
        let id = uuid::Uuid::new_v4().to_string();
        let sort_order = Self::state(cx).read(cx).collections.len() as i32;
        backend.save_collection(CollectionDto {
            id: id.clone(),
            name: trimmed.to_string(),
            sort_order: Some(sort_order),
            created_at: now.clone(),
            updated_at: now,
        })?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            s.active_collection_id = Some(id);
            s.apply_ok_msg("Collection created".into());
        });
        Ok(())
    }

    pub fn rename_collection(id: &str, name: &str, cx: &mut App) -> BridgeResult<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let collection = {
            let state = Self::state(cx).read(cx);
            let Some(existing) = state.collections.iter().find(|c| c.id == id) else {
                return Ok(());
            };
            if existing.name == trimmed {
                return Ok(());
            }
            let mut collection = existing.clone();
            collection.name = trimmed.to_string();
            collection.updated_at = chrono_now();
            collection
        };
        backend.save_collection(collection)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Collection renamed".into()));
        Ok(())
    }

    pub fn delete_collection(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        // Also wipe nested stashes that may still point at this collection via
        // descendants (parent_id chain) if their collection_id was never stamped.
        let stash_ids: Vec<String> = {
            let state = Self::state(cx).read(cx);
            let roots: Vec<String> = state
                .stashes
                .iter()
                .filter(|s| s.collection_id.as_deref() == Some(id))
                .map(|s| s.id.clone())
                .collect();
            let mut all = roots.clone();
            let mut frontier = roots;
            while let Some(cur) = frontier.pop() {
                for child in state
                    .stashes
                    .iter()
                    .filter(|s| s.parent_id.as_deref() == Some(cur.as_str()))
                {
                    frontier.push(child.id.clone());
                    all.push(child.id.clone());
                }
            }
            all
        };
        for stash_id in &stash_ids {
            // Endpoints + stash deleted by storage cascade when collection_id matches;
            // nested-only rows need explicit deletes.
            let endpoint_ids: Vec<String> = Self::state(cx)
                .read(cx)
                .endpoints
                .iter()
                .filter(|e| e.stash_id == *stash_id)
                .map(|e| e.id.clone())
                .collect();
            for endpoint_id in endpoint_ids {
                let _ = backend.delete_stash_endpoint(&endpoint_id);
            }
            let _ = backend.delete_stash(stash_id);
        }
        backend.delete_collection(id)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            if s.active_collection_id.as_deref() == Some(id) {
                s.active_collection_id = s.collections.first().map(|c| c.id.clone());
            }
            if stash_ids
                .iter()
                .any(|sid| s.active_request.stash_id.as_deref() == Some(sid))
            {
                s.active_request = ActiveRequestState::default();
            }
            s.apply_ok_msg("Collection deleted".into());
        });
        Ok(())
    }

    pub fn set_tree_filter(filter: String, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.tree_filter = filter);
    }

    pub fn set_collapsed_stashes(collapsed: Vec<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.collapsed_stashes = collapsed);
    }

    pub fn set_forge_view(view: ForgeActiveView, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.forge_view = view);
    }

    pub fn set_forge_request_tab(tab: ForgeRequestTab, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.forge_request_tab = tab);
    }

    pub fn set_forge_response_tab(tab: ForgeResponseTab, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.forge_response_tab = tab);
    }

    pub fn set_active_context(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.active_context_id = id);
    }

    pub fn set_env_modal_open(open: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.env_modal_open = open);
    }

    pub fn select_folder_node(node_id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.selected_node_id = node_id);
    }

    /// Load a stash endpoint into the active Forge draft (editors sync from UI).
    pub fn load_endpoint(endpoint_id: &str, cx: &mut App) -> BridgeResult<StashEndpointDto> {
        let endpoint = Self::state(cx)
            .read(cx)
            .endpoints
            .iter()
            .find(|e| e.id == endpoint_id)
            .cloned()
            .ok_or_else(|| BridgeError::message("Endpoint not found"))?;
        let ep = endpoint.clone();
        Self::state(cx).update(cx, |s| {
            s.selected_node_id = Some(format!("ep-{}", ep.id));
            s.active_request = ActiveRequestState {
                endpoint_id: Some(ep.id.clone()),
                stash_id: Some(ep.stash_id.clone()),
                name: ep.name.clone(),
                method: ep.method.clone(),
                url: ep.url.clone(),
                body: ep.body.clone().unwrap_or_default(),
                body_type: ep.body_type.clone().unwrap_or_else(|| "raw".into()),
                pre_script: ep.pre_script.clone().unwrap_or_default(),
                test_script: ep.test_script.clone().unwrap_or_default(),
                is_loading: false,
                error: None,
            };
            s.forge_view = ForgeActiveView::Request;
            s.apply_ok_msg(format!("Loaded {}", ep.name));
        });
        Ok(endpoint)
    }

    pub fn create_stash(name: &str, cx: &mut App) -> BridgeResult<()> {
        Self::create_stash_in(None, name, cx)
    }

    /// Create a folder; `parent_id` nests it inside another folder.
    pub fn create_stash_in(parent_id: Option<&str>, name: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let now = chrono_now();
        let (sort_order, collection_id) = {
            let state = Self::state(cx).read(cx);
            let collection_id = if let Some(pid) = parent_id {
                state
                    .stashes
                    .iter()
                    .find(|s| s.id == pid)
                    .and_then(|s| s.collection_id.clone())
                    .or_else(|| state.active_collection_id.clone())
            } else {
                state.active_collection_id.clone()
            };
            (state.stashes.len() as i32, collection_id)
        };
        let stash = StashDto {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            parent_id: parent_id.map(|id| id.to_string()),
            collection_id,
            sort_order: Some(sort_order),
            created_at: now.clone(),
            updated_at: now,
        };
        backend.save_stash(stash)?;
        Self::refresh_folders(cx)?;
        // Expand the parent so the new subfolder is visible right away.
        if let Some(parent) = parent_id {
            let mut collapsed = Self::state(cx).read(cx).collapsed_stashes.clone();
            if let Some(pos) = collapsed.iter().position(|id| id == parent) {
                collapsed.remove(pos);
                Self::set_collapsed_stashes(collapsed, cx);
            }
        }
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Folder created".into()));
        Ok(())
    }

    /// Rename a folder in place, keeping its parent and ordering.
    ///
    /// Renaming is a sidebar concern, so it goes through the same
    /// `save_stash` path as creation rather than a dedicated command.
    pub fn rename_stash(id: &str, name: &str, cx: &mut App) -> BridgeResult<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let stash = {
            let state = Self::state(cx).read(cx);
            let Some(existing) = state.stashes.iter().find(|s| s.id == id) else {
                return Ok(());
            };
            if existing.name == trimmed {
                return Ok(());
            }
            let mut stash = existing.clone();
            stash.name = trimmed.to_string();
            stash.updated_at = chrono_now();
            stash
        };
        backend.save_stash(stash)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Folder renamed".into()));
        Ok(())
    }

    /// Delete a folder plus everything nested inside it (subfolders and their
    /// saved requests). Storage has no cascade, so the tree walk happens here.
    pub fn delete_stash(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let (stash_ids, endpoint_ids) = {
            let state = Self::state(cx).read(cx);
            let mut stash_ids = vec![id.to_string()];
            let mut frontier = vec![id.to_string()];
            while let Some(cur) = frontier.pop() {
                for child in state
                    .stashes
                    .iter()
                    .filter(|s| s.parent_id.as_deref() == Some(cur.as_str()))
                {
                    frontier.push(child.id.clone());
                    stash_ids.push(child.id.clone());
                }
            }
            let endpoint_ids: Vec<String> = state
                .endpoints
                .iter()
                .filter(|e| stash_ids.iter().any(|sid| *sid == e.stash_id))
                .map(|e| e.id.clone())
                .collect();
            (stash_ids, endpoint_ids)
        };
        for endpoint_id in &endpoint_ids {
            backend.delete_stash_endpoint(endpoint_id)?;
        }
        for stash_id in &stash_ids {
            backend.delete_stash(stash_id)?;
        }
        Self::state(cx).update(cx, |s| {
            if stash_ids
                .iter()
                .any(|sid| s.active_request.stash_id.as_deref() == Some(sid))
            {
                s.active_request = ActiveRequestState::default();
            }
        });
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Folder deleted".into()));
        Ok(())
    }

    pub fn create_endpoint(stash_id: &str, name: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let now = chrono_now();
        let endpoint = StashEndpointDto {
            id: uuid::Uuid::new_v4().to_string(),
            stash_id: stash_id.to_string(),
            name: name.to_string(),
            method: "GET".into(),
            url: "https://{{host}}/".into(),
            headers: Some(r#"{"Accept":"application/json"}"#.into()),
            body: Some(String::new()),
            body_type: Some("none".into()),
            pre_script: Some(String::new()),
            test_script: Some(String::new()),
            sort_order: Some(0),
            created_at: now.clone(),
            updated_at: now,
        };
        let id = endpoint.id.clone();
        backend.save_stash_endpoint(endpoint)?;
        Self::refresh_folders(cx)?;
        let _ = Self::load_endpoint(&id, cx);
        Ok(())
    }

    pub fn save_active_endpoint(
        method: &str,
        url: &str,
        headers_json: &str,
        body: &str,
        body_type: &str,
        pre_script: &str,
        test_script: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let now = chrono_now();
        let (endpoint_id, stash_id, name) = {
            let s = Self::state(cx).read(cx);
            (
                s.active_request.endpoint_id.clone(),
                s.active_request.stash_id.clone(),
                if s.active_request.name.is_empty() {
                    "Untitled".into()
                } else {
                    s.active_request.name.clone()
                },
            )
        };

        let stash_id = match stash_id {
            Some(id) => id,
            None => {
                // Prefer a root folder in the active collection.
                let collection_id = Self::state(cx).read(cx).active_collection_id.clone();
                let stashes = backend.get_stashes()?;
                if let Some(first) = stashes.iter().find(|s| {
                    s.parent_id.is_none()
                        && (collection_id.is_none()
                            || s.collection_id.as_ref() == collection_id.as_ref())
                }) {
                    first.id.clone()
                } else if let Some(first) = stashes.first() {
                    first.id.clone()
                } else {
                    let id = uuid::Uuid::new_v4().to_string();
                    backend.save_stash(StashDto {
                        id: id.clone(),
                        name: "Folder".into(),
                        parent_id: None,
                        collection_id,
                        sort_order: Some(0),
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    })?;
                    id
                }
            }
        };

        let id = endpoint_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let endpoint = StashEndpointDto {
            id: id.clone(),
            stash_id: stash_id.clone(),
            name,
            method: method.to_string(),
            url: url.to_string(),
            headers: Some(headers_json.to_string()),
            body: Some(body.to_string()),
            body_type: Some(body_type.to_string()),
            pre_script: Some(pre_script.to_string()),
            test_script: Some(test_script.to_string()),
            sort_order: Some(0),
            created_at: now.clone(),
            updated_at: now,
        };
        backend.save_stash_endpoint(endpoint)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            s.active_request.endpoint_id = Some(id);
            s.active_request.stash_id = Some(stash_id);
            s.apply_ok_msg("Endpoint saved".into());
        });
        Ok(())
    }

    /// Rename a saved endpoint in place, keeping its folder and ordering.
    ///
    /// Like [`Self::rename_stash`], this rides the normal save path instead of a
    /// dedicated command so storage stays the single source of truth.
    pub fn rename_endpoint(id: &str, name: &str, cx: &mut App) -> BridgeResult<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let endpoint = {
            let state = Self::state(cx).read(cx);
            let Some(existing) = state.endpoints.iter().find(|e| e.id == id) else {
                return Ok(());
            };
            if existing.name == trimmed {
                return Ok(());
            }
            let mut endpoint = existing.clone();
            endpoint.name = trimmed.to_string();
            endpoint.updated_at = chrono_now();
            endpoint
        };
        backend.save_stash_endpoint(endpoint)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            if s.active_request.endpoint_id.as_deref() == Some(id) {
                s.active_request.name = trimmed.to_string();
            }
            s.apply_ok_msg("Endpoint renamed".into());
        });
        Ok(())
    }

    pub fn delete_endpoint(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.delete_stash_endpoint(id)?;
        Self::state(cx).update(cx, |s| {
            if s.active_request.endpoint_id.as_deref() == Some(id) {
                s.active_request = ActiveRequestState::default();
            }
        });
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Endpoint deleted".into()));
        Ok(())
    }

    pub fn create_context(name: &str, variables_json: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let now = chrono_now();
        let id = uuid::Uuid::new_v4().to_string();
        backend.save_context(ContextDto {
            id: id.clone(),
            name: name.to_string(),
            variables: if variables_json.trim().is_empty() {
                "{}".into()
            } else {
                variables_json.to_string()
            },
            created_at: now.clone(),
            updated_at: now,
        })?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            s.active_context_id = Some(id);
            s.apply_ok_msg("Environment created".into());
        });
        Ok(())
    }

    pub fn save_context_variables(
        id: &str,
        variables_json: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let mut ctx = Self::state(cx)
            .read(cx)
            .contexts
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| BridgeError::message("Context not found"))?;
        ctx.variables = variables_json.to_string();
        ctx.updated_at = chrono_now();
        backend.save_context(ctx)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Environment saved".into()));
        Ok(())
    }

    pub fn delete_context(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.delete_context(id)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            if s.active_context_id.as_deref() == Some(id) {
                s.active_context_id = s.contexts.first().map(|c| c.id.clone());
            }
            s.apply_ok_msg("Environment deleted".into());
        });
        Ok(())
    }

    pub fn rename_context(id: &str, new_name: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let mut ctx = Self::state(cx)
            .read(cx)
            .contexts
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| BridgeError::message("Context not found"))?;
        ctx.name = new_name.to_string();
        ctx.updated_at = chrono_now();
        backend.save_context(ctx)?;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| s.apply_ok_msg("Environment renamed".into()));
        Ok(())
    }

    pub fn clear_chronicle(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.clear_chronicle_logs()?;
        Self::state(cx).update(cx, |s| {
            s.chronicle.clear();
            s.apply_ok_msg("Chronicle cleared".into());
        });
        Ok(())
    }

    /// Save Postman exports that the UI already read off the UI thread.
    ///
    /// Each entry is `(filename, json)`. Collections and environments are added;
    /// nothing already stored is replaced. The new collection is selected, and
    /// the new context is selected when the file defines variables.
    pub fn import_postman_files(
        documents: Vec<(String, String)>,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if documents.is_empty() {
            return Ok(());
        }
        let mut merged = super::postman_import::PostmanImport::default();
        for (name, text) in &documents {
            match super::postman_import::parse_postman_export(text) {
                Ok(parsed) => {
                    merged.collections.extend(parsed.collections);
                    merged.environments.extend(parsed.environments);
                }
                Err(err) => {
                    let message = format!("{name}: {err}");
                    Self::state(cx).update(cx, |s| {
                        s.apply_err(BridgeError::message(message.clone()));
                    });
                    return Err(BridgeError::message(message));
                }
            }
        }

        let backend = Self::backend(cx)?;
        let now = chrono_now();
        let mut sort_order = Self::state(cx).read(cx).collections.len() as i32;
        let mut saved = SavedImport::default();
        if let Err(err) =
            persist_postman_import(&*backend, &merged, &now, &mut sort_order, &mut saved)
        {
            let _ = Self::refresh_folders(cx);
            Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
            return Err(err);
        }

        let message = import_summary(&saved);
        let SavedImport {
            last_collection_id,
            collection_context_id,
            environment_context_id,
            ..
        } = saved;
        Self::refresh_folders(cx)?;
        Self::state(cx).update(cx, |s| {
            if let Some(id) = last_collection_id {
                s.active_collection_id = Some(id);
            }
            if let Some(id) = collection_context_id.or(environment_context_id) {
                s.active_context_id = Some(id);
            }
            s.apply_ok_msg(message);
        });
        Ok(())
    }
}

struct SavedImport {
    collection_names: Vec<String>,
    environment_names: Vec<String>,
    folders: usize,
    requests: usize,
    last_collection_id: Option<String>,
    collection_context_id: Option<String>,
    environment_context_id: Option<String>,
}

impl Default for SavedImport {
    fn default() -> Self {
        Self {
            collection_names: Vec::new(),
            environment_names: Vec::new(),
            folders: 0,
            requests: 0,
            last_collection_id: None,
            collection_context_id: None,
            environment_context_id: None,
        }
    }
}

fn persist_postman_import(
    backend: &dyn Backend,
    import: &super::postman_import::PostmanImport,
    now: &str,
    sort_order: &mut i32,
    saved: &mut SavedImport,
) -> BridgeResult<()> {
    for collection in &import.collections {
        let id = uuid::Uuid::new_v4().to_string();
        backend.save_collection(CollectionDto {
            id: id.clone(),
            name: collection.name.clone(),
            sort_order: Some(*sort_order),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        })?;
        *sort_order += 1;
        saved.last_collection_id = Some(id.clone());
        saved.collection_names.push(collection.name.clone());
        for (index, folder) in collection.folders.iter().enumerate() {
            save_imported_folder(backend, folder, None, &id, index as i32, now, saved)?;
        }
        if let Some(variables) = collection.variables_json.as_deref() {
            let context_id = uuid::Uuid::new_v4().to_string();
            backend.save_context(ContextDto {
                id: context_id.clone(),
                name: format!("{} variables", collection.name),
                variables: variables.to_string(),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            })?;
            saved.collection_context_id = Some(context_id);
        }
    }
    for environment in &import.environments {
        let context_id = uuid::Uuid::new_v4().to_string();
        backend.save_context(ContextDto {
            id: context_id.clone(),
            name: environment.name.clone(),
            variables: environment.variables_json.clone(),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        })?;
        saved.environment_names.push(environment.name.clone());
        saved.environment_context_id = Some(context_id);
    }
    Ok(())
}

fn save_imported_folder(
    backend: &dyn Backend,
    folder: &super::postman_import::ParsedFolder,
    parent_id: Option<&str>,
    collection_id: &str,
    sort_order: i32,
    now: &str,
    saved: &mut SavedImport,
) -> BridgeResult<()> {
    let id = uuid::Uuid::new_v4().to_string();
    // Nested folders also carry `collection_id`. The startup migration stamps
    // every stash with a null collection onto Default, which would pull them
    // out of this collection.
    backend.save_stash(StashDto {
        id: id.clone(),
        name: folder.name.clone(),
        parent_id: parent_id.map(str::to_string),
        collection_id: Some(collection_id.to_string()),
        sort_order: Some(sort_order),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })?;
    saved.folders += 1;
    for (index, request) in folder.requests.iter().enumerate() {
        backend.save_stash_endpoint(StashEndpointDto {
            id: uuid::Uuid::new_v4().to_string(),
            stash_id: id.clone(),
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            headers: Some(request.headers_json.clone()),
            body: Some(request.body.clone()),
            body_type: Some(request.body_type.clone()),
            pre_script: Some(request.pre_script.clone()),
            test_script: Some(request.test_script.clone()),
            sort_order: Some(index as i32),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        })?;
        saved.requests += 1;
    }
    for (index, child) in folder.folders.iter().enumerate() {
        save_imported_folder(
            backend,
            child,
            Some(&id),
            collection_id,
            index as i32,
            now,
            saved,
        )?;
    }
    Ok(())
}

fn import_summary(saved: &SavedImport) -> String {
    let mut parts = Vec::new();
    match saved.collection_names.as_slice() {
        [name] => parts.push(format!(
            "Imported {name}: {}, {}",
            count_label(saved.folders, "folder", "folders"),
            count_label(saved.requests, "request", "requests"),
        )),
        names if !names.is_empty() => parts.push(format!(
            "Imported {} collections: {}, {}",
            names.len(),
            count_label(saved.folders, "folder", "folders"),
            count_label(saved.requests, "request", "requests"),
        )),
        _ => {}
    }
    match saved.environment_names.as_slice() {
        [name] => parts.push(format!("Imported environment {name}")),
        names if !names.is_empty() => {
            parts.push(format!("Imported {} environments", names.len()));
        }
        _ => {}
    }
    if parts.is_empty() {
        "Nothing to import".into()
    } else {
        parts.join(". ")
    }
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}
