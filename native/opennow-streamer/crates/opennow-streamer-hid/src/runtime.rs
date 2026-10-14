use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    InventoryOutcome, InventoryView, MAX_QUEUED_STATE_BYTES, MAX_QUEUED_STATES, MAX_SOURCES,
    SdlDeviceClaim, SnapshotAdmission, SonySnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseRequest {
    pub slot: u8,
    pub incarnation: u64,
    pub observed_at_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneFault {
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceMode {
    Rich,
    OrdinaryFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointToken(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngressItem {
    Snapshot(SonySnapshot),
    Release {
        slot: u8,
        incarnation: u64,
        observed_at_us: u64,
    },
    Fault {
        slot: u8,
        incarnation: u64,
        observed_at_us: u64,
    },
}

#[derive(Debug, Default)]
struct SourceLane {
    states: VecDeque<SonySnapshot>,
    bytes: usize,
    release: Option<ReleaseRequest>,
    fault: Option<LaneFault>,
    last_observed_at_us: u64,
    dropped: u64,
}

impl SourceLane {
    fn reset(&mut self) {
        self.states.clear();
        self.bytes = 0;
        self.release = None;
        self.fault = None;
        self.last_observed_at_us = 0;
    }

    fn drop_buffered(&mut self) {
        self.states.clear();
        self.bytes = 0;
    }

    fn arm_release(&mut self, slot: u8, incarnation: u64) {
        if self.release.is_none() {
            self.release = Some(ReleaseRequest {
                slot,
                incarnation,
                observed_at_us: self.last_observed_at_us,
            });
        }
    }

    fn push(&mut self, snapshot: SonySnapshot) -> SnapshotAdmission {
        if self.fault.is_some() {
            self.dropped = self.dropped.saturating_add(1);
            return SnapshotAdmission::Faulted;
        }
        self.last_observed_at_us = self.last_observed_at_us.max(snapshot.observed_at_us);
        let bytes = core::mem::size_of::<SonySnapshot>();
        if self.states.len() >= MAX_QUEUED_STATES
            || self.bytes.saturating_add(bytes) > MAX_QUEUED_STATE_BYTES
        {
            self.drop_buffered();
            self.fault = Some(LaneFault::Overflow);
            self.dropped = self.dropped.saturating_add(1);
            self.arm_release(snapshot.slot, snapshot.incarnation);
            return SnapshotAdmission::Overflow;
        }
        self.bytes += bytes;
        self.states.push_back(snapshot);
        SnapshotAdmission::Admitted
    }
}

#[derive(Debug, Default)]
struct RuntimeState {
    binding: Option<u64>,
    capture_active: bool,
    inventory: InventoryView,
    inventory_epoch: u64,
    endpoint: Option<u64>,
    lanes: [SourceLane; MAX_SOURCES],
    session_evaluated_epoch: Option<u64>,
    session_admitted: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionBinding {
    pub generation: u64,
}

#[derive(Debug, Default)]
pub struct HidRuntime {
    pub generic: crate::generic_runtime::GenericRuntime,
    state: Mutex<RuntimeState>,
    incarnation_source: AtomicU64,
    inventory_epoch: AtomicU64,
    endpoint_source: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryUpdate {
    pub outcome: InventoryOutcome,
    pub epoch: u64,
}

impl HidRuntime {
    pub fn new() -> Self {
        Self {
            generic: crate::generic_runtime::GenericRuntime::new(),
            state: Mutex::new(RuntimeState::default()),
            incarnation_source: AtomicU64::new(1),
            inventory_epoch: AtomicU64::new(0),
            endpoint_source: AtomicU64::new(1),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn next_incarnation(&self) -> u64 {
        self.incarnation_source.fetch_add(1, Ordering::AcqRel)
    }

    pub fn inventory_epoch(&self) -> u64 {
        self.inventory_epoch.load(Ordering::Acquire)
    }

    pub fn inventory(&self) -> InventoryView {
        self.lock().inventory
    }

    pub fn replace_inventory(&self, claims: &[Option<SdlDeviceClaim>]) -> InventoryUpdate {
        let mut state = self.lock();
        let previous = state.inventory;
        let outcome = state.inventory.replace(claims);
        if outcome != InventoryOutcome::Replaced {
            return InventoryUpdate {
                outcome,
                epoch: state.inventory_epoch,
            };
        }
        for slot in 0..MAX_SOURCES as u8 {
            if previous.incarnation(slot) == state.inventory.incarnation(slot) {
                continue;
            }
            state.lanes[usize::from(slot)].reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.inventory_epoch = state.inventory_epoch.wrapping_add(1);
        let epoch = state.inventory_epoch;
        self.inventory_epoch.store(epoch, Ordering::Release);
        InventoryUpdate { outcome, epoch }
    }

    pub fn inventory_snapshot(&self) -> (InventoryView, u64) {
        let state = self.lock();
        (state.inventory, state.inventory_epoch)
    }

    pub fn open_endpoint(&self) -> EndpointToken {
        let mut state = self.lock();
        if let Some(generation) = state.binding.take() {
            self.generic.unbind_session(generation);
        }
        let token = self.endpoint_source.fetch_add(1, Ordering::AcqRel);
        for lane in state.lanes.iter_mut() {
            lane.reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.endpoint = Some(token);
        EndpointToken(token)
    }

    pub fn close_endpoint(&self, token: EndpointToken) {
        let mut state = self.lock();
        if state.endpoint != Some(token.0) {
            return;
        }
        if let Some(generation) = state.binding {
            self.generic.unbind_session(generation);
        }
        state.endpoint = None;
        for lane in state.lanes.iter_mut() {
            lane.reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.binding = None;
    }

    pub fn bind_session(&self, generation: u64) -> Option<SessionBinding> {
        let mut state = self.lock();
        state.endpoint?;
        self.generic.bind_session(generation);
        for lane in state.lanes.iter_mut() {
            lane.reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.binding = Some(generation);
        Some(SessionBinding { generation })
    }

    pub fn unbind_session(&self, generation: u64) {
        let mut state = self.lock();
        if state.binding.is_some_and(|bound| bound != generation) {
            return;
        }
        self.generic.unbind_session(generation);
        for lane in state.lanes.iter_mut() {
            lane.reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.binding = None;
    }

    pub fn session_generation(&self) -> Option<u64> {
        self.lock().binding
    }

    pub fn session_generation_for_endpoint(&self, endpoint: EndpointToken) -> Option<u64> {
        let state = self.lock();
        if state.endpoint == Some(endpoint.0) {
            state.binding
        } else {
            None
        }
    }

    pub fn set_session_admission(&self, admitted: u16, epoch: u64) -> bool {
        let mut state = self.lock();
        if epoch != state.inventory_epoch {
            return false;
        }
        state.session_evaluated_epoch = Some(epoch);
        state.session_admitted = admitted;
        true
    }

    fn mode_for_epoch(state: &RuntimeState, slot: u8) -> Option<SourceMode> {
        if state.session_evaluated_epoch != Some(state.inventory_epoch) {
            return None;
        }
        state.inventory.incarnation(slot)?;
        Some(if state.session_admitted & (1 << slot) != 0 {
            SourceMode::Rich
        } else {
            SourceMode::OrdinaryFallback
        })
    }

    pub fn source_mode(&self, slot: u8) -> Option<SourceMode> {
        Self::mode_for_epoch(&self.lock(), slot)
    }

    pub fn set_active(&self, active: bool) {
        let mut state = self.lock();
        self.generic.set_capture(active);
        if state.capture_active == active {
            return;
        }
        state.capture_active = active;
        if active {
            return;
        }
        for slot in 0..MAX_SOURCES as u8 {
            let Some(incarnation) = state.inventory.incarnation(slot) else {
                continue;
            };
            let lane = &mut state.lanes[usize::from(slot)];
            lane.drop_buffered();
            lane.arm_release(slot, incarnation);
        }
    }

    pub fn is_active(&self) -> bool {
        self.lock().capture_active
    }

    pub fn output_admitted(&self, slot: u8) -> bool {
        let state = self.lock();
        if !state.capture_active || state.binding.is_none() {
            return false;
        }
        if state.inventory.incarnation(slot).is_none() {
            return false;
        }
        state
            .lanes
            .get(usize::from(slot))
            .is_some_and(|lane| lane.fault.is_none())
    }

    pub fn submit_snapshot_with<F>(
        &self,
        snapshot: SonySnapshot,
        admit_ordinary: F,
    ) -> SnapshotAdmission
    where
        F: FnOnce(&SonySnapshot, u16),
    {
        let snapshot = match snapshot.validate() {
            Ok(snapshot) => snapshot,
            Err(_) => return SnapshotAdmission::StaleSource,
        };
        let mut state = self.lock();
        if state.binding.is_none() {
            return SnapshotAdmission::Unbound;
        }
        let Some(incarnation) = state.inventory.incarnation(snapshot.slot) else {
            return SnapshotAdmission::StaleSource;
        };
        if incarnation != snapshot.incarnation {
            return SnapshotAdmission::StaleSource;
        }
        let Some(mode) = Self::mode_for_epoch(&state, snapshot.slot) else {
            return SnapshotAdmission::StaleSource;
        };
        if mode == SourceMode::OrdinaryFallback {
            let mut bitmap = 0_u16;
            for slot in 0..MAX_SOURCES as u8 {
                if state.inventory.incarnation(slot).is_some() {
                    bitmap |= (1_u16 << slot) | (1_u16 << (slot + 8));
                }
            }
            admit_ordinary(&snapshot, bitmap);
            return SnapshotAdmission::Admitted;
        }
        let active = state.capture_active;
        let lane = &mut state.lanes[usize::from(snapshot.slot)];
        lane.last_observed_at_us = lane.last_observed_at_us.max(snapshot.observed_at_us);
        if !active {
            if snapshot.is_neutral() {
                lane.arm_release(snapshot.slot, snapshot.incarnation);
            }
            return SnapshotAdmission::AdmittedInactive;
        }
        lane.push(snapshot)
    }

    pub fn submit_snapshot(&self, snapshot: SonySnapshot) -> SnapshotAdmission {
        self.submit_snapshot_with(snapshot, |_, _| {})
    }

    pub fn drain(&self, budget: usize) -> Vec<IngressItem> {
        let mut drained = Vec::with_capacity(budget.min(crate::DRAIN_PER_ITERATION));
        let mut state = self.lock();
        for lane in state.lanes.iter_mut() {
            if drained.len() >= budget {
                break;
            }
            if let Some(release) = lane.release.take() {
                let item = match lane.fault.take() {
                    Some(_) => IngressItem::Fault {
                        slot: release.slot,
                        incarnation: release.incarnation,
                        observed_at_us: release.observed_at_us,
                    },
                    None => IngressItem::Release {
                        slot: release.slot,
                        incarnation: release.incarnation,
                        observed_at_us: release.observed_at_us,
                    },
                };
                drained.push(item);
                if drained.len() >= budget {
                    break;
                }
            }
            while drained.len() < budget {
                let Some(snapshot) = lane.states.pop_front() else {
                    break;
                };
                lane.bytes = lane
                    .bytes
                    .saturating_sub(core::mem::size_of::<SonySnapshot>());
                drained.push(IngressItem::Snapshot(snapshot));
            }
        }
        drained
    }

    pub fn pending_states(&self) -> usize {
        self.lock().lanes.iter().map(|lane| lane.states.len()).sum()
    }

    pub fn dropped_states(&self) -> u64 {
        self.lock().lanes.iter().map(|lane| lane.dropped).sum()
    }

    pub fn faulted_sources(&self) -> u16 {
        let mut mask = 0_u16;
        for (slot, lane) in self.lock().lanes.iter().enumerate() {
            if lane.fault.is_some() {
                mask |= 1 << slot;
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SONY_VENDOR, SonyContact};

    fn ds4(slot: u8, incarnation: u64) -> Option<SdlDeviceClaim> {
        SdlDeviceClaim::new(slot, incarnation, SONY_VENDOR, 0x05c4)
    }

    fn runtime_with(claims: [Option<SdlDeviceClaim>; 4]) -> HidRuntime {
        let runtime = HidRuntime::new();
        assert_eq!(
            runtime.replace_inventory(&claims).outcome,
            InventoryOutcome::Replaced
        );
        runtime.open_endpoint();
        assert!(runtime.bind_session(1).is_some());
        publish_rich_admission(&runtime, &claims);
        runtime
    }

    fn publish_rich_admission(runtime: &HidRuntime, claims: &[Option<SdlDeviceClaim>; 4]) {
        let rich = claims
            .iter()
            .flatten()
            .filter(|claim| claim.sony_product().is_some())
            .fold(0_u16, |mask, claim| mask | (1 << claim.slot));
        assert!(runtime.set_session_admission(rich, runtime.inventory_snapshot().1));
    }

    fn snapshot(slot: u8, incarnation: u64) -> SonySnapshot {
        SonySnapshot::neutral(slot, incarnation, 0)
    }

    #[test]
    fn incarnation_source_never_repeats() {
        let runtime = HidRuntime::new();
        let first = runtime.next_incarnation();
        let second = runtime.next_incarnation();
        assert_ne!(first, second);
        assert!(second > first);
    }

    #[test]
    fn unsupported_inventory_replacement_leaves_the_previous_view_intact() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        let before = runtime.inventory_epoch();
        let mut duplicate = [None; 4];
        duplicate[0] = ds4(2, 9);
        duplicate[1] = Some(SdlDeviceClaim {
            slot: 2,
            incarnation: 10,
            vendor: SONY_VENDOR,
            product: 0x05c4,
        });
        assert_eq!(
            runtime.replace_inventory(&duplicate).outcome,
            InventoryOutcome::DuplicateSlot
        );
        assert_eq!(runtime.inventory_epoch(), before);
        assert_eq!(runtime.inventory().incarnation(0), Some(1));
        assert_eq!(runtime.inventory().incarnation(2), None);
    }

    #[test]
    fn snapshots_for_unknown_or_replaced_sources_are_rejected() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        runtime.set_active(true);
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 2)),
            SnapshotAdmission::StaleSource
        );
        assert_eq!(
            runtime.submit_snapshot(snapshot(1, 1)),
            SnapshotAdmission::StaleSource
        );
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Admitted
        );
    }

    #[test]
    fn inactive_capture_drops_non_neutral_snapshots_but_keeps_the_release() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        let mut active_snapshot = snapshot(0, 1);
        active_snapshot.left_stick_x = 4096;
        assert_eq!(
            runtime.submit_snapshot(active_snapshot),
            SnapshotAdmission::AdmittedInactive
        );
        assert!(runtime.drain(8).is_empty());
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::AdmittedInactive
        );
        assert_eq!(
            runtime.drain(8),
            vec![IngressItem::Release {
                slot: 0,
                incarnation: 1,
                observed_at_us: 0,
            }]
        );
    }

    #[test]
    fn capture_loss_arms_one_release_per_source_that_resume_preserves_until_drained() {
        let runtime = runtime_with([
            Some(ds4(0, 1).unwrap()),
            None,
            Some(ds4(2, 3).unwrap()),
            None,
        ]);
        runtime.set_active(true);
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Admitted
        );
        runtime.set_active(false);
        runtime.set_active(false);
        let items = runtime.drain(8);
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .all(|item| matches!(item, IngressItem::Release { .. }))
        );
    }

    #[test]
    fn repeated_capture_toggles_do_not_accumulate_duplicate_releases() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        runtime.set_active(true);
        for _ in 0..64 {
            runtime.set_active(false);
            runtime.set_active(true);
            let items = runtime.drain(8);
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0],
                IngressItem::Release {
                    slot: 0,
                    incarnation: 1,
                    observed_at_us: 0,
                }
            );
        }
    }

    #[test]
    fn overflow_faults_the_source_and_keeps_the_release_until_consumed() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        runtime.set_active(true);
        for _ in 0..MAX_QUEUED_STATES {
            assert_eq!(
                runtime.submit_snapshot(snapshot(0, 1)),
                SnapshotAdmission::Admitted
            );
        }
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Overflow
        );
        assert!(runtime.dropped_states() >= 1);
        assert_eq!(runtime.faulted_sources(), 0b0001);
        assert!(!runtime.output_admitted(0));
        assert_eq!(
            runtime.submit_snapshot({
                let mut held = snapshot(0, 1);
                held.buttons = 0x1000;
                held
            }),
            SnapshotAdmission::Faulted
        );
        let items = runtime.drain(crate::MAX_QUEUED_STATES + 8);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], IngressItem::Fault { slot: 0, .. }));
        assert_eq!(runtime.pending_states(), 0);
        assert_eq!(runtime.faulted_sources(), 0);
        assert!(runtime.output_admitted(0));
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Admitted
        );
    }

    #[test]
    fn unbound_sessions_reject_admission_and_drop_queued_input() {
        let runtime = HidRuntime::new();
        assert_eq!(
            runtime
                .replace_inventory(&[Some(ds4(0, 1).unwrap()), None, None, None])
                .outcome,
            InventoryOutcome::Replaced
        );
        runtime.set_active(true);
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Unbound
        );
        assert_eq!(runtime.session_generation(), None);
        assert!(!runtime.output_admitted(0));
        runtime.open_endpoint();
        assert_eq!(runtime.bind_session(7).unwrap().generation, 7);
        assert_eq!(runtime.session_generation(), Some(7));
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::StaleSource
        );
        assert!(runtime.set_session_admission(0b0001, runtime.inventory_snapshot().1));
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Admitted
        );
        assert!(
            runtime
                .drain(8)
                .iter()
                .any(|item| matches!(item, IngressItem::Snapshot(_)))
        );
        assert!(runtime.output_admitted(0));
        runtime.unbind_session(8);
        assert_eq!(runtime.session_generation(), Some(7));
        runtime.unbind_session(7);
        assert_eq!(runtime.session_generation(), None);
        assert!(runtime.drain(8).is_empty());
        assert_eq!(runtime.pending_states(), 0);
        assert!(!runtime.output_admitted(0));
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::Unbound
        );
    }

    #[test]
    fn replacing_an_endpoint_waits_for_a_new_accepted_generation() {
        let runtime = HidRuntime::new();
        let old = runtime.open_endpoint();
        runtime.bind_session(7).unwrap();
        let replacement = runtime.open_endpoint();
        assert_eq!(runtime.session_generation(), None);
        runtime.bind_session(9).unwrap();
        runtime.close_endpoint(old);
        assert_eq!(runtime.session_generation(), Some(9));
        runtime.close_endpoint(replacement);
        assert_eq!(runtime.session_generation(), None);
    }

    #[test]
    fn output_admission_requires_capture_a_binding_and_a_healthy_source() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        assert!(!runtime.output_admitted(0));
        runtime.set_active(true);
        assert!(runtime.output_admitted(0));
        assert!(!runtime.output_admitted(1));
        runtime.set_active(false);
        assert!(!runtime.output_admitted(0));
    }

    #[test]
    fn drain_budget_bounds_work_per_iteration() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        runtime.set_active(true);
        for _ in 0..MAX_QUEUED_STATES {
            let _ = runtime.submit_snapshot(snapshot(0, 1));
        }
        let first = runtime.drain(32);
        assert_eq!(first.len(), 32);
        assert_eq!(runtime.pending_states(), MAX_QUEUED_STATES - 32);
        let second = runtime.drain(32);
        assert_eq!(second.len(), 32);
        assert_eq!(runtime.pending_states(), 0);
    }

    #[test]
    fn inventory_replacement_resets_the_lane_and_drops_previous_states() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        runtime.set_active(true);
        for _ in 0..8 {
            let _ = runtime.submit_snapshot(snapshot(0, 1));
        }
        runtime.set_active(false);
        assert!(
            runtime
                .replace_inventory(&[None, Some(ds4(1, 2).unwrap()), None, None])
                .outcome
                == InventoryOutcome::Replaced
        );
        assert_eq!(runtime.pending_states(), 0);
        assert!(runtime.drain(8).is_empty());
    }

    #[test]
    fn repeated_neutral_reports_arm_one_release_until_drained() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        runtime.set_active(true);
        runtime.set_active(false);
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::AdmittedInactive
        );
        assert_eq!(
            runtime.submit_snapshot(snapshot(0, 1)),
            SnapshotAdmission::AdmittedInactive
        );
        assert_eq!(runtime.drain(8).len(), 1);
        assert!(runtime.drain(8).is_empty());
    }

    #[test]
    fn neutral_snapshot_keeps_contacts_but_is_recognized_as_a_release() {
        let runtime = runtime_with([Some(ds4(0, 1).unwrap()), None, None, None]);
        let mut neutral = snapshot(0, 1);
        neutral.contacts[0] = SonyContact {
            active: false,
            x: 100,
            y: -100,
        };
        assert_eq!(
            runtime.submit_snapshot(neutral),
            SnapshotAdmission::AdmittedInactive
        );
        assert_eq!(runtime.drain(8).len(), 1);
    }
}
