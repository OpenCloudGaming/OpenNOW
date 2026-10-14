use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::generic::{
    GENERIC_LANE_CAPACITY, GENERIC_OUTPUT_MAILBOX, GenericAdmission, GenericCounters,
    GenericHidDevice, GenericOutputReport, GenericReport, MAX_GENERIC_DEVICES,
    generic_request_index,
};
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

/// Bounded ingress/egress buffers for one forwarded generic HID device.
///
/// Unlike [`SourceLane`], overflow never faults the lane: raw reports are
/// absolute device state, so dropping the oldest buffered report is always
/// safe and the newest one wins.
#[derive(Debug)]
struct GenericLane {
    device: GenericHidDevice,
    forwarding: bool,
    /// Published but not yet opened by the reader thread: absent from the
    /// snapshot so no attach can be sent until `mark_generic_live`.
    dormant: bool,
    removed: bool,
    removal_pending: bool,
    reports: VecDeque<GenericReport>,
    outputs: VecDeque<GenericOutputReport>,
    counters: GenericCounters,
}

impl GenericLane {
    fn new(device: GenericHidDevice) -> Self {
        Self {
            device,
            forwarding: false,
            dormant: false,
            removed: false,
            removal_pending: false,
            reports: VecDeque::with_capacity(GENERIC_LANE_CAPACITY),
            outputs: VecDeque::with_capacity(GENERIC_OUTPUT_MAILBOX),
            counters: GenericCounters::default(),
        }
    }

    fn drop_buffered(&mut self) {
        self.reports.clear();
        self.outputs.clear();
    }

    fn detach(&mut self) {
        self.forwarding = false;
        self.drop_buffered();
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
    generic: [Option<GenericLane>; MAX_GENERIC_DEVICES],
    generic_epoch: u64,
}

impl RuntimeState {
    fn generic_lane(&mut self, request_id: u8) -> Option<&mut GenericLane> {
        self.generic.get_mut(generic_request_index(request_id)?)?.as_mut()
    }

    fn detach_generic(&mut self) {
        for lane in self.generic.iter_mut().flatten() {
            lane.detach();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionBinding {
    pub generation: u64,
}

#[derive(Debug, Default)]
pub struct HidRuntime {
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
        state.endpoint = None;
        for lane in state.lanes.iter_mut() {
            lane.reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.binding = None;
        state.detach_generic();
    }

    pub fn bind_session(&self, generation: u64) -> Option<SessionBinding> {
        let mut state = self.lock();
        state.endpoint?;
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
        for lane in state.lanes.iter_mut() {
            lane.reset();
        }
        state.session_evaluated_epoch = None;
        state.session_admitted = 0;
        state.binding = None;
        state.detach_generic();
    }

    pub fn session_generation(&self) -> Option<u64> {
        self.lock().binding
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
        // Generic lanes have no descriptor knowledge, so no neutral report can
        // be synthesized on focus loss; buffered reports are simply dropped.
        for lane in state.generic.iter_mut().flatten() {
            let dropped = lane.reports.len() as u64;
            lane.reports.clear();
            lane.counters.dropped_inactive = lane.counters.dropped_inactive.saturating_add(dropped);
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

    // ---- generic raw-HID lanes -------------------------------------------

    /// Replace the generic device inventory. Every accepted device receives a
    /// fresh incarnation; existing lanes (and their buffers) are discarded.
    /// Returns the devices as stored (with incarnations) and the new epoch.
    pub fn replace_generic_inventory(
        &self,
        devices: &[GenericHidDevice],
    ) -> (Vec<GenericHidDevice>, u64) {
        self.replace_generic_inventory_with(devices, false)
    }

    /// [`replace_generic_inventory`](Self::replace_generic_inventory) whose
    /// lanes start *dormant* (absent from the snapshot, so no attach can be
    /// sent) until the reader thread has actually opened the node and calls
    /// [`mark_generic_live`](Self::mark_generic_live). A node that fails to
    /// open simply stays dormant; nothing is ever announced for it.
    pub fn replace_generic_inventory_dormant(
        &self,
        devices: &[GenericHidDevice],
    ) -> (Vec<GenericHidDevice>, u64) {
        self.replace_generic_inventory_with(devices, true)
    }

    fn replace_generic_inventory_with(
        &self,
        devices: &[GenericHidDevice],
        dormant: bool,
    ) -> (Vec<GenericHidDevice>, u64) {
        let mut state = self.lock();
        let mut next: [Option<GenericLane>; MAX_GENERIC_DEVICES] = Default::default();
        let mut stored = Vec::with_capacity(MAX_GENERIC_DEVICES);
        for device in devices {
            let Some(index) = generic_request_index(device.request_id) else {
                continue;
            };
            if next[index].is_some() {
                continue;
            }
            let mut device = device.clone();
            device.incarnation = self.next_incarnation();
            stored.push(device.clone());
            let mut lane = GenericLane::new(device);
            lane.dormant = dormant;
            next[index] = Some(lane);
        }
        state.generic = next;
        state.generic_epoch = state.generic_epoch.wrapping_add(1);
        (stored, state.generic_epoch)
    }

    /// Flip a dormant lane live once its node is open: it enters the
    /// snapshot and the epoch moves so the bundle pump sends the attach.
    /// Returns `false` for unknown lanes, lanes that are already live, or a
    /// stale incarnation.
    pub fn mark_generic_live(&self, request_id: u8, incarnation: u64) -> bool {
        let mut state = self.lock();
        let Some(lane) = state.generic_lane(request_id) else {
            return false;
        };
        if !lane.dormant || lane.removed || lane.device.incarnation != incarnation {
            return false;
        }
        lane.dormant = false;
        state.generic_epoch = state.generic_epoch.wrapping_add(1);
        true
    }

    pub fn generic_inventory_snapshot(&self) -> (Vec<GenericHidDevice>, u64) {
        let state = self.lock();
        let devices = state
            .generic
            .iter()
            .flatten()
            .filter(|lane| !lane.removed && !lane.dormant)
            .map(|lane| lane.device.clone())
            .collect();
        (devices, state.generic_epoch)
    }

    pub fn generic_inventory_epoch(&self) -> u64 {
        self.lock().generic_epoch
    }

    /// Submit one raw input report from the reader thread. Overflow evicts
    /// the oldest buffered report and admits the new one; it never faults.
    pub fn submit_generic_report(
        &self,
        request_id: u8,
        incarnation: u64,
        observed_at_us: u64,
        report: &[u8],
    ) -> GenericAdmission {
        let mut state = self.lock();
        let capture_active = state.capture_active;
        let Some(lane) = state.generic_lane(request_id) else {
            return GenericAdmission::UnknownDevice;
        };
        if lane.device.incarnation != incarnation {
            return GenericAdmission::StaleIncarnation;
        }
        lane.counters.reports_in = lane.counters.reports_in.saturating_add(1);
        let Some(report) = GenericReport::new(request_id, incarnation, observed_at_us, report)
        else {
            return GenericAdmission::Oversize;
        };
        if !lane.forwarding {
            lane.counters.dropped_not_forwarding =
                lane.counters.dropped_not_forwarding.saturating_add(1);
            return GenericAdmission::NotForwarding;
        }
        if !capture_active {
            lane.counters.dropped_inactive = lane.counters.dropped_inactive.saturating_add(1);
            return GenericAdmission::Inactive;
        }
        let mut admission = GenericAdmission::Admitted;
        if lane.reports.len() >= GENERIC_LANE_CAPACITY {
            lane.reports.pop_front();
            lane.counters.dropped_overflow = lane.counters.dropped_overflow.saturating_add(1);
            admission = GenericAdmission::DroppedOldest;
        }
        lane.reports.push_back(report);
        admission
    }

    pub fn set_generic_forwarding(&self, request_id: u8, forwarding: bool) -> bool {
        let mut state = self.lock();
        let Some(lane) = state.generic_lane(request_id) else {
            return false;
        };
        lane.forwarding = forwarding;
        if !forwarding {
            lane.reports.clear();
        }
        true
    }

    pub fn generic_forwarding(&self, request_id: u8) -> bool {
        self.lock()
            .generic_lane(request_id)
            .is_some_and(|lane| lane.forwarding)
    }

    /// Drain up to `per_device_budget` reports from every lane, oldest first.
    pub fn drain_generic(&self, per_device_budget: usize) -> Vec<GenericReport> {
        let mut state = self.lock();
        let mut drained = Vec::new();
        for lane in state.generic.iter_mut().flatten() {
            for _ in 0..per_device_budget {
                let Some(report) = lane.reports.pop_front() else {
                    break;
                };
                drained.push(report);
            }
        }
        drained
    }

    /// Queue an output report for the reader thread. The mailbox is four
    /// deep and latest-wins: a full mailbox evicts the oldest entry.
    pub fn push_generic_output(&self, output: GenericOutputReport) -> bool {
        let mut state = self.lock();
        let Some(lane) = state.generic_lane(output.request_id) else {
            return false;
        };
        if lane.removed || lane.dormant {
            return false;
        }
        lane.counters.outputs_in = lane.counters.outputs_in.saturating_add(1);
        if lane.outputs.len() >= GENERIC_OUTPUT_MAILBOX {
            lane.outputs.pop_front();
            lane.counters.outputs_coalesced = lane.counters.outputs_coalesced.saturating_add(1);
        }
        lane.outputs.push_back(output);
        true
    }

    pub fn take_generic_outputs(&self) -> Vec<GenericOutputReport> {
        let mut state = self.lock();
        let mut outputs = Vec::new();
        for lane in state.generic.iter_mut().flatten() {
            outputs.extend(lane.outputs.drain(..));
        }
        outputs
    }

    pub fn pending_generic_outputs(&self) -> usize {
        self.lock()
            .generic
            .iter()
            .flatten()
            .map(|lane| lane.outputs.len())
            .sum()
    }

    pub fn mark_generic_removed(&self, request_id: u8) -> bool {
        let mut state = self.lock();
        let Some(lane) = state.generic_lane(request_id) else {
            return false;
        };
        if lane.removed {
            return false;
        }
        lane.removed = true;
        // A dormant lane was never in the snapshot, so nothing was announced
        // and there is no removal to send.
        lane.removal_pending = !lane.dormant;
        lane.detach();
        true
    }

    /// Request ids whose device vanished since the last call.
    pub fn take_generic_removals(&self) -> Vec<u8> {
        let mut state = self.lock();
        let mut removals = Vec::new();
        for lane in state.generic.iter_mut().flatten() {
            if lane.removal_pending {
                lane.removal_pending = false;
                removals.push(lane.device.request_id);
            }
        }
        removals
    }

    pub fn generic_counters(&self, request_id: u8) -> Option<GenericCounters> {
        self.lock().generic_lane(request_id).map(|lane| lane.counters)
    }

    pub fn record_generic_forwarded(&self, request_id: u8) {
        if let Some(lane) = self.lock().generic_lane(request_id) {
            lane.counters.reports_forwarded = lane.counters.reports_forwarded.saturating_add(1);
        }
    }

    pub fn record_generic_send_refused(&self, request_id: u8) {
        if let Some(lane) = self.lock().generic_lane(request_id) {
            lane.counters.dropped_send_refused =
                lane.counters.dropped_send_refused.saturating_add(1);
        }
    }

    pub fn record_generic_write(&self, request_id: u8, ok: bool) {
        if let Some(lane) = self.lock().generic_lane(request_id) {
            if ok {
                lane.counters.outputs_written = lane.counters.outputs_written.saturating_add(1);
            } else {
                lane.counters.write_errors = lane.counters.write_errors.saturating_add(1);
            }
        }
    }

    pub fn pending_generic_reports(&self) -> usize {
        self.lock()
            .generic
            .iter()
            .flatten()
            .map(|lane| lane.reports.len())
            .sum()
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

    // ---- generic raw-HID lanes -------------------------------------------

    fn generic_device(request_id: u8) -> GenericHidDevice {
        GenericHidDevice {
            request_id,
            incarnation: 0,
            identity: crate::generic::GenericHidIdentity {
                vendor: 0x0738,
                product: 0x2221,
                revision: 0x0100,
                interface: 0,
            },
            hidraw_path: "/dev/hidraw8".to_owned(),
            sysfs_hid_id: "0003:00000738:00002221".to_owned(),
            name: "X-56 stick".to_owned(),
        }
    }

    fn generic_runtime() -> (HidRuntime, Vec<GenericHidDevice>) {
        let runtime = HidRuntime::new();
        runtime.open_endpoint();
        assert!(runtime.bind_session(1).is_some());
        let (devices, epoch) = runtime.replace_generic_inventory(&[generic_device(0)]);
        assert_eq!(epoch, 1);
        assert_eq!(devices.len(), 1);
        assert_ne!(devices[0].incarnation, 0);
        runtime.set_active(true);
        (runtime, devices)
    }

    #[test]
    fn generic_inventory_assigns_incarnations_and_ignores_bad_request_ids() {
        let runtime = HidRuntime::new();
        let mut bad = generic_device(7);
        bad.incarnation = 99;
        let (devices, epoch) = runtime.replace_generic_inventory(&[
            generic_device(0),
            generic_device(2),
            bad,
            generic_device(0),
        ]);
        assert_eq!(epoch, 1);
        assert_eq!(
            devices.iter().map(|d| d.request_id).collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_ne!(devices[0].incarnation, devices[1].incarnation);
        let (snapshot, snapshot_epoch) = runtime.generic_inventory_snapshot();
        assert_eq!(snapshot, devices);
        assert_eq!(snapshot_epoch, 1);
        let (empty, epoch) = runtime.replace_generic_inventory(&[]);
        assert!(empty.is_empty());
        assert_eq!(epoch, 2);
        assert_eq!(runtime.generic_inventory_epoch(), 2);
    }

    #[test]
    fn generic_reports_require_forwarding_capture_and_a_live_incarnation() {
        let (runtime, devices) = generic_runtime();
        let incarnation = devices[0].incarnation;
        let report = [1_u8; 11];
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 1, &report),
            GenericAdmission::NotForwarding
        );
        assert!(runtime.set_generic_forwarding(0, true));
        assert!(runtime.generic_forwarding(0));
        assert_eq!(
            runtime.submit_generic_report(0, incarnation + 1, 1, &report),
            GenericAdmission::StaleIncarnation
        );
        assert_eq!(
            runtime.submit_generic_report(1, incarnation, 1, &report),
            GenericAdmission::UnknownDevice
        );
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 1, &[0; 248]),
            GenericAdmission::Oversize
        );
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 2, &report),
            GenericAdmission::Admitted
        );
        runtime.set_active(false);
        assert_eq!(runtime.pending_generic_reports(), 0, "focus loss drops buffered reports");
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 3, &report),
            GenericAdmission::Inactive
        );
        let counters = runtime.generic_counters(0).unwrap();
        // Stale-incarnation and unknown-device rejections are not reports from
        // this device, so they do not count as ingress.
        assert_eq!(counters.reports_in, 4);
        assert_eq!(counters.dropped_not_forwarding, 1);
        assert_eq!(counters.dropped_inactive, 2);
        assert_eq!(counters.dropped_overflow, 0);
    }

    #[test]
    fn generic_lane_overflow_drops_the_oldest_without_a_fault() {
        let (runtime, devices) = generic_runtime();
        let incarnation = devices[0].incarnation;
        assert!(runtime.set_generic_forwarding(0, true));
        for index in 0..GENERIC_LANE_CAPACITY {
            assert_eq!(
                runtime.submit_generic_report(0, incarnation, index as u64, &[index as u8; 11]),
                GenericAdmission::Admitted
            );
        }
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 99, &[0xee; 11]),
            GenericAdmission::DroppedOldest
        );
        assert_eq!(runtime.pending_generic_reports(), GENERIC_LANE_CAPACITY);
        assert_eq!(runtime.generic_counters(0).unwrap().dropped_overflow, 1);
        assert_eq!(runtime.faulted_sources(), 0);
        let first = runtime.drain_generic(16);
        assert_eq!(first.len(), 16);
        assert_eq!(first[0].observed_at_us, 1, "oldest report was evicted");
        let second = runtime.drain_generic(16);
        assert_eq!(second.len(), 16);
        assert_eq!(second[15].as_slice(), &[0xee; 11]);
        assert!(runtime.drain_generic(16).is_empty());
        assert!(runtime.generic_forwarding(0), "overflow never detaches the lane");
    }

    #[test]
    fn unbinding_or_closing_clears_generic_forwarding_and_buffers() {
        let (runtime, devices) = generic_runtime();
        let incarnation = devices[0].incarnation;
        assert!(runtime.set_generic_forwarding(0, true));
        let _ = runtime.submit_generic_report(0, incarnation, 1, &[1; 11]);
        runtime.unbind_session(1);
        assert!(!runtime.generic_forwarding(0));
        assert_eq!(runtime.pending_generic_reports(), 0);
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 2, &[1; 11]),
            GenericAdmission::NotForwarding
        );
        let (snapshot, _) = runtime.generic_inventory_snapshot();
        assert_eq!(snapshot.len(), 1, "inventory survives unbind; the engine clears it");
        let token = runtime.open_endpoint();
        assert!(runtime.bind_session(2).is_some());
        assert!(runtime.set_generic_forwarding(0, true));
        runtime.close_endpoint(token);
        assert!(!runtime.generic_forwarding(0));
    }

    #[test]
    fn generic_output_mailbox_is_four_deep_and_latest_wins() {
        let (runtime, _) = generic_runtime();
        let output = |third: u8| GenericOutputReport {
            request_id: 0,
            interface: 0,
            third,
            bytes: vec![third, 1, 2],
        };
        for third in 0..GENERIC_OUTPUT_MAILBOX as u8 {
            assert!(runtime.push_generic_output(output(third)));
        }
        assert!(runtime.push_generic_output(output(9)));
        assert_eq!(runtime.pending_generic_outputs(), GENERIC_OUTPUT_MAILBOX);
        let counters = runtime.generic_counters(0).unwrap();
        assert_eq!(counters.outputs_in, 5);
        assert_eq!(counters.outputs_coalesced, 1);
        let outputs = runtime.take_generic_outputs();
        assert_eq!(
            outputs.iter().map(|o| o.third).collect::<Vec<_>>(),
            vec![1, 2, 3, 9]
        );
        assert!(runtime.take_generic_outputs().is_empty());
        assert!(!runtime.push_generic_output(GenericOutputReport {
            request_id: 2,
            interface: 0,
            third: 0,
            bytes: vec![]
        }));
        runtime.record_generic_write(0, true);
        runtime.record_generic_write(0, false);
        runtime.record_generic_forwarded(0);
        runtime.record_generic_send_refused(0);
        let counters = runtime.generic_counters(0).unwrap();
        assert_eq!(counters.outputs_written, 1);
        assert_eq!(counters.write_errors, 1);
        assert_eq!(counters.reports_forwarded, 1);
        assert_eq!(counters.dropped_send_refused, 1);
    }

    #[test]
    fn dormant_generic_lanes_stay_out_of_the_snapshot_until_marked_live() {
        let runtime = HidRuntime::new();
        let devices = [generic_device(0), generic_device(1)];
        let (stored, epoch) = runtime.replace_generic_inventory_dormant(&devices);
        assert_eq!(stored.len(), 2);
        assert_eq!(epoch, 1);
        let (snapshot, _) = runtime.generic_inventory_snapshot();
        assert!(snapshot.is_empty(), "nothing is announced before the node is open");
        assert!(runtime.take_generic_removals().is_empty());
        // An open failure removes the dormant lane without reporting an
        // unplug: nothing was ever announced for it.
        assert!(runtime.mark_generic_removed(1));
        assert!(runtime.take_generic_removals().is_empty());
        assert!(!runtime.mark_generic_live(1, stored[1].incarnation), "removed stays removed");
        // A stale incarnation or unknown lane cannot go live.
        assert!(!runtime.mark_generic_live(0, stored[0].incarnation + 99));
        assert!(!runtime.mark_generic_live(2, 1));
        assert!(runtime.mark_generic_live(0, stored[0].incarnation));
        assert!(!runtime.mark_generic_live(0, stored[0].incarnation), "already live");
        let (snapshot, epoch) = runtime.generic_inventory_snapshot();
        assert_eq!(epoch, 2, "going live moves the epoch so the pump snapshots");
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].request_id, 0);
        assert!(runtime.set_generic_forwarding(0, true));
        assert_eq!(
            runtime.submit_generic_report(0, stored[0].incarnation, 1, &[1; 11]),
            GenericAdmission::Inactive,
            "live lane admits reports (capture is simply not active yet)"
        );
        // A live lane that then vanishes reports the unplug normally.
        assert!(runtime.mark_generic_removed(0));
        assert_eq!(runtime.take_generic_removals(), vec![0]);
        assert!(!runtime.mark_generic_live(0, stored[0].incarnation), "removed lanes stay removed");
    }

    #[test]
    fn generic_removal_is_reported_once_and_detaches_the_lane() {
        let (runtime, devices) = generic_runtime();
        let incarnation = devices[0].incarnation;
        assert!(runtime.set_generic_forwarding(0, true));
        assert!(runtime.take_generic_removals().is_empty());
        assert!(runtime.mark_generic_removed(0));
        assert!(!runtime.mark_generic_removed(0));
        assert_eq!(runtime.take_generic_removals(), vec![0]);
        assert!(runtime.take_generic_removals().is_empty());
        assert!(!runtime.generic_forwarding(0));
        assert_eq!(
            runtime.submit_generic_report(0, incarnation, 1, &[1; 11]),
            GenericAdmission::NotForwarding
        );
        assert!(!runtime.push_generic_output(GenericOutputReport {
            request_id: 0,
            interface: 0,
            third: 0,
            bytes: vec![1]
        }));
        let (snapshot, _) = runtime.generic_inventory_snapshot();
        assert!(snapshot.is_empty(), "removed devices leave the snapshot");
        assert!(!runtime.mark_generic_removed(1));
    }
}
