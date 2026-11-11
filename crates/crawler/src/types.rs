pub use halo_core::types::{AIInsight, ActivityLog, CrawlConfig, CrawlPage, CrawlSession};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, atomic::AtomicBool};

pub type CrawlControls = HashMap<String, HashMap<String, Arc<celestia_spider::CrawlControl>>>;
pub type CrawlCancellations = HashMap<String, HashMap<String, Arc<AtomicBool>>>;

#[derive(Default, Clone)]
pub struct CrawlerState {
    pub sessions: Arc<Mutex<HashMap<String, CrawlSession>>>,
    pub pages: Arc<Mutex<HashMap<String, Vec<CrawlPage>>>>,
    pub insights: Arc<Mutex<HashMap<String, Vec<AIInsight>>>>,
    pub logs: Arc<Mutex<HashMap<String, Vec<ActivityLog>>>>,
    pub controls: Arc<Mutex<CrawlControls>>,
    pub cancellations: Arc<Mutex<CrawlCancellations>>,
}

pub type AiBrowserState = CrawlerState;
