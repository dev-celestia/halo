//! Stub repeater sends and folder records.

use super::*;

impl BackendRepeater for StubBackend {
    fn repeater_send(&self, request: RepeaterRequestDto) -> BridgeResult<RepeaterResponseDto> {
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "text/plain".into());
        Ok(RepeaterResponseDto {
            status: 200,
            status_text: "OK".into(),
            headers,
            body: format!(
                "Stub response for {} {}\n\nEcho body:\n{}",
                request.method, request.url, request.body
            ),
            time_ms: 12,
            final_url: request.url,
        })
    }

    fn get_collections(&self) -> BridgeResult<Vec<CollectionDto>> {
        Ok(lock_err(self.collections.lock()).clone())
    }

    fn save_collection(&self, collection: CollectionDto) -> BridgeResult<()> {
        let mut list = lock_err(self.collections.lock());
        if let Some(existing) = list.iter_mut().find(|c| c.id == collection.id) {
            *existing = collection;
        } else {
            list.push(collection);
        }
        Ok(())
    }

    fn delete_collection(&self, id: &str) -> BridgeResult<()> {
        let stash_ids: Vec<String> = lock_err(self.stashes.lock())
            .iter()
            .filter(|s| s.collection_id.as_deref() == Some(id))
            .map(|s| s.id.clone())
            .collect();
        lock_err(self.endpoints.lock()).retain(|e| !stash_ids.iter().any(|sid| *sid == e.stash_id));
        lock_err(self.stashes.lock()).retain(|s| s.collection_id.as_deref() != Some(id));
        lock_err(self.collections.lock()).retain(|c| c.id != id);
        Ok(())
    }

    fn get_stashes(&self) -> BridgeResult<Vec<StashDto>> {
        Ok(lock_err(self.stashes.lock()).clone())
    }

    fn save_stash(&self, stash: StashDto) -> BridgeResult<()> {
        let mut list = lock_err(self.stashes.lock());
        if let Some(existing) = list.iter_mut().find(|s| s.id == stash.id) {
            *existing = stash;
        } else {
            list.push(stash);
        }
        Ok(())
    }

    fn delete_stash(&self, id: &str) -> BridgeResult<()> {
        lock_err(self.stashes.lock()).retain(|s| s.id != id);
        lock_err(self.endpoints.lock()).retain(|e| e.stash_id != id);
        Ok(())
    }

    fn get_stash_endpoints(&self) -> BridgeResult<Vec<StashEndpointDto>> {
        Ok(lock_err(self.endpoints.lock()).clone())
    }

    fn save_stash_endpoint(&self, endpoint: StashEndpointDto) -> BridgeResult<()> {
        let mut list = lock_err(self.endpoints.lock());
        if let Some(existing) = list.iter_mut().find(|e| e.id == endpoint.id) {
            *existing = endpoint;
        } else {
            list.push(endpoint);
        }
        Ok(())
    }

    fn delete_stash_endpoint(&self, id: &str) -> BridgeResult<()> {
        lock_err(self.endpoints.lock()).retain(|e| e.id != id);
        Ok(())
    }

    fn get_contexts(&self) -> BridgeResult<Vec<ContextDto>> {
        Ok(lock_err(self.contexts.lock()).clone())
    }

    fn save_context(&self, context: ContextDto) -> BridgeResult<()> {
        let mut list = lock_err(self.contexts.lock());
        if let Some(existing) = list.iter_mut().find(|c| c.id == context.id) {
            *existing = context;
        } else {
            list.push(context);
        }
        Ok(())
    }

    fn delete_context(&self, id: &str) -> BridgeResult<()> {
        lock_err(self.contexts.lock()).retain(|c| c.id != id);
        Ok(())
    }

    fn get_chronicle_logs(&self, limit: u32) -> BridgeResult<Vec<ChronicleLogDto>> {
        let list = lock_err(self.chronicle.lock());
        Ok(list.iter().take(limit as usize).cloned().collect())
    }

    fn add_chronicle_log(&self, record: ChronicleLogDto) -> BridgeResult<()> {
        let mut list = lock_err(self.chronicle.lock());
        list.insert(0, record);
        if list.len() > 500 {
            list.truncate(500);
        }
        Ok(())
    }

    fn clear_chronicle_logs(&self) -> BridgeResult<()> {
        lock_err(self.chronicle.lock()).clear();
        Ok(())
    }
}
