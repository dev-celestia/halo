use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct CollaboratorPollingState {
    pub active: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}
