//! Epoch-based concurrent read access for `MermaidDiagramIr`.
//!
//! Provides `EpochIrHandle`, a handle that allows lock-free concurrent reads of
//! graph IR while layout computation happens on a snapshot. Uses `Arc` + `RwLock`
//! for safe concurrency without `unsafe` code.
//!
//! # Design
//!
//! Each update to the IR increments a monotonic epoch counter. Readers obtain a
//! cheaply clonable `IrSnapshot` (an `Arc` reference) that is valid for the lifetime
//! of that epoch. Writers replace the current IR atomically via `update()`, which
//! increments the epoch and publishes the new IR. Old snapshots remain valid until
//! all readers drop their `Arc` references.
//!
//! This is a safe-Rust epoch-based reclamation pattern:
//! - Readers never block writers (they hold `Arc` clones, not locks).
//! - Writers never block readers (atomic `Arc` swap via write lock).
//! - Old IR data is reclaimed when the last reader drops its `Arc`.
//!
//! # Usage
//!
//! ```
//! use fm_core::epoch::EpochIrHandle;
//! use fm_core::MermaidDiagramIr;
//! use fm_core::DiagramType;
//!
//! let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));
//!
//! // Reader: obtain a snapshot (cheap Arc clone).
//! let snapshot = handle.snapshot();
//! assert_eq!(snapshot.epoch(), 0);
//! assert_eq!(snapshot.ir().nodes.len(), 0);
//!
//! // Writer: publish a new IR version.
//! let mut new_ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
//! new_ir.nodes.push(fm_core::IrNode {
//!     id: "A".to_string(),
//!     ..fm_core::IrNode::default()
//! });
//! handle.update(new_ir);
//!
//! // New snapshot reflects the update.
//! let snapshot2 = handle.snapshot();
//! assert_eq!(snapshot2.epoch(), 1);
//! assert_eq!(snapshot2.ir().nodes.len(), 1);
//!
//! // Old snapshot is still valid (independent Arc).
//! assert_eq!(snapshot.ir().nodes.len(), 0);
//! ```

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::MermaidDiagramIr;

/// A versioned snapshot of the IR, held as a reference-counted pointer.
///
/// Snapshots are cheap to create (Arc clone) and safe to hold across threads.
/// The IR data is reclaimed when all snapshots from that epoch are dropped.
#[derive(Debug, Clone)]
pub struct IrSnapshot {
    epoch: u64,
    ir: Arc<MermaidDiagramIr>,
}

impl IrSnapshot {
    /// The monotonic epoch at which this snapshot was created.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Reference to the IR at this epoch.
    #[must_use]
    pub fn ir(&self) -> &MermaidDiagramIr {
        &self.ir
    }

    /// Consume the snapshot and return the inner Arc.
    #[must_use]
    pub fn into_arc(self) -> Arc<MermaidDiagramIr> {
        self.ir
    }

    /// Number of live references to this epoch's IR data.
    /// Useful for diagnostics: if count > 1 after an update, old readers are still active.
    #[must_use]
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.ir)
    }
}

/// Thread-safe handle for epoch-based concurrent access to `MermaidDiagramIr`.
///
/// Multiple readers can hold `IrSnapshot` values concurrently while a writer
/// publishes new IR versions via `update()`. Old snapshots remain valid and
/// their backing data is reclaimed when the last reader drops its reference.
#[derive(Debug)]
pub struct EpochIrHandle {
    inner: RwLock<EpochInner>,
}

#[derive(Debug)]
struct EpochInner {
    epoch: u64,
    current: Arc<MermaidDiagramIr>,
}

impl EpochIrHandle {
    fn read_inner(&self) -> RwLockReadGuard<'_, EpochInner> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_inner(&self) -> RwLockWriteGuard<'_, EpochInner> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Create a new handle with the given initial IR at epoch 0.
    #[must_use]
    pub fn new(ir: MermaidDiagramIr) -> Self {
        Self {
            inner: RwLock::new(EpochInner {
                epoch: 0,
                current: Arc::new(ir),
            }),
        }
    }

    /// Obtain a snapshot of the current IR.
    ///
    /// This is a cheap operation (acquires read lock briefly, clones an `Arc`).
    /// The returned snapshot is valid indefinitely and does not block future updates.
    #[must_use]
    pub fn snapshot(&self) -> IrSnapshot {
        let inner = self.read_inner();
        IrSnapshot {
            epoch: inner.epoch,
            ir: Arc::clone(&inner.current),
        }
    }

    /// Publish a new IR version, incrementing the epoch.
    ///
    /// Previous snapshots remain valid. The old IR data is reclaimed when all
    /// outstanding snapshots from that epoch are dropped.
    pub fn update(&self, ir: MermaidDiagramIr) {
        let mut inner = self.write_inner();
        inner.epoch = inner.epoch.wrapping_add(1);
        inner.current = Arc::new(ir);
    }

    /// Current epoch number.
    #[must_use]
    pub fn current_epoch(&self) -> u64 {
        self.read_inner().epoch
    }

    /// Number of outstanding `Arc` references to the current epoch's IR.
    ///
    /// Returns 1 when only the handle itself holds a reference.
    #[must_use]
    pub fn current_ref_count(&self) -> usize {
        Arc::strong_count(&self.read_inner().current)
    }

    /// Try to reclaim old epochs. In this safe implementation, reclamation is
    /// automatic via `Arc` reference counting. This method returns diagnostic
    /// information about the current state.
    #[must_use]
    pub fn reclamation_status(&self) -> ReclamationStatus {
        let inner = self.read_inner();
        ReclamationStatus {
            current_epoch: inner.epoch,
            current_ref_count: Arc::strong_count(&inner.current),
        }
    }
}

/// Diagnostic information about epoch reclamation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclamationStatus {
    /// The current epoch number.
    pub current_epoch: u64,
    /// Number of outstanding references to the current epoch's IR.
    /// A count of 1 means only the handle holds a reference (no active readers).
    pub current_ref_count: usize,
}

impl Default for EpochIrHandle {
    fn default() -> Self {
        Self::new(MermaidDiagramIr::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagramType, IrNode};

    #[test]
    fn initial_epoch_is_zero() {
        let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));
        assert_eq!(handle.current_epoch(), 0);
    }

    #[test]
    fn snapshot_returns_current_ir() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        let handle = EpochIrHandle::new(ir);

        let snap = handle.snapshot();
        assert_eq!(snap.epoch(), 0);
        assert_eq!(snap.ir().nodes.len(), 1);
        assert_eq!(snap.ir().nodes[0].id, "A");
    }

    #[test]
    fn update_increments_epoch() {
        let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));
        assert_eq!(handle.current_epoch(), 0);

        handle.update(MermaidDiagramIr::empty(DiagramType::Sequence));
        assert_eq!(handle.current_epoch(), 1);

        handle.update(MermaidDiagramIr::empty(DiagramType::Class));
        assert_eq!(handle.current_epoch(), 2);
    }

    #[test]
    fn old_snapshot_remains_valid_after_update() {
        let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));

        let snap_v0 = handle.snapshot();
        assert_eq!(snap_v0.epoch(), 0);
        assert_eq!(snap_v0.ir().diagram_type, DiagramType::Flowchart);

        // Update to a new IR.
        let mut new_ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        new_ir.nodes.push(IrNode {
            id: "Seq".to_string(),
            ..IrNode::default()
        });
        handle.update(new_ir);

        // Old snapshot still valid.
        assert_eq!(snap_v0.epoch(), 0);
        assert_eq!(snap_v0.ir().diagram_type, DiagramType::Flowchart);
        assert_eq!(snap_v0.ir().nodes.len(), 0);

        // New snapshot reflects update.
        let snap_v1 = handle.snapshot();
        assert_eq!(snap_v1.epoch(), 1);
        assert_eq!(snap_v1.ir().diagram_type, DiagramType::Sequence);
        assert_eq!(snap_v1.ir().nodes.len(), 1);
    }

    #[test]
    fn ref_count_tracks_outstanding_snapshots() {
        let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));
        assert_eq!(handle.current_ref_count(), 1); // Only the handle.

        let snap1 = handle.snapshot();
        assert_eq!(handle.current_ref_count(), 2); // Handle + snap1.

        let snap2 = handle.snapshot();
        assert_eq!(handle.current_ref_count(), 3); // Handle + snap1 + snap2.

        drop(snap1);
        assert_eq!(handle.current_ref_count(), 2);

        drop(snap2);
        assert_eq!(handle.current_ref_count(), 1);
    }

    #[test]
    fn reclamation_status_reports_diagnostics() {
        let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));
        let status = handle.reclamation_status();
        assert_eq!(status.current_epoch, 0);
        assert_eq!(status.current_ref_count, 1);
    }

    #[test]
    fn concurrent_read_during_update() {
        use std::sync::Arc;
        use std::thread;

        let handle = Arc::new(EpochIrHandle::new(MermaidDiagramIr::empty(
            DiagramType::Flowchart,
        )));

        // Take a snapshot before spawning threads.
        let pre_update_snap = handle.snapshot();

        // Spawn reader threads that hold snapshots.
        let mut readers = Vec::new();
        for _ in 0..4 {
            let h = Arc::clone(&handle);
            readers.push(thread::spawn(move || {
                let snap = h.snapshot();
                // Simulate some work reading the IR.
                let _count = snap.ir().nodes.len();
                snap.epoch()
            }));
        }

        // Update while readers are active.
        let mut new_ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        new_ir.nodes.push(IrNode {
            id: "Updated".to_string(),
            ..IrNode::default()
        });
        handle.update(new_ir);

        // Collect reader results.
        for reader in readers {
            let epoch = reader.join().expect("reader thread panicked");
            // Readers may see epoch 0 or 1 depending on timing.
            assert!(epoch <= 1);
        }

        // Pre-update snapshot is still at epoch 0.
        assert_eq!(pre_update_snap.epoch(), 0);
        assert_eq!(pre_update_snap.ir().nodes.len(), 0);

        // Post-update snapshot is at epoch 1.
        let post = handle.snapshot();
        assert_eq!(post.epoch(), 1);
        assert_eq!(post.ir().nodes.len(), 1);
    }

    #[test]
    fn many_epochs_accumulate_correctly() {
        let handle = EpochIrHandle::new(MermaidDiagramIr::empty(DiagramType::Flowchart));

        for i in 0..1000 {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            ir.nodes.push(IrNode {
                id: format!("epoch_{i}"),
                ..IrNode::default()
            });
            handle.update(ir);
        }

        assert_eq!(handle.current_epoch(), 1000);
        let snap = handle.snapshot();
        assert_eq!(snap.ir().nodes[0].id, "epoch_999");
    }

    #[test]
    fn default_handle_has_empty_ir() {
        let handle = EpochIrHandle::default();
        let snap = handle.snapshot();
        assert_eq!(snap.epoch(), 0);
        assert!(snap.ir().nodes.is_empty());
    }
}
