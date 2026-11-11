//! Repeater sends and folder records (collections, stashes, endpoints, contexts, chronicle).

use super::dto::*;
use super::*;

impl BackendRepeater for NativeBackend {
    fn repeater_send(&self, request: RepeaterRequestDto) -> BridgeResult<RepeaterResponseDto> {
        let req = RepeaterRequest {
            method: request.method,
            url: request.url,
            headers: request.headers,
            body: request.body,
        };
        let response = self
            .tokio
            .block_on(send_repeater_request(req))
            .map_err(BridgeError::message)?;
        Ok(RepeaterResponseDto {
            status: response.status,
            status_text: response.status_text,
            headers: response.headers,
            body: response.body,
            time_ms: response.time_ms,
            final_url: response.final_url,
        })
    }

    fn get_collections(&self) -> BridgeResult<Vec<CollectionDto>> {
        self.runtime
            .history
            .get_collections()
            .map(|rows| rows.into_iter().map(collection_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn save_collection(&self, collection: CollectionDto) -> BridgeResult<()> {
        self.runtime
            .history
            .save_collection(&dto_to_collection(collection))
            .map_err(BridgeError::message)
    }

    fn delete_collection(&self, id: &str) -> BridgeResult<()> {
        self.runtime
            .history
            .delete_collection(id)
            .map_err(BridgeError::message)
    }

    fn get_stashes(&self) -> BridgeResult<Vec<StashDto>> {
        self.runtime
            .history
            .get_stashes()
            .map(|rows| rows.into_iter().map(stash_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn save_stash(&self, stash: StashDto) -> BridgeResult<()> {
        self.runtime
            .history
            .save_stash(&dto_to_stash(stash))
            .map_err(BridgeError::message)
    }

    fn delete_stash(&self, id: &str) -> BridgeResult<()> {
        self.runtime
            .history
            .delete_stash(id)
            .map_err(BridgeError::message)
    }

    fn get_stash_endpoints(&self) -> BridgeResult<Vec<StashEndpointDto>> {
        self.runtime
            .history
            .get_stash_endpoints()
            .map(|rows| rows.into_iter().map(endpoint_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn save_stash_endpoint(&self, endpoint: StashEndpointDto) -> BridgeResult<()> {
        self.runtime
            .history
            .save_stash_endpoint(&dto_to_endpoint(endpoint))
            .map_err(BridgeError::message)
    }

    fn delete_stash_endpoint(&self, id: &str) -> BridgeResult<()> {
        self.runtime
            .history
            .delete_stash_endpoint(id)
            .map_err(BridgeError::message)
    }

    fn get_contexts(&self) -> BridgeResult<Vec<ContextDto>> {
        self.runtime
            .history
            .get_contexts()
            .map(|rows| rows.into_iter().map(context_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn save_context(&self, context: ContextDto) -> BridgeResult<()> {
        self.runtime
            .history
            .save_context(&dto_to_context(context))
            .map_err(BridgeError::message)
    }

    fn delete_context(&self, id: &str) -> BridgeResult<()> {
        self.runtime
            .history
            .delete_context(id)
            .map_err(BridgeError::message)
    }

    fn get_chronicle_logs(&self, limit: u32) -> BridgeResult<Vec<ChronicleLogDto>> {
        self.runtime
            .history
            .get_chronicle_logs(limit)
            .map(|rows| rows.into_iter().map(chronicle_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn add_chronicle_log(&self, record: ChronicleLogDto) -> BridgeResult<()> {
        self.runtime
            .history
            .add_chronicle_log(&dto_to_chronicle(record))
            .map_err(BridgeError::message)
    }

    fn clear_chronicle_logs(&self) -> BridgeResult<()> {
        self.runtime
            .history
            .clear_chronicle_logs()
            .map_err(BridgeError::message)
    }
}
