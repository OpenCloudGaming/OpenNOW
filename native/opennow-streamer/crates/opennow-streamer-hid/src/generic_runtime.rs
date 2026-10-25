use std::collections::VecDeque;
use std::sync::Mutex;

use crate::generic::{
    GenericAdmission, GenericAdmissionStatus, GenericDevice, GenericError, GenericOutput,
    GenericReport, GenericSession, MAX_GENERIC_SOURCES, validate_inventory, validate_operation,
    validate_report,
};

const MAX_REPORTS_PER_SOURCE: usize = 64;
const MAX_REPORT_BYTES_PER_SOURCE: usize = 16 * 1024;
const MAX_OUTPUTS: usize = 32;
const MAX_OUTPUT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericIngress {
    Report(GenericReport),
    Release { slot: u8, incarnation: u64 },
    Fault { slot: u8, incarnation: u64 },
}

#[derive(Debug, Default)]
struct Lane {
    reports: VecDeque<GenericReport>,
    bytes: usize,
    release: Option<u64>,
    fault: Option<u64>,
    faulted: Option<u64>,
}

impl Lane {
    fn clear_reports(&mut self) {
        self.reports.clear();
        self.bytes = 0;
    }
}

#[derive(Debug, Default)]
struct State {
    inventory: Vec<GenericDevice>,
    inventory_epoch: u64,
    binding: Option<u64>,
    capture: bool,
    release_epoch: u64,
    lanes: [Lane; MAX_GENERIC_SOURCES],
    next_source: usize,
    outputs: VecDeque<GenericOutput>,
    output_bytes: usize,
    admissions: [Option<GenericAdmissionStatus>; MAX_GENERIC_SOURCES],
}

impl State {
    fn clear_queues(&mut self) {
        self.lanes = Default::default();
        self.next_source = 0;
        self.outputs.clear();
        self.output_bytes = 0;
        self.admissions = Default::default();
    }

    fn source(&self, slot: u8, incarnation: u64, interface: u8) -> Result<(), GenericError> {
        let device = self
            .inventory
            .iter()
            .find(|device| device.slot == slot && device.incarnation == incarnation)
            .ok_or(GenericError::StaleSource)?;
        if !device
            .interfaces
            .iter()
            .any(|entry| entry.number == interface)
        {
            return Err(GenericError::UnknownInterface);
        }
        if self.lanes[usize::from(slot)].faulted == Some(incarnation) {
            return Err(GenericError::Faulted);
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct GenericRuntime {
    state: Mutex<State>,
}

impl GenericRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn replace_inventory(&self, inventory: Vec<GenericDevice>) -> Result<(), GenericError> {
        validate_inventory(&inventory)?;
        let mut state = self.lock();
        if state.inventory == inventory {
            return Ok(());
        }
        let previous = std::mem::replace(&mut state.inventory, inventory);
        for device in previous {
            if state.inventory.contains(&device) {
                continue;
            }
            state.admissions[usize::from(device.slot)] = None;
            let lane = &mut state.lanes[usize::from(device.slot)];
            lane.clear_reports();
            lane.release.get_or_insert(device.incarnation);
            if !state
                .inventory
                .iter()
                .any(|entry| entry.slot == device.slot && entry.incarnation == device.incarnation)
            {
                state.lanes[usize::from(device.slot)].faulted = None;
            }
        }
        state.outputs.clear();
        state.output_bytes = 0;
        state.inventory_epoch = state.inventory_epoch.wrapping_add(1);
        Ok(())
    }

    pub fn inventory_snapshot(&self) -> (Vec<GenericDevice>, u64) {
        let state = self.lock();
        (state.inventory.clone(), state.inventory_epoch)
    }

    pub fn publish_admissions(&self, session: &GenericSession) {
        let mut state = self.lock();
        if state.binding != Some(session.generation()) {
            return;
        }
        for index in 0..state.inventory.len() {
            let device = &state.inventory[index];
            let status = GenericAdmissionStatus {
                slot: device.slot,
                incarnation: device.incarnation,
                admission: session.admission(device),
            };
            state.admissions[usize::from(status.slot)] = Some(status);
        }
    }

    pub fn admissions_snapshot(&self) -> Vec<GenericAdmissionStatus> {
        let state = self.lock();
        state
            .inventory
            .iter()
            .map(|device| {
                state.admissions[usize::from(device.slot)].unwrap_or(GenericAdmissionStatus {
                    slot: device.slot,
                    incarnation: device.incarnation,
                    admission: GenericAdmission::Pending,
                })
            })
            .collect()
    }

    pub fn submit_report(&self, report: GenericReport) -> Result<(), GenericError> {
        validate_report(&report)?;
        let mut state = self.lock();
        state.source(report.slot, report.incarnation, report.interface)?;
        if state.binding.is_none() {
            return Err(GenericError::Unbound);
        }
        if !state.capture {
            return Err(GenericError::Inactive);
        }
        let lane = &mut state.lanes[usize::from(report.slot)];
        if lane.reports.len() >= MAX_REPORTS_PER_SOURCE
            || lane.bytes + report.bytes.len() > MAX_REPORT_BYTES_PER_SOURCE
        {
            lane.clear_reports();
            lane.fault = Some(report.incarnation);
            lane.faulted = Some(report.incarnation);
            return Err(GenericError::Overflow);
        }
        lane.bytes += report.bytes.len();
        lane.reports.push_back(report);
        Ok(())
    }

    pub fn drain(&self, limit: usize) -> Vec<GenericIngress> {
        let mut state = self.lock();
        let mut result = Vec::with_capacity(limit.min(MAX_GENERIC_SOURCES * 66));
        for (slot, lane) in state.lanes.iter_mut().enumerate() {
            if result.len() == limit {
                break;
            }
            if let Some(incarnation) = lane.fault.take() {
                result.push(GenericIngress::Fault {
                    slot: slot as u8,
                    incarnation,
                });
            }
            if result.len() == limit {
                break;
            }
            if let Some(incarnation) = lane.release.take() {
                result.push(GenericIngress::Release {
                    slot: slot as u8,
                    incarnation,
                });
            }
        }
        while result.len() < limit {
            let mut progressed = false;
            for _ in 0..MAX_GENERIC_SOURCES {
                if result.len() == limit {
                    break;
                }
                let slot = state.next_source;
                state.next_source = (slot + 1) % MAX_GENERIC_SOURCES;
                let lane = &mut state.lanes[slot];
                if let Some(report) = lane.reports.pop_front() {
                    lane.bytes -= report.bytes.len();
                    result.push(GenericIngress::Report(report));
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        result
    }

    pub fn set_capture(&self, active: bool) {
        let mut state = self.lock();
        if state.capture == active {
            return;
        }
        state.capture = active;
        if active {
            return;
        }
        state.release_epoch = state.release_epoch.wrapping_add(1);
        for lane in &mut state.lanes {
            lane.clear_reports();
        }
        state.outputs.clear();
        state.output_bytes = 0;
    }

    pub fn capture_state(&self) -> (bool, u64) {
        let state = self.lock();
        (state.capture, state.release_epoch)
    }

    pub fn bind_session(&self, generation: u64) {
        let mut state = self.lock();
        state.clear_queues();
        state.binding = Some(generation);
    }

    pub fn unbind_session(&self, generation: u64) {
        let mut state = self.lock();
        if state.binding != Some(generation) {
            return;
        }
        state.binding = None;
        state.clear_queues();
    }

    pub fn push_output(&self, output: GenericOutput) -> Result<(), GenericError> {
        validate_operation(output.operation, &output.bytes, true)?;
        let mut state = self.lock();
        state.source(output.slot, output.incarnation, output.interface)?;
        if state.binding.is_none() {
            return Err(GenericError::Unbound);
        }
        if !state.capture {
            return Err(GenericError::Inactive);
        }
        if state.outputs.len() >= MAX_OUTPUTS
            || state.output_bytes + output.bytes.len() > MAX_OUTPUT_BYTES
        {
            return Err(GenericError::Overflow);
        }
        state.output_bytes += output.bytes.len();
        state.outputs.push_back(output);
        Ok(())
    }

    pub fn take_output(&self) -> Option<GenericOutput> {
        let mut state = self.lock();
        let output = state.outputs.pop_front()?;
        state.output_bytes -= output.bytes.len();
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generic::tests::{device, report};
    use crate::generic::{GenericInterface, MAX_DESCRIPTOR_BYTES, MAX_GENERIC_INTERFACES};

    fn runtime() -> GenericRuntime {
        let runtime = GenericRuntime::new();
        runtime.replace_inventory(vec![device(0, 1)]).unwrap();
        runtime.bind_session(1);
        runtime.set_capture(true);
        runtime
    }

    #[test]
    fn admission_tracks_verified_responses_and_resets_on_detach_or_replacement() {
        let runtime = runtime();
        let mut source = device(0, 1);
        source.interfaces[0].descriptor.clear();
        runtime.replace_inventory(vec![source.clone()]).unwrap();
        let mut session = GenericSession::new(1);
        let actions = session.reconcile(&[source.clone()], true);
        runtime.publish_admissions(&session);
        assert_eq!(
            runtime.admissions_snapshot()[0].admission,
            GenericAdmission::Pending
        );
        session.handle_response(10, 1, 0, 22);
        runtime.publish_admissions(&session);
        assert_eq!(
            runtime.admissions_snapshot()[0].admission,
            GenericAdmission::Pending
        );
        session.sent(&actions[0]);
        session.handle_response(10, 1, 0, 22);
        runtime.publish_admissions(&session);
        assert_eq!(
            runtime.admissions_snapshot()[0].admission,
            GenericAdmission::Admitted
        );
        let detach = session.set_capture(false);
        runtime.publish_admissions(&session);
        assert_eq!(
            runtime.admissions_snapshot()[0].admission,
            GenericAdmission::Pending
        );
        session.sent(&detach[0]);
        let actions = session.set_capture(true);
        session.sent(&actions[0]);
        session.handle_response(11, 2, 0, 22);
        runtime.publish_admissions(&session);
        assert_eq!(
            runtime.admissions_snapshot()[0].admission,
            GenericAdmission::Declined
        );
        source.incarnation = 2;
        runtime.replace_inventory(vec![source]).unwrap();
        runtime.publish_admissions(&session);
        assert_eq!(
            runtime.admissions_snapshot(),
            vec![GenericAdmissionStatus {
                slot: 0,
                incarnation: 2,
                admission: GenericAdmission::Pending,
            }]
        );
        runtime.replace_inventory(vec![]).unwrap();
        assert!(runtime.admissions_snapshot().is_empty());
    }

    #[test]
    fn admission_reset_is_session_scoped_and_bounded() {
        let runtime = runtime();
        let devices = (0..8)
            .map(|slot| device(slot, u64::from(slot) + 1))
            .collect::<Vec<_>>();
        runtime.replace_inventory(devices.clone()).unwrap();
        let mut session = GenericSession::new(1);
        for action in session.reconcile(&devices, true) {
            session.sent(&action);
        }
        for slot in 0..8 {
            session.handle_response(10 + slot, 1, 0, 30 + slot);
        }
        runtime.publish_admissions(&session);
        let statuses = runtime.admissions_snapshot();
        assert_eq!(statuses.len(), 8);
        assert!(
            statuses
                .iter()
                .all(|row| row.admission == GenericAdmission::Admitted)
        );
        runtime.unbind_session(9);
        assert_eq!(runtime.admissions_snapshot(), statuses);
        runtime.unbind_session(1);
        runtime.publish_admissions(&session);
        assert!(
            runtime
                .admissions_snapshot()
                .iter()
                .all(|row| row.admission == GenericAdmission::Pending)
        );
        runtime.bind_session(2);
        runtime.publish_admissions(&session);
        assert!(
            runtime
                .admissions_snapshot()
                .iter()
                .all(|row| row.admission == GenericAdmission::Pending)
        );
    }

    fn output() -> GenericOutput {
        GenericOutput {
            slot: 0,
            incarnation: 1,
            interface: 2,
            operation: 0,
            bytes: vec![1],
        }
    }

    #[test]
    fn invalid_inventory_does_not_mutate_existing_inventory() {
        let runtime = runtime();
        let original = runtime.inventory_snapshot();
        let mut invalid = device(0, 1);
        invalid.slot = 8;
        assert_eq!(
            runtime.replace_inventory(vec![invalid]),
            Err(GenericError::InvalidSlot)
        );
        let mut invalid = device(0, 0);
        assert_eq!(
            runtime.replace_inventory(vec![invalid.clone()]),
            Err(GenericError::InvalidIncarnation)
        );
        invalid.incarnation = 1;
        invalid.product = 1;
        assert_eq!(
            runtime.replace_inventory(vec![invalid]),
            Err(GenericError::NotAllowlisted)
        );
        assert_eq!(
            runtime.replace_inventory(vec![device(0, 1), device(0, 2)]),
            Err(GenericError::DuplicateSlot)
        );
        let mut invalid = device(0, 1);
        invalid.interfaces.push(invalid.interfaces[0].clone());
        assert_eq!(
            runtime.replace_inventory(vec![invalid]),
            Err(GenericError::DuplicateInterface)
        );
        let mut invalid = device(0, 1);
        invalid.interfaces[0].descriptor = vec![0; MAX_DESCRIPTOR_BYTES + 1];
        assert_eq!(
            runtime.replace_inventory(vec![invalid]),
            Err(GenericError::DescriptorTooLarge)
        );
        let mut invalid = device(0, 1);
        invalid.interfaces = (0..=MAX_GENERIC_INTERFACES)
            .map(|number| GenericInterface {
                number: number as u8,
                descriptor: vec![],
            })
            .collect();
        assert_eq!(
            runtime.replace_inventory(vec![invalid]),
            Err(GenericError::TooManyInterfaces)
        );
        assert_eq!(runtime.inventory_snapshot(), original);
    }

    #[test]
    fn unknown_interface_255_is_rejected_at_inventory_boundary() {
        let runtime = runtime();
        let mut source = device(0, 1);
        source.interfaces[0].number = 255;
        assert_eq!(
            runtime.replace_inventory(vec![source]),
            Err(GenericError::UnknownInterface)
        );
        assert_eq!(runtime.inventory_snapshot().0, vec![device(0, 1)]);
    }

    #[test]
    fn only_error_control_replies_can_have_empty_payloads() {
        let runtime = runtime();
        for operation in 0..=5 {
            for error in [false, true] {
                let mut input = report(0, 1);
                input.operation = operation;
                input.error = error;
                input.bytes.clear();
                let admitted = error && (1..=4).contains(&operation);
                assert_eq!(runtime.submit_report(input).is_ok(), admitted);
            }
        }
        assert_eq!(runtime.drain(32).len(), 4);
    }

    #[test]
    fn inventory_accepts_all_128_interfaces_and_unreadable_descriptors() {
        let runtime = GenericRuntime::new();
        let devices = (0..8)
            .map(|slot| {
                let mut entry = device(slot, u64::from(slot) + 1);
                entry.interfaces = (0..16)
                    .map(|number| GenericInterface {
                        number,
                        descriptor: vec![],
                    })
                    .collect();
                entry
            })
            .collect::<Vec<_>>();
        runtime.replace_inventory(devices.clone()).unwrap();
        assert_eq!(runtime.inventory_snapshot().0, devices);
    }

    #[test]
    fn capture_off_on_discards_backlog_but_preserves_release_epoch() {
        let runtime = runtime();
        runtime.submit_report(report(0, 1)).unwrap();
        runtime.push_output(output()).unwrap();
        runtime.set_capture(false);
        assert_eq!(
            runtime.submit_report(report(0, 1)),
            Err(GenericError::Inactive)
        );
        assert_eq!(runtime.push_output(output()), Err(GenericError::Inactive));
        runtime.set_capture(true);
        assert_eq!(runtime.capture_state(), (true, 1));
        assert!(runtime.take_output().is_none());
        assert!(runtime.drain(0).is_empty());
        runtime.submit_report(report(0, 1)).unwrap();
        assert_eq!(runtime.drain(1), vec![GenericIngress::Report(report(0, 1))]);
        runtime.set_capture(false);
        runtime.set_capture(false);
        assert_eq!(runtime.capture_state(), (false, 2));
    }

    #[test]
    fn collapsed_capture_transition_generates_one_descriptor_release() {
        use crate::generic::{GenericOutbound, GenericSession};

        let runtime = runtime();
        let mut session = GenericSession::new(1);
        let attach = session.reconcile(&runtime.inventory_snapshot().0, true);
        session.sent(&attach[0]);
        session.handle_response(10, 1, 0, 22);
        runtime.submit_report(report(0, 1)).unwrap();
        let GenericIngress::Report(input) = runtime.drain(1).pop().unwrap() else {
            panic!("expected input");
        };
        let outgoing = session.report(input).unwrap();
        session.sent(&outgoing);
        runtime.set_capture(false);
        runtime.set_capture(true);
        let (active, release_epoch) = runtime.capture_state();
        assert_eq!((active, release_epoch), (true, 1));
        let mut outbound = session.set_capture(false);
        outbound.extend(session.reconcile(&runtime.inventory_snapshot().0, active));
        assert_eq!(outbound.len(), 1);
        assert!(matches!(
            outbound[0],
            GenericOutbound::Report { release: true, .. }
        ));
        session.sent(&outbound[0]);
        assert!(runtime.drain(8).is_empty());
        assert!(
            session
                .reconcile(&runtime.inventory_snapshot().0, active)
                .is_empty()
        );
        runtime.submit_report(report(0, 1)).unwrap();
        let GenericIngress::Report(input) = runtime.drain(1).pop().unwrap() else {
            panic!("expected input");
        };
        assert!(matches!(
            session.report(input),
            Some(GenericOutbound::Report { release: false, .. })
        ));
    }

    #[test]
    fn overflow_drops_backlog_and_fault_is_sticky_across_drain_and_capture() {
        let runtime = runtime();
        for _ in 0..64 {
            runtime.submit_report(report(0, 1)).unwrap();
        }
        assert_eq!(
            runtime.submit_report(report(0, 1)),
            Err(GenericError::Overflow)
        );
        assert_eq!(
            runtime.submit_report(report(0, 1)),
            Err(GenericError::Faulted)
        );
        runtime.set_capture(false);
        runtime.set_capture(true);
        assert!(runtime.drain(0).is_empty());
        assert_eq!(
            runtime.drain(1),
            vec![GenericIngress::Fault {
                slot: 0,
                incarnation: 1
            }]
        );
        assert!(runtime.drain(64).is_empty());
        assert_eq!(runtime.push_output(output()), Err(GenericError::Faulted));
        assert_eq!(
            runtime.submit_report(report(0, 1)),
            Err(GenericError::Faulted)
        );
        runtime.replace_inventory(vec![device(0, 2)]).unwrap();
        runtime.submit_report(report(0, 2)).unwrap();
    }

    #[test]
    fn sources_have_independent_64_report_and_16kib_budgets() {
        let runtime = runtime();
        runtime
            .replace_inventory(vec![device(0, 1), device(1, 2)])
            .unwrap();
        let mut first = report(0, 1);
        first.bytes = vec![1; 256];
        for _ in 0..64 {
            runtime.submit_report(first.clone()).unwrap();
        }
        runtime.submit_report(report(1, 2)).unwrap();
        assert_eq!(runtime.submit_report(first), Err(GenericError::Overflow));
        assert_eq!(
            runtime.drain(2),
            vec![
                GenericIngress::Fault {
                    slot: 0,
                    incarnation: 1
                },
                GenericIngress::Report(report(1, 2))
            ]
        );
    }

    #[test]
    fn small_drain_budgets_do_not_starve_later_sources() {
        let runtime = runtime();
        runtime
            .replace_inventory(vec![device(0, 1), device(1, 2)])
            .unwrap();
        for _ in 0..4 {
            runtime.submit_report(report(0, 1)).unwrap();
        }
        runtime.submit_report(report(1, 2)).unwrap();
        assert_eq!(runtime.drain(1), vec![GenericIngress::Report(report(0, 1))]);
        assert_eq!(runtime.drain(1), vec![GenericIngress::Report(report(1, 2))]);
    }

    #[test]
    fn removal_and_restart_reject_stale_reports_and_clear_session_queues() {
        let runtime = runtime();
        runtime.submit_report(report(0, 1)).unwrap();
        runtime.push_output(output()).unwrap();
        runtime.replace_inventory(vec![device(0, 2)]).unwrap();
        assert_eq!(
            runtime.submit_report(report(0, 1)),
            Err(GenericError::StaleSource)
        );
        assert_eq!(
            runtime.drain(1),
            vec![GenericIngress::Release {
                slot: 0,
                incarnation: 1
            }]
        );
        assert!(runtime.take_output().is_none());
        runtime.submit_report(report(0, 2)).unwrap();
        runtime.bind_session(2);
        assert!(runtime.drain(8).is_empty());
        assert_eq!(runtime.inventory_snapshot().0, vec![device(0, 2)]);
        runtime.unbind_session(1);
        runtime.submit_report(report(0, 2)).unwrap();
        runtime.unbind_session(2);
        assert!(runtime.drain(8).is_empty());
        assert_eq!(
            runtime.submit_report(report(0, 2)),
            Err(GenericError::Unbound)
        );
    }

    #[test]
    fn output_queue_is_bounded_and_capture_or_session_closure_clears_it() {
        let runtime = runtime();
        let mut outgoing = output();
        outgoing.bytes = vec![1; 256];
        for _ in 0..32 {
            runtime.push_output(outgoing.clone()).unwrap();
        }
        assert_eq!(
            runtime.push_output(outgoing.clone()),
            Err(GenericError::Overflow)
        );
        assert_eq!(runtime.take_output(), Some(outgoing.clone()));
        runtime.push_output(outgoing).unwrap();
        runtime.set_capture(false);
        assert!(runtime.take_output().is_none());
        runtime.set_capture(true);
        runtime.push_output(output()).unwrap();
        runtime.unbind_session(1);
        assert!(runtime.take_output().is_none());
        assert_eq!(runtime.push_output(output()), Err(GenericError::Unbound));
    }

    #[test]
    fn hid_runtime_propagates_session_capture_and_endpoint_closure() {
        let runtime = crate::HidRuntime::new();
        runtime
            .generic
            .replace_inventory(vec![device(0, 1)])
            .unwrap();
        let endpoint = runtime.open_endpoint();
        runtime.bind_session(1).unwrap();
        runtime.set_capture_active(true);
        runtime.generic.submit_report(report(0, 1)).unwrap();
        runtime.set_capture_active(false);
        runtime.set_capture_active(true);
        assert_eq!(runtime.generic.capture_state(), (true, 1));
        assert!(runtime.generic.drain(1).is_empty());
        runtime.generic.submit_report(report(0, 1)).unwrap();
        runtime.close_endpoint(endpoint);
        assert!(runtime.generic.drain(1).is_empty());
        assert_eq!(
            runtime.generic.submit_report(report(0, 1)),
            Err(GenericError::Unbound)
        );
        assert_eq!(runtime.generic.inventory_snapshot().0, vec![device(0, 1)]);
    }

    #[test]
    fn replaced_endpoint_clears_generic_queues_and_stale_endpoint_cannot_close_new_session() {
        let runtime = crate::HidRuntime::new();
        runtime
            .generic
            .replace_inventory(vec![device(0, 1)])
            .unwrap();
        let first = runtime.open_endpoint();
        runtime.bind_session(1).unwrap();
        runtime.set_capture_active(true);
        runtime.generic.submit_report(report(0, 1)).unwrap();
        let second = runtime.open_endpoint();
        assert!(runtime.generic.drain(8).is_empty());
        assert_eq!(
            runtime.generic.submit_report(report(0, 1)),
            Err(GenericError::Unbound)
        );
        runtime.bind_session(2).unwrap();
        runtime.close_endpoint(first);
        runtime.generic.submit_report(report(0, 1)).unwrap();
        runtime.close_endpoint(second);
        assert!(runtime.generic.drain(8).is_empty());
        assert_eq!(
            runtime.generic.submit_report(report(0, 1)),
            Err(GenericError::Unbound)
        );
    }
}
