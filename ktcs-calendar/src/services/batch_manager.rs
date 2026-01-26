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
    #[allow(dead_code)] // TODO: Used for batch window timing analysis
    pub submitted_at: Instant,
    #[allow(dead_code)] // TODO: Used for batch mode filtering
    pub mode: BatchMode,
}

/// Batch of stamps ready for commitment
#[derive(Debug)]
#[allow(dead_code)] // TODO: Returned from get_ready_batches for processing
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

        let pending = self.pending.entry(mode).or_default();

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
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }

    /// Get count of pending stamps by mode
    #[allow(dead_code)]
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

    #[test]
    fn test_batch_window_by_mode() {
        // Instant: 100ms
        assert_eq!(BatchMode::Instant.window_ms(), 100);
        // Standard: 1000ms
        assert_eq!(BatchMode::Standard.window_ms(), 1000);
        // Economic: 10000ms (10 seconds)
        assert_eq!(BatchMode::Economic.window_ms(), 10000);
    }

    #[test]
    fn test_pending_stamp_creation() {
        let stamp = PendingStamp {
            id: "test".to_string(),
            digest: [0xab; 32],
            submitted_at: Instant::now(),
            mode: BatchMode::Instant,
        };

        assert_eq!(stamp.id, "test");
        assert_eq!(stamp.digest, [0xab; 32]);
        assert_eq!(stamp.mode, BatchMode::Instant);
    }

    #[test]
    fn test_batch_manager_new() {
        let bm = BatchManager::new();
        assert_eq!(bm.pending_count(), 0);
        assert_eq!(bm.pending_count_by_mode(BatchMode::Instant), 0);
        assert_eq!(bm.pending_count_by_mode(BatchMode::Standard), 0);
        assert_eq!(bm.pending_count_by_mode(BatchMode::Economic), 0);
    }

    #[test]
    fn test_get_ready_batches_empty() {
        let mut bm = BatchManager::new();
        let batches = bm.get_ready_batches();
        assert!(batches.is_empty());
    }

    #[test]
    fn test_batch_manager_default() {
        let bm = BatchManager::default();
        assert_eq!(bm.pending_count(), 0);
    }

    #[test]
    fn test_batch_mode_separation() {
        let mut manager = BatchManager::new();

        // Add stamps with different modes
        manager.add_digest("inst".to_string(), [0x01; 32], BatchMode::Instant);
        manager.add_digest("std".to_string(), [0x02; 32], BatchMode::Standard);
        manager.add_digest("eco".to_string(), [0x03; 32], BatchMode::Economic);

        // Verify counts by mode
        assert_eq!(manager.pending_count_by_mode(BatchMode::Instant), 1);
        assert_eq!(manager.pending_count_by_mode(BatchMode::Standard), 1);
        assert_eq!(manager.pending_count_by_mode(BatchMode::Economic), 1);
        assert_eq!(manager.pending_count(), 3);
    }

    #[test]
    fn test_batch_accumulation() {
        let mut manager = BatchManager::new();

        // Add multiple stamps to same mode
        for i in 0..5 {
            manager.add_digest(format!("stamp_{}", i), [i as u8; 32], BatchMode::Instant);
        }

        assert_eq!(manager.pending_count_by_mode(BatchMode::Instant), 5);

        // Wait for batch to be ready
        std::thread::sleep(Duration::from_millis(150));

        // All should be in single batch
        let batches = manager.get_ready_batches();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].1.len(), 5);

        // Pending should be cleared
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_batch_cleared_after_get_ready() {
        let mut manager = BatchManager::new();

        manager.add_digest("test1".to_string(), [0x01; 32], BatchMode::Instant);
        assert_eq!(manager.pending_count(), 1);

        // Wait for batch to be ready
        std::thread::sleep(Duration::from_millis(150));

        // Get ready batches
        let ready = manager.get_ready_batches();
        assert_eq!(ready.len(), 1);

        // Pending should now be empty
        assert_eq!(manager.pending_count(), 0);

        // Next call should return empty
        let ready = manager.get_ready_batches();
        assert!(ready.is_empty());
    }
}
