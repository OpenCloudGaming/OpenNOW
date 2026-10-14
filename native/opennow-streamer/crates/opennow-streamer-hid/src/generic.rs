//! Generic raw-HID forwarding policy (allowlisted HOTAS/wheel devices).
//!
//! This module is pure state and policy: no I/O, no threads. It models the
//! per-session admission handshake NVIDIA's native client performs for generic
//! HID devices (RI `0x12` change event, RI `0x1a` response, RI `0x11` reports)
//! and the bounded per-device ingress/egress buffers that connect the hidraw
//! reader thread to the NVST bundle thread.
//!
//! Request ids `0..=2` are chosen by the client and carried by the `0x12`
//! attach only. The `0x1a` response hands back a *server-assigned* device id
//! (an arbitrary `u8`; 25 was observed live) that keys everything afterwards:
//! every `0x11` input report, inbound `0x11` output reports and the `0x12`
//! detach. The synthesized DualShock 4 lanes own ids `6..=9` on the same wire,
//! so a server assignment that collides with an active DS4 lane is refused.

use std::time::Duration;

/// Geronimo keeps `nvbutil::Array<GSHidDevice,3>`: at most three forwarded
/// devices per session.
pub const MAX_GENERIC_DEVICES: usize = 3;
/// JS SDK caps one HID report packet at 263 bytes; 16 bytes of framing
/// (frame header, optional PR prefix, marker, RI type, id/interface/zero)
/// leave 247 bytes for the raw report.
pub const MAX_GENERIC_REPORT_BYTES: usize = 247;
pub const GENERIC_REQUEST_ID_BASE: u8 = 0;
/// `flags` for the `0x12` change event. Only bit `0x2` is established (the JS
/// SDK writes the four trailing `u16` identity words when it is set); the
/// other low bits are EXPERIMENTAL and have not been observed on the wire from
/// the official client. Change here if a captured attach shows otherwise.
pub const GENERIC_DEVICE_CHANGE_FLAGS: u32 = 0x0000_000F;
pub const GENERIC_LANE_CAPACITY: usize = 32;
pub const GENERIC_DRAIN_PER_DEVICE: usize = 16;
pub const GENERIC_OUTPUT_MAILBOX: usize = 4;
pub const GENERIC_ATTACH_TIMEOUT: Duration = Duration::from_secs(10);
/// `status` byte (offset `+5`) of the `0x1a` response that admits the device.
/// The official client (`HIDDevicesController::handleGenericResponse`) accepts
/// iff the byte equals this exactly; any other value, including 0, disables
/// the device.
pub const GENERIC_RESPONSE_STATUS_ACCEPTED: u8 = 1;
/// Server-assigned device id the official client treats as "disable" even
/// when the status byte says accepted.
pub const GENERIC_DEVICE_ID_DISABLED: u8 = 0xff;
/// Server-assigned device id the official client stores without starting a
/// reading loop: the device is admitted but stays silent.
pub const GENERIC_DEVICE_ID_SILENT: u8 = 0;
/// Device ids representable in the `u16` collision masks exchanged with the
/// DS4 lane (bit `n` = id `n`). Only ids below this can collide with the
/// synthesized DualShock 4 domain (`6..=9`); higher server assignments never
/// conflict with it.
pub const DEVICE_ID_MASK_BITS: u8 = 16;

pub fn generic_request_id(index: u8) -> Option<u8> {
    (usize::from(index) < MAX_GENERIC_DEVICES).then_some(index + GENERIC_REQUEST_ID_BASE)
}

pub fn generic_request_index(request_id: u8) -> Option<usize> {
    let index = usize::from(request_id.checked_sub(GENERIC_REQUEST_ID_BASE)?);
    (index < MAX_GENERIC_DEVICES).then_some(index)
}

/// Bit for `device_id` in a `u16` collision mask; `None` for ids the mask
/// cannot represent, which never collide with the DS4 domain.
pub fn device_id_mask_bit(device_id: u8) -> Option<u16> {
    (device_id < DEVICE_ID_MASK_BITS).then(|| 1_u16 << device_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GenericHidIdentity {
    pub vendor: u16,
    pub product: u16,
    /// USB `bcdDevice`, forwarded as the third identity word.
    pub revision: u16,
    /// USB interface number of the forwarded HID interface.
    pub interface: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericHidDevice {
    pub request_id: u8,
    /// Zero until the runtime assigns an incarnation on inventory replacement.
    pub incarnation: u64,
    pub identity: GenericHidIdentity,
    pub hidraw_path: String,
    /// `HID_ID=` line of the hidraw node's sysfs `uevent`, re-verified by the
    /// reader before the node is opened.
    pub sysfs_hid_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericAttachState {
    /// Known to the session but the attach has not been sent yet.
    Idle,
    Pending {
        sent_at_us: u64,
    },
    /// Admitted; `device_id` is the server-assigned id that keys reports.
    Accepted {
        device_id: u8,
    },
    /// Admitted with server id [`GENERIC_DEVICE_ID_SILENT`]: the official
    /// client stores the id and starts no reading loop, so no reports flow.
    AcceptedSilent,
    /// `status != 1`: the seat refused the device.
    Denied {
        status: u8,
    },
    /// `status == 1` but the server id is [`GENERIC_DEVICE_ID_DISABLED`].
    Disabled,
    /// The server handed back a device id that is already owned elsewhere
    /// (another generic device or a synthesized DS4 id); forwarding refused.
    Conflicted {
        device_id: u8,
    },
    TimedOut,
    Removed,
}

impl GenericAttachState {
    /// Id a `0x12` detach (control 3) must carry when one is owed: the
    /// server-assigned device id once the attach was answered, otherwise the
    /// request id the attach was sent with. `None` when the server does not
    /// hold the device (never sent, refused, or already withdrawn).
    ///
    /// A conflicted assignment owes nothing: a detach by the shared id would
    /// withdraw the lane that legitimately owns it, so the server-side
    /// phantom is the lesser harm.
    pub fn wire_id(self, request_id: u8) -> Option<u8> {
        match self {
            Self::Pending { .. } | Self::TimedOut => Some(request_id),
            Self::Accepted { device_id } => Some(device_id),
            Self::AcceptedSilent => Some(GENERIC_DEVICE_ID_SILENT),
            Self::Idle
            | Self::Denied { .. }
            | Self::Disabled
            | Self::Conflicted { .. }
            | Self::Removed => None,
        }
    }

    /// Whether the server currently knows about this device (attach sent and
    /// not yet withdrawn), i.e. whether a removal must be sent on teardown.
    pub fn announced(self) -> bool {
        self.wire_id(GENERIC_REQUEST_ID_BASE).is_some()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Pending { .. } => "pending",
            Self::Accepted { .. } => "accepted",
            Self::AcceptedSilent => "accepted-silent",
            Self::Denied { .. } => "denied",
            Self::Disabled => "disabled",
            Self::Conflicted { .. } => "conflicted",
            Self::TimedOut => "timeout",
            Self::Removed => "removed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericOutbound {
    Attach {
        request_id: u8,
        identity: GenericHidIdentity,
    },
    Removal {
        request_id: u8,
        /// Id carried by the `0x12` (control 3): the server-assigned device
        /// id once the attach was accepted, otherwise the request id.
        wire_id: u8,
        identity: GenericHidIdentity,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseOutcome {
    Accepted {
        device_id: u8,
    },
    /// Accepted with server id 0: stored, but no reports will be forwarded.
    AcceptedSilent,
    Denied {
        status: u8,
    },
    /// Accepted with server id `0xff`: the official client disables the device.
    Disabled,
    UnknownRequest,
    DeviceIdConflict {
        device_id: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenericSlot {
    device: GenericHidDevice,
    state: GenericAttachState,
}

impl GenericSlot {
    fn removal(&self) -> Option<GenericOutbound> {
        let request_id = self.device.request_id;
        Some(GenericOutbound::Removal {
            request_id,
            wire_id: self.state.wire_id(request_id)?,
            identity: self.device.identity,
        })
    }
}

/// Per-session admission state for up to three generic devices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenericHidSession {
    slots: [Option<GenericSlot>; MAX_GENERIC_DEVICES],
}

impl GenericHidSession {
    fn slot(&self, request_id: u8) -> Option<&GenericSlot> {
        self.slots.get(generic_request_index(request_id)?)?.as_ref()
    }

    fn slot_mut(&mut self, request_id: u8) -> Option<&mut GenericSlot> {
        self.slots
            .get_mut(generic_request_index(request_id)?)?
            .as_mut()
    }

    pub fn identity(&self, request_id: u8) -> Option<GenericHidIdentity> {
        self.slot(request_id).map(|slot| slot.device.identity)
    }

    pub fn device(&self, request_id: u8) -> Option<&GenericHidDevice> {
        self.slot(request_id).map(|slot| &slot.device)
    }

    pub fn state(&self, request_id: u8) -> Option<GenericAttachState> {
        self.slot(request_id).map(|slot| slot.state)
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// Replace the device inventory. Devices that vanished (or whose
    /// incarnation changed) produce a `Removal` when they had been announced;
    /// new devices are entered as `Idle` and produce an `Attach` the caller
    /// must send and then confirm with [`mark_attach_sent`].
    ///
    /// [`mark_attach_sent`]: Self::mark_attach_sent
    pub fn replace(&mut self, devices: &[GenericHidDevice]) -> Vec<GenericOutbound> {
        let mut outbound = Vec::with_capacity(MAX_GENERIC_DEVICES * 2);
        for index in 0..MAX_GENERIC_DEVICES {
            let request_id = GENERIC_REQUEST_ID_BASE + index as u8;
            let wanted = devices
                .iter()
                .find(|device| device.request_id == request_id && device.incarnation != 0);
            match (self.slots[index].as_ref(), wanted) {
                (Some(current), Some(device))
                    if current.device.incarnation == device.incarnation =>
                {
                    continue;
                }
                (Some(current), _) => {
                    outbound.extend(current.removal());
                    self.slots[index] = None;
                }
                (None, _) => {}
            }
            if let Some(device) = wanted {
                self.slots[index] = Some(GenericSlot {
                    device: device.clone(),
                    state: GenericAttachState::Idle,
                });
                outbound.push(GenericOutbound::Attach {
                    request_id,
                    identity: device.identity,
                });
            }
        }
        outbound
    }

    /// Devices whose attach has not been sent yet.
    pub fn pending_attaches(&self) -> Vec<(u8, GenericHidIdentity)> {
        self.slots
            .iter()
            .flatten()
            .filter(|slot| slot.state == GenericAttachState::Idle)
            .map(|slot| (slot.device.request_id, slot.device.identity))
            .collect()
    }

    pub fn mark_attach_sent(&mut self, request_id: u8, now_us: u64) -> bool {
        let Some(slot) = self.slot_mut(request_id) else {
            return false;
        };
        if slot.state != GenericAttachState::Idle {
            return false;
        }
        slot.state = GenericAttachState::Pending { sent_at_us: now_us };
        true
    }

    /// Apply a `0x1a` generic device response (`requestId`, `status`, echoed
    /// `flags`, server-assigned `device_id`). Accepted iff `status` is exactly
    /// [`GENERIC_RESPONSE_STATUS_ACCEPTED`]; an accepted id of
    /// [`GENERIC_DEVICE_ID_DISABLED`] disables and [`GENERIC_DEVICE_ID_SILENT`]
    /// admits without forwarding, as in the official client.
    /// `externally_owned_ids` marks device ids (bit `n` = id `n`, see
    /// [`device_id_mask_bit`]) owned by other HID lanes, such as active
    /// synthesized DualShock 4 ids, so a colliding assignment is refused
    /// instead of hijacking that lane.
    pub fn on_response(
        &mut self,
        request_id: u8,
        status: u8,
        _flags: u32,
        device_id: u8,
        externally_owned_ids: u16,
    ) -> ResponseOutcome {
        let Some(index) = generic_request_index(request_id) else {
            return ResponseOutcome::UnknownRequest;
        };
        if self.slots[index].is_none() {
            return ResponseOutcome::UnknownRequest;
        }
        let (state, outcome) = if status != GENERIC_RESPONSE_STATUS_ACCEPTED {
            (
                GenericAttachState::Denied { status },
                ResponseOutcome::Denied { status },
            )
        } else if device_id == GENERIC_DEVICE_ID_DISABLED {
            (GenericAttachState::Disabled, ResponseOutcome::Disabled)
        } else if device_id == GENERIC_DEVICE_ID_SILENT {
            (
                GenericAttachState::AcceptedSilent,
                ResponseOutcome::AcceptedSilent,
            )
        } else if self.device_id_owned_elsewhere(index, device_id, externally_owned_ids) {
            (
                GenericAttachState::Conflicted { device_id },
                ResponseOutcome::DeviceIdConflict { device_id },
            )
        } else {
            (
                GenericAttachState::Accepted { device_id },
                ResponseOutcome::Accepted { device_id },
            )
        };
        self.slots[index].as_mut().expect("checked above").state = state;
        outcome
    }

    fn device_id_owned_elsewhere(
        &self,
        index: usize,
        device_id: u8,
        externally_owned_ids: u16,
    ) -> bool {
        let owned_by_other_generic = self
            .slots
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .filter_map(|(_, slot)| slot.as_ref())
            .any(|slot| slot.state == GenericAttachState::Accepted { device_id });
        owned_by_other_generic
            || device_id_mask_bit(device_id).is_some_and(|bit| externally_owned_ids & bit != 0)
    }

    /// Transition attaches that waited longer than [`GENERIC_ATTACH_TIMEOUT`]
    /// to `TimedOut`. Each request id is returned exactly once.
    pub fn on_timeout(&mut self, now_us: u64) -> Vec<u8> {
        let timeout_us = GENERIC_ATTACH_TIMEOUT.as_micros() as u64;
        let mut expired = Vec::new();
        for slot in self.slots.iter_mut().flatten() {
            if let GenericAttachState::Pending { sent_at_us } = slot.state
                && now_us.saturating_sub(sent_at_us) >= timeout_us
            {
                slot.state = GenericAttachState::TimedOut;
                expired.push(slot.device.request_id);
            }
        }
        expired
    }

    /// Request id of the device forwarding under server-assigned `device_id`.
    /// Silent devices (server id 0) are never matched, so an inbound `0x11`
    /// for id 0 is not written to a device the server never asked to read.
    pub fn request_for_device_id(&self, device_id: u8) -> Option<u8> {
        self.slots
            .iter()
            .flatten()
            .find(|slot| slot.state == GenericAttachState::Accepted { device_id })
            .map(|slot| slot.device.request_id)
    }

    /// Server-assigned id every `0x11` report for `request_id` must carry;
    /// `None` unless the device is accepted and forwarding.
    pub fn accepted_device_id(&self, request_id: u8) -> Option<u8> {
        match self.slot(request_id)?.state {
            GenericAttachState::Accepted { device_id } => Some(device_id),
            _ => None,
        }
    }

    /// Bitmask (bit `n` = device id `n`) of server-assigned ids accepted for
    /// generic devices in this session, limited to the ids a `u16` can carry
    /// (see [`device_id_mask_bit`]); only those can collide with the DS4 lane.
    pub fn accepted_device_id_mask(&self) -> u16 {
        self.slots
            .iter()
            .flatten()
            .filter_map(|slot| match slot.state {
                GenericAttachState::Accepted { device_id } => device_id_mask_bit(device_id),
                _ => None,
            })
            .fold(0, |mask, bit| mask | bit)
    }

    /// Mark a device as gone (unplug / read error). Returns the removal to
    /// send when the device had been announced to the server.
    pub fn mark_removed(&mut self, request_id: u8) -> Option<GenericOutbound> {
        let slot = self.slot_mut(request_id)?;
        let removal = slot.removal();
        slot.state = GenericAttachState::Removed;
        removal
    }

    /// Withdraw every announced device and forget the inventory.
    pub fn retire_all(&mut self) -> Vec<GenericOutbound> {
        let mut outbound = Vec::with_capacity(MAX_GENERIC_DEVICES);
        for slot in self.slots.iter_mut() {
            if let Some(current) = slot.take() {
                outbound.extend(current.removal());
            }
        }
        outbound
    }
}

/// One raw input report captured from a hidraw node.
#[derive(Debug, Clone, Copy)]
pub struct GenericReport {
    pub request_id: u8,
    pub incarnation: u64,
    pub observed_at_us: u64,
    len: u8,
    bytes: [u8; MAX_GENERIC_REPORT_BYTES + 1],
}

impl GenericReport {
    pub fn new(
        request_id: u8,
        incarnation: u64,
        observed_at_us: u64,
        report: &[u8],
    ) -> Option<Self> {
        if report.is_empty() || report.len() > MAX_GENERIC_REPORT_BYTES {
            return None;
        }
        let mut bytes = [0_u8; MAX_GENERIC_REPORT_BYTES + 1];
        bytes[..report.len()].copy_from_slice(report);
        Some(Self {
            request_id,
            incarnation,
            observed_at_us,
            len: report.len() as u8,
            bytes,
        })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

impl PartialEq for GenericReport {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.incarnation == other.incarnation
            && self.observed_at_us == other.observed_at_us
            && self.as_slice() == other.as_slice()
    }
}

impl Eq for GenericReport {}

/// A HID output report the server asked us to write back to the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericOutputReport {
    pub request_id: u8,
    pub interface: u8,
    /// Third byte of the inbound `0x11` payload. Report id or flags; the
    /// official client's interpretation is unverified, so it is logged and
    /// the payload is written exactly as received.
    pub third: u8,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericAdmission {
    Admitted,
    /// Admitted after evicting the oldest buffered report (lane was full).
    DroppedOldest,
    Inactive,
    NotForwarding,
    Oversize,
    UnknownDevice,
    StaleIncarnation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenericCounters {
    pub reports_in: u64,
    pub reports_forwarded: u64,
    pub dropped_overflow: u64,
    pub dropped_inactive: u64,
    pub dropped_not_forwarding: u64,
    pub dropped_send_refused: u64,
    pub outputs_in: u64,
    pub outputs_coalesced: u64,
    pub outputs_written: u64,
    pub write_errors: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ds4::low_id_for_slot;

    fn identity() -> GenericHidIdentity {
        GenericHidIdentity {
            vendor: 0x0738,
            product: 0x2221,
            revision: 0x0100,
            interface: 0,
        }
    }

    fn device(request_id: u8, incarnation: u64) -> GenericHidDevice {
        GenericHidDevice {
            request_id,
            incarnation,
            identity: identity(),
            hidraw_path: format!("/dev/hidraw{}", 8 + request_id),
            sysfs_hid_id: "0003:00000738:00002221".to_owned(),
            name: "Mad Catz Saitek Pro Flight X-56 Rhino Stick".to_owned(),
        }
    }

    fn removal(request_id: u8, wire_id: u8) -> GenericOutbound {
        GenericOutbound::Removal {
            request_id,
            wire_id,
            identity: identity(),
        }
    }

    #[test]
    fn request_ids_are_disjoint_from_the_ds4_domain_and_capped_at_three() {
        let generic: Vec<u8> = (0..3)
            .map(|index| generic_request_id(index).unwrap())
            .collect();
        assert_eq!(generic, vec![0, 1, 2]);
        assert_eq!(generic_request_id(3), None);
        assert_eq!(generic_request_id(255), None);
        for slot in 0..crate::MAX_SOURCES as u8 {
            let ds4 = low_id_for_slot(slot).unwrap();
            assert!(
                !generic.contains(&ds4),
                "generic ids must never collide with DS4 id {ds4}"
            );
            assert_eq!(generic_request_index(ds4), None);
        }
        assert_eq!(generic_request_index(2), Some(2));
        assert_eq!(generic_request_index(3), None);
    }

    #[test]
    fn replace_announces_new_devices_and_withdraws_vanished_ones() {
        let mut session = GenericHidSession::default();
        let outbound = session.replace(&[device(0, 10), device(1, 11)]);
        assert_eq!(
            outbound,
            vec![
                GenericOutbound::Attach {
                    request_id: 0,
                    identity: identity()
                },
                GenericOutbound::Attach {
                    request_id: 1,
                    identity: identity()
                },
            ]
        );
        assert_eq!(
            session.pending_attaches(),
            vec![(0, identity()), (1, identity())]
        );
        assert!(session.mark_attach_sent(0, 1_000));
        assert!(!session.mark_attach_sent(0, 1_000), "attach is sent once");
        assert_eq!(session.pending_attaches(), vec![(1, identity())]);
        // Device 1 vanished before its attach was sent: no removal owed.
        // Device 0 is pending (never answered): removal carries its request
        // id. Device 2 is new.
        let outbound = session.replace(&[device(2, 12)]);
        assert_eq!(
            outbound,
            vec![
                removal(0, 0),
                GenericOutbound::Attach {
                    request_id: 2,
                    identity: identity()
                },
            ]
        );
        assert_eq!(session.state(0), None);
        assert_eq!(session.state(1), None);
        assert_eq!(session.state(2), Some(GenericAttachState::Idle));
        assert!(session.replace(&[device(2, 12)]).is_empty(), "idempotent");
    }

    #[test]
    fn a_fourth_device_and_zero_incarnations_are_ignored() {
        let mut session = GenericHidSession::default();
        let outbound = session.replace(&[
            device(0, 1),
            device(1, 2),
            device(2, 3),
            device(3, 4),
            device(9, 5),
        ]);
        assert_eq!(outbound.len(), 3);
        assert_eq!(session.state(3), None);
        let mut session = GenericHidSession::default();
        assert!(session.replace(&[device(0, 0)]).is_empty());
        assert!(session.is_empty());
    }

    #[test]
    fn observed_live_response_binds_the_server_assigned_id_25() {
        // Live seat reply to the X-56 attach (requestId 0): status 1, flags
        // echo 2, server-assigned device id 25.
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1)]);
        assert!(session.mark_attach_sent(0, 0));
        assert_eq!(
            session.on_response(0, 1, 2, 25, 0),
            ResponseOutcome::Accepted { device_id: 25 }
        );
        assert_eq!(
            session.state(0),
            Some(GenericAttachState::Accepted { device_id: 25 })
        );
        assert_eq!(session.accepted_device_id(0), Some(25));
        assert_eq!(session.request_for_device_id(25), Some(0));
        assert_eq!(
            session.request_for_device_id(0),
            None,
            "request id is not a wire id"
        );
        assert_eq!(
            session.accepted_device_id_mask(),
            0,
            "id 25 cannot collide with the DS4 domain and is outside the u16 mask"
        );
        assert_eq!(session.state(0).unwrap().wire_id(0), Some(25));
    }

    #[test]
    fn response_state_machine_accepts_denies_and_reports_unknown_requests() {
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1), device(1, 2)]);
        assert!(session.mark_attach_sent(0, 0));
        assert!(session.mark_attach_sent(1, 0));
        assert_eq!(
            session.on_response(0, 1, 0, 4, 0),
            ResponseOutcome::Accepted { device_id: 4 }
        );
        assert_eq!(session.accepted_device_id(0), Some(4));
        assert_eq!(session.request_for_device_id(4), Some(0));
        assert_eq!(session.accepted_device_id_mask(), 0b1_0000);
        // status 0 is a denial even though a device id is present.
        assert_eq!(
            session.on_response(1, 0, 0, 5, 0),
            ResponseOutcome::Denied { status: 0 }
        );
        assert_eq!(
            session.state(1),
            Some(GenericAttachState::Denied { status: 0 })
        );
        assert_eq!(session.accepted_device_id(1), None);
        assert!(
            !session.state(1).unwrap().announced(),
            "denied devices owe no removal"
        );
        // Any status other than exactly 1 denies.
        assert_eq!(
            session.on_response(1, 2, 0, 5, 0),
            ResponseOutcome::Denied { status: 2 }
        );
        assert_eq!(
            session.on_response(2, 1, 0, 6, 0),
            ResponseOutcome::UnknownRequest
        );
        assert_eq!(
            session.on_response(7, 1, 0, 6, 0),
            ResponseOutcome::UnknownRequest
        );
    }

    #[test]
    fn accepted_id_ff_disables_and_id_zero_admits_silently() {
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1), device(1, 2)]);
        assert!(session.mark_attach_sent(0, 0));
        assert!(session.mark_attach_sent(1, 0));
        assert_eq!(
            session.on_response(0, 1, 0, GENERIC_DEVICE_ID_DISABLED, 0),
            ResponseOutcome::Disabled
        );
        assert_eq!(session.state(0), Some(GenericAttachState::Disabled));
        assert_eq!(session.accepted_device_id(0), None);
        assert_eq!(session.request_for_device_id(0xff), None);
        assert!(!session.state(0).unwrap().announced());
        assert_eq!(
            session.mark_removed(0),
            None,
            "disabled devices owe no removal"
        );

        assert_eq!(
            session.on_response(1, 1, 0, GENERIC_DEVICE_ID_SILENT, 0),
            ResponseOutcome::AcceptedSilent
        );
        assert_eq!(session.state(1), Some(GenericAttachState::AcceptedSilent));
        assert_eq!(
            session.accepted_device_id(1),
            None,
            "silent devices forward nothing"
        );
        assert_eq!(
            session.request_for_device_id(0),
            None,
            "and receive no outputs"
        );
        assert_eq!(session.accepted_device_id_mask(), 0);
        // The server stored id 0, so the detach is sent with id 0.
        assert_eq!(session.mark_removed(1), Some(removal(1, 0)));
    }

    #[test]
    fn device_id_conflicts_refuse_forwarding_without_touching_the_owner() {
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1), device(1, 2), device(2, 3)]);
        for id in 0..3 {
            assert!(session.mark_attach_sent(id, 0));
        }
        assert_eq!(
            session.on_response(0, 1, 0, 3, 0),
            ResponseOutcome::Accepted { device_id: 3 }
        );
        assert_eq!(
            session.on_response(1, 1, 0, 3, 0),
            ResponseOutcome::DeviceIdConflict { device_id: 3 }
        );
        assert_eq!(session.request_for_device_id(3), Some(0));
        assert_eq!(session.accepted_device_id(1), None);
        assert_eq!(
            session.state(1),
            Some(GenericAttachState::Conflicted { device_id: 3 })
        );
        assert_eq!(
            session.mark_removed(1),
            None,
            "no detach by the shared id: it would withdraw the owner"
        );
        // A DS4 lane owning id 6 refuses a generic assignment of 6.
        let ds4_owned = 1_u16 << low_id_for_slot(0).unwrap();
        assert_eq!(
            session.on_response(2, 1, 0, 6, ds4_owned),
            ResponseOutcome::DeviceIdConflict { device_id: 6 }
        );
        // Server-assigned ids above the DS4 domain are plain u8 values: with
        // every DS4 lane attached (bits 6..=9) they are still accepted, and
        // ids the u16 mask cannot represent never collide at all.
        let all_ds4_owned = (0..crate::MAX_SOURCES as u8)
            .filter_map(low_id_for_slot)
            .fold(0_u16, |mask, low_id| mask | (1 << low_id));
        assert_eq!(all_ds4_owned, 0b11_1100_0000);
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1), device(1, 2), device(2, 3)]);
        for id in 0..3 {
            assert!(session.mark_attach_sent(id, 0));
        }
        assert_eq!(
            session.on_response(0, 1, 0, 10, all_ds4_owned),
            ResponseOutcome::Accepted { device_id: 10 }
        );
        assert_eq!(
            session.on_response(1, 1, 0, 200, u16::MAX),
            ResponseOutcome::Accepted { device_id: 200 }
        );
        assert_eq!(
            session.on_response(2, 1, 0, 9, all_ds4_owned),
            ResponseOutcome::DeviceIdConflict { device_id: 9 }
        );
        assert_eq!(session.accepted_device_id(0), Some(10));
        assert_eq!(session.accepted_device_id(1), Some(200));
        assert_eq!(session.accepted_device_id(2), None);
        assert_eq!(session.accepted_device_id_mask(), 1 << 10);
        assert_eq!(device_id_mask_bit(15), Some(1 << 15));
        assert_eq!(device_id_mask_bit(16), None);
        assert_eq!(device_id_mask_bit(25), None);
    }

    #[test]
    fn attach_timeout_fires_once_and_keeps_listening_for_a_late_response() {
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1)]);
        assert!(session.mark_attach_sent(0, 1_000_000));
        let timeout_us = GENERIC_ATTACH_TIMEOUT.as_micros() as u64;
        assert!(session.on_timeout(1_000_000 + timeout_us - 1).is_empty());
        assert_eq!(session.on_timeout(1_000_000 + timeout_us), vec![0]);
        assert_eq!(session.state(0), Some(GenericAttachState::TimedOut));
        assert!(session.on_timeout(1_000_000 + timeout_us * 2).is_empty());
        // The official client has no timeout; a late accept still binds.
        assert_eq!(
            session.on_response(0, 1, 0, 2, 0),
            ResponseOutcome::Accepted { device_id: 2 }
        );
        assert_eq!(session.accepted_device_id(0), Some(2));
    }

    #[test]
    fn removal_uses_the_server_id_after_accept_and_the_request_id_before() {
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1), device(1, 2), device(2, 3)]);
        assert_eq!(session.mark_removed(0), None, "idle devices owe nothing");
        // Pending (never answered): the server only knows the request id.
        assert!(session.mark_attach_sent(1, 0));
        assert_eq!(session.mark_removed(1), Some(removal(1, 1)));
        assert_eq!(session.mark_removed(1), None, "removal is sent once");
        // Accepted: the detach names the server-assigned id, not the request id.
        assert!(session.mark_attach_sent(2, 0));
        assert_eq!(
            session.on_response(2, 1, 0, 25, 0),
            ResponseOutcome::Accepted { device_id: 25 }
        );
        assert_eq!(session.retire_all(), vec![removal(2, 25)]);
        assert!(session.is_empty());
        assert!(session.retire_all().is_empty());
        assert_eq!(session.mark_removed(5), None);
        // Timed out (still unanswered) keeps the request id; a replaced
        // accepted device is withdrawn by its server id.
        let mut session = GenericHidSession::default();
        let _ = session.replace(&[device(0, 1), device(1, 2)]);
        assert!(session.mark_attach_sent(0, 0));
        assert!(session.mark_attach_sent(1, 0));
        let timeout_us = GENERIC_ATTACH_TIMEOUT.as_micros() as u64;
        assert_eq!(session.on_timeout(timeout_us), vec![0, 1]);
        assert_eq!(
            session.on_response(1, 1, 0, 9, 0),
            ResponseOutcome::Accepted { device_id: 9 }
        );
        assert_eq!(session.replace(&[]), vec![removal(0, 0), removal(1, 9)]);
    }

    #[test]
    fn state_labels_are_stable_diagnostic_tokens() {
        assert_eq!(GenericAttachState::Idle.label(), "idle");
        assert_eq!(
            GenericAttachState::Pending { sent_at_us: 0 }.label(),
            "pending"
        );
        assert_eq!(
            GenericAttachState::Accepted { device_id: 25 }.label(),
            "accepted"
        );
        assert_eq!(
            GenericAttachState::AcceptedSilent.label(),
            "accepted-silent"
        );
        assert_eq!(GenericAttachState::Denied { status: 0 }.label(), "denied");
        assert_eq!(GenericAttachState::Disabled.label(), "disabled");
        assert_eq!(
            GenericAttachState::Conflicted { device_id: 6 }.label(),
            "conflicted"
        );
        assert_eq!(GenericAttachState::TimedOut.label(), "timeout");
        assert_eq!(GenericAttachState::Removed.label(), "removed");
    }

    #[test]
    fn reports_are_bounded_and_keep_their_bytes() {
        assert!(GenericReport::new(0, 1, 0, &[]).is_none());
        let max = vec![0xab_u8; MAX_GENERIC_REPORT_BYTES];
        let report = GenericReport::new(0, 1, 7, &max).unwrap();
        assert_eq!(report.as_slice(), &max[..]);
        assert_eq!(report.observed_at_us, 7);
        let oversize = vec![0_u8; MAX_GENERIC_REPORT_BYTES + 1];
        assert!(GenericReport::new(0, 1, 0, &oversize).is_none());
        let eleven = GenericReport::new(2, 9, 1, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]).unwrap();
        assert_eq!(eleven.as_slice().len(), 11);
        assert_eq!(
            eleven,
            GenericReport::new(2, 9, 1, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]).unwrap()
        );
    }
}
