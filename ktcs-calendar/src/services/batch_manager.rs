//! Batch manager for aggregating timestamp requests
//!
//! Collects digests and groups them into batches based on the configured
//! batching windows (instant, standard, economic).

use ktcs_core::BatchMode;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A pending stamp waiting to be batched
#[derive(Debug, Clone)]
pub struct PendingStamp {
    pub id: String,
    pub digest: [u8; 32],
    pub submitted_at: Instant,
    pub mode: BatchMode,
}

/// Batch of stamps ready for commitment
#[derive(Debug)]
pub struct Batch {
    pub mode: BatchMode,
    pub stamps: Vec<PendingStamp>,
    pub started_at: Instant,
}

/// Manages batching of timestamp requests
pub struct BatchManager {
    /// Pending stamps grouped by batch mode
    pending: HashMap<BatchMode, Vec<PendingStamp>>,
    /// When each batch mode started collecting
    batch_start: HashMap<BatchMode, Instant>,
}

impl BatchManager {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            batch_start: HashMap::new(),
        }
    }

    /// Add a new digest to be batched
    pub fn add_digest(&mut self, id: String, digest: [u8; 32], mode: BatchMode) {
        let stamp = PendingStamp {
            id,
            digest,
            submitted_at: Instant::now(),
            mode,
        };

        let pending = self.pending.entry(mode).or_insert_with(Vec::new);

        // Start batch timer if this is the first stamp
        if pending.is_empty() {
            self.batch_start.insert(mode, Instant::now());
        }

        pending.push(stamp);
    }

    /// Get batches that are ready to commit
    pub fn get_ready_batches(&mut self) -> Vec<(BatchMode, Vec<PendingStamp>)> {
        let mut ready = Vec::new();
        let now = Instant::now();

        for mode in [BatchMode::Instant, BatchMode::Standard, BatchMode::Economic] {
            if let Some(start) = self.batch_start.get(&mode) {
                let window = Duration::from_millis(mode.window_ms());

                if now.duration_since(*start) >= window {
                    if let Some(stamps) = self.pending.remove(&mode) {
                        if !stamps.is_empty() {
                            ready.push((mode, stamps));
                        }
                    }
                    self.batch_start.remove(&mode);
                }
            }
        }

        ready
    }

    /// Get count of pending stamps
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }

    /// Get count of pending stamps by mode
    pub fn pending_count_by_mode(&self, mode: BatchMode) -> usize {
        self.pending.get(&mode).map(|v| v.len()).unwrap_or(0)
    }
}

impl Default for BatchManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_digest() {
        let mut manager = BatchManager::new();

        manager.add_digest("test1".to_string(), [0x01; 32], BatchMode::Standard);
        manager.add_digest("test2".to_string(), [0x02; 32], BatchMode::Standard);
        manager.add_digest("test3".to_string(), [0x03; 32], BatchMode::Instant);

        assert_eq!(manager.pending_count(), 3);
        assert_eq!(manager.pending_count_by_mode(BatchMode::Standard), 2);
        assert_eq!(manager.pending_count_by_mode(BatchMode::Instant), 1);
    }

    #[test]
    fn test_instant_batch_ready() {
        let mut manager = BatchManager::new();

        manager.add_digest("test1".to_string(), [0x01; 32], BatchMode::Instant);

        // Immediately shouldn't be ready
        let ready = manager.get_ready_batches();
        assert!(ready.is_empty());

        // After 100ms should be ready
        std::thread::sleep(Duration::from_millis(150));

        let ready = manager.get_ready_batches();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, BatchMode::Instant);
        assert_eq!(ready[0].1.len(), 1);
    }
}
