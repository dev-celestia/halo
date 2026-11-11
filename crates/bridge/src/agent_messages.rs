//! Cheap immutable transcript snapshots. Only the changed row is copy-on-write.
use crate::work::HaloAgentMessage;
use std::{
    ops::{Deref, Range},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default)]
pub struct HaloAgentMessages {
    rows: Arc<Vec<Arc<HaloAgentMessage>>>,
    revision: u64,
    changed_rows: Option<Range<usize>>,
}

impl PartialEq for HaloAgentMessages {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
    }
}
impl Eq for HaloAgentMessages {}

impl Deref for HaloAgentMessages {
    type Target = [Arc<HaloAgentMessage>];
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}
impl<'a> IntoIterator for &'a HaloAgentMessages {
    type Item = &'a Arc<HaloAgentMessage>;
    type IntoIter = std::slice::Iter<'a, Arc<HaloAgentMessage>>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}
impl From<Vec<HaloAgentMessage>> for HaloAgentMessages {
    fn from(rows: Vec<HaloAgentMessage>) -> Self {
        let mut result = Self::default();
        for row in rows {
            result.push(row);
        }
        result
    }
}
impl HaloAgentMessages {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Rows changed since the start of the latest backend batch. Consumers that
    /// skipped revisions must invalidate their visible rows rather than rely on this range.
    pub fn changed_rows(&self) -> Option<Range<usize>> {
        self.changed_rows.clone()
    }
    pub(crate) fn begin_batch(&mut self) {
        self.changed_rows = None;
    }
    fn touch(&mut self, range: Range<usize>) {
        self.revision = NEXT_REVISION.fetch_add(1, Ordering::Relaxed);
        self.changed_rows = Some(match self.changed_rows.take() {
            Some(old) => old.start.min(range.start)..old.end.max(range.end),
            None => range,
        });
    }
    pub fn push(&mut self, message: HaloAgentMessage) {
        let row = self.len();
        self.touch(row..row + 1);
        Arc::make_mut(&mut self.rows).push(Arc::new(message));
    }
    pub fn pop(&mut self) -> Option<Arc<HaloAgentMessage>> {
        let row = self.len().checked_sub(1)?;
        self.touch(row..row + 1);
        Arc::make_mut(&mut self.rows).pop()
    }
    pub fn clear(&mut self) {
        if self.is_empty() {
            return;
        }
        self.touch(0..self.len());
        self.rows = Arc::default();
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut HaloAgentMessage> {
        if index >= self.len() {
            return None;
        }
        self.touch(index..index + 1);
        Some(Arc::make_mut(&mut Arc::make_mut(&mut self.rows)[index]))
    }
    pub fn last_mut(&mut self) -> Option<&mut HaloAgentMessage> {
        let index = self.len().checked_sub(1)?;
        self.get_mut(index)
    }
}
