use crate::ds4::{Ds4ReportState, low_id_for_slot, slot_for_low_id};
use crate::{InventoryView, SdlDeviceClaim, SonyCapability, SonySnapshot};

pub const DS4_OUTPUT_REPORT_ID: u8 = 5;
pub const DS4_OUTPUT_ENABLE_BIT: u8 = 0x01;
pub const DS4_OUTPUT_MIN_BYTES: usize = 6;
pub const DS4_OUTPUT_LOW_BYTE: usize = 4;
pub const DS4_OUTPUT_HIGH_BYTE: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SonyRumble {
    pub slot: u8,
    pub incarnation: u64,
    pub low_frequency: u16,
    pub high_frequency: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidAttachment {
    Attached,
    Removed,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidOutbound {
    Attach {
        low_id: u8,
    },
    Report {
        low_id: u8,
        bytes: Box<[u8; crate::ds4::DS4_REPORT_BYTES]>,
    },
    Release {
        low_id: u8,
        bytes: Box<[u8; crate::ds4::DS4_REPORT_BYTES]>,
    },
    Removal {
        low_id: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AdmittedSource {
    slot: u8,
    incarnation: u64,
    vendor: u16,
    product: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidSession {
    capability: SonyCapability,
    sources: [Option<AdmittedSource>; crate::MAX_SOURCES],
    tombstones: [Option<u64>; crate::MAX_SOURCES],
    report_states: [Ds4ReportState; crate::MAX_SOURCES],
}

impl Default for HidSession {
    fn default() -> Self {
        Self::new(SonyCapability::default())
    }
}

impl HidSession {
    pub fn new(capability: SonyCapability) -> Self {
        Self {
            capability,
            sources: [None; crate::MAX_SOURCES],
            tombstones: [None; crate::MAX_SOURCES],
            report_states: Default::default(),
        }
    }

    pub fn capability(&self) -> SonyCapability {
        self.capability
    }

    pub fn set_server_mask(&mut self, server_mask: u32) {
        self.capability.server_mask = server_mask;
    }
    pub fn attached(&self, slot: u8) -> Option<u64> {
        self.sources
            .get(usize::from(slot))
            .copied()
            .flatten()
            .map(|source| source.incarnation)
    }

    pub fn is_tombstoned(&self, slot: u8) -> bool {
        self.tombstones
            .get(usize::from(slot))
            .is_some_and(|entry| entry.is_some())
    }

    pub fn rich_slot_mask(&self) -> u16 {
        let mut mask = 0_u16;
        for (slot, source) in self.sources.iter().enumerate() {
            if source.is_some() {
                mask |= 1 << slot;
            }
        }
        mask
    }

    pub fn reconcile(&mut self, inventory: &InventoryView) -> Vec<HidOutbound> {
        self.reconcile_excluding(inventory, 0)
    }

    /// [`reconcile`](Self::reconcile) that refuses to admit a slot whose
    /// synthesized low id (bit `n` of `foreign_low_ids` = id `n`) is already
    /// owned by another HID lane, such as a generic device the seat accepted
    /// under that id. The refused slot stays unadmitted and is retried on a
    /// later call once the id is free; an already admitted slot is left alone.
    pub fn reconcile_excluding(
        &mut self,
        inventory: &InventoryView,
        foreign_low_ids: u16,
    ) -> Vec<HidOutbound> {
        let mut outbound = Vec::with_capacity(crate::MAX_SOURCES * 2);
        for slot in 0..crate::MAX_SOURCES as u8 {
            let wanted = inventory.claim(slot);
            let foreign = low_id_for_slot(slot)
                .is_some_and(|low_id| foreign_low_ids & (1_u16 << low_id) != 0);
            match (self.sources[usize::from(slot)], wanted) {
                (Some(current), Some(claim)) if current.incarnation == claim.incarnation => {}
                (Some(_), _) => {
                    self.retire_slot(slot, &mut outbound);
                    if let Some(claim) = wanted
                        && !foreign
                    {
                        self.admit(claim, &mut outbound);
                    }
                }
                (None, Some(claim)) => {
                    if !foreign {
                        self.admit(claim, &mut outbound);
                    }
                }
                (None, None) => {}
            }
        }
        outbound
    }

    /// Slots in `inventory` that [`reconcile_excluding`](Self::reconcile_excluding)
    /// would refuse (or did refuse) because their low id is in `foreign_low_ids`.
    pub fn slots_refused_by(inventory: &InventoryView, foreign_low_ids: u16) -> u16 {
        (0..crate::MAX_SOURCES as u8)
            .filter(|slot| inventory.claim(*slot).is_some())
            .filter(|slot| {
                low_id_for_slot(*slot)
                    .is_some_and(|low_id| foreign_low_ids & (1_u16 << low_id) != 0)
            })
            .fold(0_u16, |mask, slot| mask | (1 << slot))
    }

    fn admit(&mut self, claim: SdlDeviceClaim, outbound: &mut Vec<HidOutbound>) -> HidAttachment {
        if usize::from(claim.slot) >= crate::MAX_SOURCES || claim.incarnation == 0 {
            return HidAttachment::Refused;
        }
        let index = usize::from(claim.slot);
        let Some((target, cross_synthesis)) = claim.sony_product() else {
            return HidAttachment::Refused;
        };
        if !self.capability.admits(target, cross_synthesis) {
            return HidAttachment::Refused;
        }
        let Some(low_id) = low_id_for_slot(claim.slot) else {
            return HidAttachment::Refused;
        };
        if self.tombstones[index].is_some() {
            return HidAttachment::Refused;
        }
        self.sources[index] = Some(AdmittedSource {
            slot: claim.slot,
            incarnation: claim.incarnation,
            vendor: claim.vendor,
            product: claim.product,
        });
        self.report_states[index] = Ds4ReportState::default();
        outbound.push(HidOutbound::Attach { low_id });
        HidAttachment::Attached
    }

    pub fn retire(&mut self, slot: u8) -> Vec<HidOutbound> {
        let mut outbound = Vec::with_capacity(2);
        self.retire_slot(slot, &mut outbound);
        outbound
    }

    fn retire_slot(&mut self, slot: u8, outbound: &mut Vec<HidOutbound>) {
        let index = usize::from(slot);
        let Some(source) = self.sources.get(index).copied().flatten() else {
            return;
        };
        self.sources[index] = None;
        self.tombstones[index] = Some(source.incarnation);
        if let Some(low_id) = low_id_for_slot(slot) {
            let bytes = self.release_report(slot);
            self.report_states[index] = Ds4ReportState::default();
            outbound.push(HidOutbound::Release {
                low_id,
                bytes: Box::new(bytes),
            });
            outbound.push(HidOutbound::Removal { low_id });
        } else {
            self.report_states[index] = Ds4ReportState::default();
        }
    }

    fn release_report(&mut self, slot: u8) -> [u8; crate::ds4::DS4_REPORT_BYTES] {
        let state = &mut self.report_states[usize::from(slot)];
        let mut release = SonySnapshot::neutral(slot, 1, state.last_observed_at_us());
        let previous = *state.contacts();
        release.contacts = previous.map(|contact| crate::SonyContact {
            active: false,
            x: contact.x,
            y: contact.y,
        });
        state.build(&release)
    }

    pub fn observe_slot(&mut self, slot: u8, observed_at_us: u64) {
        if let Some(state) = self.report_states.get_mut(usize::from(slot)) {
            state.observe(observed_at_us);
        }
    }

    pub fn retire_all(&mut self) -> Vec<HidOutbound> {
        let mut outbound = Vec::with_capacity(crate::MAX_SOURCES * 2);
        for slot in 0..crate::MAX_SOURCES as u8 {
            self.retire_slot(slot, &mut outbound);
        }
        outbound
    }

    pub fn build_report(
        &mut self,
        snapshot: &SonySnapshot,
    ) -> Option<(u8, [u8; crate::ds4::DS4_REPORT_BYTES])> {
        let index = usize::from(snapshot.slot);
        let source = self.sources.get(index).copied().flatten()?;
        if source.incarnation != snapshot.incarnation {
            return None;
        }
        let low_id = low_id_for_slot(snapshot.slot)?;
        Some((low_id, self.report_states[index].build(snapshot)))
    }

    pub fn release_snapshot(
        &mut self,
        slot: u8,
    ) -> Option<(u8, [u8; crate::ds4::DS4_REPORT_BYTES])> {
        let index = usize::from(slot);
        self.sources.get(index).copied().flatten()?;
        let low_id = low_id_for_slot(slot)?;
        Some((low_id, self.release_report(slot)))
    }

    pub fn take_output(&self, payload: &[u8], low_id: u8) -> Option<SonyRumble> {
        let slot = slot_for_low_id(low_id)?;
        let source = self.sources.get(usize::from(slot)).copied().flatten()?;
        if payload.len() < DS4_OUTPUT_MIN_BYTES {
            return None;
        }
        if payload[0] != DS4_OUTPUT_REPORT_ID {
            return None;
        }
        if payload[1] & DS4_OUTPUT_ENABLE_BIT == 0 {
            return None;
        }
        Some(SonyRumble {
            slot,
            incarnation: source.incarnation,
            low_frequency: u16::from(payload[DS4_OUTPUT_LOW_BYTE]) << 8,
            high_frequency: u16::from(payload[DS4_OUTPUT_HIGH_BYTE]) << 8,
        })
    }

    pub fn adopt(&mut self, claim: SdlDeviceClaim) -> Result<HidAttachment, HidSessionError> {
        if claim.sony_product().is_none() {
            return Err(HidSessionError::UnsupportedIdentity);
        }
        let mut outbound = Vec::new();
        Ok(self.admit(claim, &mut outbound))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidSessionError {
    UnsupportedIdentity,
}

impl std::fmt::Display for HidSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedIdentity => {
                formatter.write_str("identity is not an admitted Sony source")
            }
        }
    }
}

impl std::error::Error for HidSessionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InventoryOutcome, SONY_VENDOR, SonyContact, SonyTarget};

    fn ds4(slot: u8, incarnation: u64) -> Option<SdlDeviceClaim> {
        SdlDeviceClaim::new(slot, incarnation, SONY_VENDOR, 0x05c4)
    }

    fn ds5(slot: u8, incarnation: u64) -> Option<SdlDeviceClaim> {
        SdlDeviceClaim::new(slot, incarnation, SONY_VENDOR, 0x0ce6)
    }

    fn xbox(slot: u8, incarnation: u64) -> Option<SdlDeviceClaim> {
        SdlDeviceClaim::new(slot, incarnation, 0x045e, 0x02ea)
    }

    fn inventory(claims: [Option<SdlDeviceClaim>; 4]) -> InventoryView {
        let mut view = InventoryView::default();
        assert_eq!(view.replace(&claims), InventoryOutcome::Replaced);
        view
    }

    fn snapshot(slot: u8, incarnation: u64) -> SonySnapshot {
        SonySnapshot::neutral(slot, incarnation, 0)
    }

    #[test]
    fn admission_attaches_only_supported_sony_sources() {
        let mut session = session_with_server_mask(0x5);
        let outbound = session.reconcile(&inventory([
            Some(ds4(0, 1).unwrap()),
            xbox(1, 2),
            None,
            None,
        ]));
        assert_eq!(outbound, vec![HidOutbound::Attach { low_id: 6 }]);
        assert_eq!(session.attached(0), Some(1));
        assert_eq!(session.attached(1), None);
        assert_eq!(session.rich_slot_mask(), 0b0001);
    }

    #[test]
    fn reconcile_refuses_slots_whose_low_id_a_generic_device_owns() {
        let mut session = session_with_server_mask(0x5);
        let view = inventory([
            Some(ds4(0, 1).unwrap()),
            Some(ds4(1, 2).unwrap()),
            None,
            None,
        ]);
        // The seat accepted a generic device as id 6 (= DS4 slot 0's low id).
        let generic_ids = 1_u16 << 6;
        assert_eq!(HidSession::slots_refused_by(&view, generic_ids), 0b0001);
        let outbound = session.reconcile_excluding(&view, generic_ids);
        assert_eq!(outbound, vec![HidOutbound::Attach { low_id: 7 }]);
        assert_eq!(session.attached(0), None, "slot 0 is refused, not attached");
        assert_eq!(session.attached(1), Some(2));
        // Once the id is free again the refused slot attaches on the next pass.
        let outbound = session.reconcile_excluding(&view, 0);
        assert_eq!(outbound, vec![HidOutbound::Attach { low_id: 6 }]);
        assert_eq!(session.attached(0), Some(1));
        // An already admitted slot is never retired by the mask alone.
        let outbound = session.reconcile_excluding(&view, generic_ids);
        assert!(outbound.is_empty());
        assert_eq!(session.attached(0), Some(1));
        assert_eq!(HidSession::slots_refused_by(&view, 0), 0);
    }

    #[test]
    fn dualsense_cross_synthesis_is_locally_enabled_and_needs_no_ds5_server_bit() {
        let mut session = session_with_server_mask(0x5);
        assert_eq!(
            session.adopt(ds5(0, 1).unwrap()).unwrap(),
            HidAttachment::Attached
        );
        assert_eq!(session.attached(0), Some(1));
        assert!(!session.is_tombstoned(0));
    }

    #[test]
    fn ds4_support_without_game_permission_falls_back_to_ordinary_behavior() {
        let mut session = session_with_server_mask(0x4);
        assert_eq!(
            session.adopt(ds4(0, 1).unwrap()).unwrap(),
            HidAttachment::Refused
        );
        assert_eq!(session.attached(0), None);
        let mut session = session_with_server_mask(0x1);
        assert_eq!(
            session.adopt(ds4(0, 1).unwrap()).unwrap(),
            HidAttachment::Refused
        );
        let mut session = session_with_server_mask(0x0);
        assert_eq!(
            session.adopt(ds5(0, 1).unwrap()).unwrap(),
            HidAttachment::Refused
        );
    }

    #[test]
    fn a_disabled_local_cross_synthesis_policy_refuses_dualsense_but_keeps_ds4() {
        let mut capability = SonyCapability {
            server_mask: 0x5,
            ..SonyCapability::default()
        };
        capability.cross_synthesize_ds5_to_ds4 = false;
        let mut session = HidSession::new(capability);
        assert_eq!(
            session.adopt(ds5(0, 1).unwrap()).unwrap(),
            HidAttachment::Refused
        );
        assert_eq!(
            session.adopt(ds4(1, 2).unwrap()).unwrap(),
            HidAttachment::Attached
        );
    }

    #[test]
    fn a_disabled_local_ds4_support_byte_refuses_every_sony_source() {
        let capability = SonyCapability {
            server_mask: 0x5,
            synthesized_ds4_support: false,
            ..SonyCapability::default()
        };
        let mut session = HidSession::new(capability);
        assert_eq!(
            session.adopt(ds4(0, 1).unwrap()).unwrap(),
            HidAttachment::Refused
        );
        assert_eq!(
            session.adopt(ds5(1, 2).unwrap()).unwrap(),
            HidAttachment::Refused
        );
        assert_eq!(session.rich_slot_mask(), 0);
    }

    fn session_with_server_mask(server_mask: u32) -> HidSession {
        let mut session = HidSession::new(SonyCapability {
            server_mask,
            ..SonyCapability::default()
        });
        session.set_server_mask(server_mask);
        session
    }

    #[test]
    fn unsupported_identity_is_rejected_before_any_state_change() {
        let mut session = session_with_server_mask(0x5);
        assert_eq!(
            session.adopt(xbox(0, 1).unwrap()),
            Err(HidSessionError::UnsupportedIdentity)
        );
        assert!(session.retire_all().is_empty());
    }

    #[test]
    fn publication_requires_the_admitted_incarnation() {
        let mut session = session_with_server_mask(0x5);
        let adopted = session.adopt(ds4(2, 77).unwrap()).unwrap();
        assert_eq!(adopted, HidAttachment::Attached);
        assert_eq!(session.attached(2), Some(77));
        assert!(session.build_report(&snapshot(2, 78)).is_none());
        assert_eq!(
            session.build_report(&snapshot(2, 77)).map(|(id, _)| id),
            Some(8)
        );
        assert!(session.build_report(&snapshot(1, 77)).is_none());
    }

    #[test]
    fn retire_sends_release_before_removal_and_keeps_a_tombstone() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(1, 40).unwrap());
        let outbound = session.retire(1);
        assert_eq!(outbound.len(), 2);
        assert!(matches!(
            outbound[0],
            HidOutbound::Release { low_id: 7, .. }
        ));
        assert_eq!(outbound[1], HidOutbound::Removal { low_id: 7 });
        assert!(session.is_tombstoned(1));
        assert_eq!(session.attached(1), None);
    }

    #[test]
    fn retired_low_ids_are_not_rebound_to_another_incarnation_on_the_same_association() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(1, 40).unwrap());
        let _ = session.retire(1);
        assert_eq!(
            session.adopt(ds4(1, 41).unwrap()).unwrap(),
            HidAttachment::Refused
        );
        assert_eq!(session.attached(1), None);
        let outbound = session.reconcile(&inventory([None, ds4(1, 41), None, None]));
        assert!(outbound.is_empty());
        assert_eq!(session.attached(1), None);
    }

    #[test]
    fn a_new_association_clears_tombstones_and_reopens_the_slot() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(1, 40).unwrap());
        let _ = session.retire(1);
        let mut fresh = session_with_server_mask(0x5);
        assert_eq!(
            fresh.adopt(ds4(1, 41).unwrap()).unwrap(),
            HidAttachment::Attached
        );
        assert_eq!(fresh.attached(1), Some(41));
        assert!(!session.is_tombstoned(0));
    }

    #[test]
    fn reconcile_retires_the_old_source_before_attaching_the_replacement_elsewhere() {
        let mut session = session_with_server_mask(0x5);
        let first = inventory([Some(ds4(0, 1).unwrap()), None, None, None]);
        assert_eq!(
            session.reconcile(&first),
            vec![HidOutbound::Attach { low_id: 6 }]
        );
        let second = inventory([None, Some(ds4(1, 2).unwrap()), None, None]);
        let outbound = session.reconcile(&second);
        assert!(matches!(
            outbound[0],
            HidOutbound::Release { low_id: 6, .. }
        ));
        assert_eq!(outbound[1], HidOutbound::Removal { low_id: 6 });
        assert_eq!(outbound[2], HidOutbound::Attach { low_id: 7 });
        assert_eq!(session.attached(0), None);
        assert_eq!(session.attached(1), Some(2));
    }

    #[test]
    fn repeated_reconcile_with_the_same_inventory_is_idempotent() {
        let mut session = session_with_server_mask(0x5);
        let view = inventory([Some(ds4(0, 1).unwrap()), None, None, None]);
        let _ = session.reconcile(&view);
        assert!(session.reconcile(&view).is_empty());
        assert!(session.reconcile(&view).is_empty());
    }

    #[test]
    fn report_building_publishes_the_full_64_byte_report_for_the_admitted_slot() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(3, 9).unwrap());
        let mut state = snapshot(3, 9);
        state.contacts[0] = SonyContact {
            active: true,
            x: 0,
            y: 0,
        };
        let (low_id, report) = session.build_report(&state).unwrap();
        assert_eq!(low_id, 9);
        assert_eq!(report.len(), 64);
        assert_eq!(report[35], 0x01);
    }

    #[test]
    fn low_id_output_only_accepts_the_proved_report_id_and_enable_bit() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(0, 5).unwrap());
        assert_eq!(
            session.take_output(&[5, 0x01, 0, 0, 0x40, 0x80], 6),
            Some(SonyRumble {
                slot: 0,
                incarnation: 5,
                low_frequency: 0x4000,
                high_frequency: 0x8000,
            })
        );
        assert_eq!(session.take_output(&[5, 0x00, 0, 0, 0x40, 0x80], 6), None);
        assert_eq!(session.take_output(&[4, 0x01, 0, 0, 0x40, 0x80], 6), None);
        assert_eq!(session.take_output(&[5, 0x01, 0, 0, 0x40], 6), None);
        assert_eq!(session.take_output(&[5, 0x01, 0, 0, 0x40, 0x80], 5), None);
        assert_eq!(session.take_output(&[5, 0x01, 0, 0, 0x40, 0x80], 10), None);
    }

    #[test]
    fn low_id_output_for_a_retired_slot_is_dropped() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(0, 5).unwrap());
        let _ = session.retire(0);
        assert_eq!(session.take_output(&[5, 0x01, 0, 0, 0x40, 0x80], 6), None);
    }

    #[test]
    fn low_id_output_cannot_drive_a_replacement_controller_in_the_same_slot() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(0, 5).unwrap());
        let stale = session
            .take_output(&[5, 0x01, 0, 0, 0x11, 0x22], 6)
            .unwrap();
        assert_eq!(stale.incarnation, 5);
        let _ = session.retire(0);
        let _ = session.adopt(ds4(0, 6).unwrap());
        assert_ne!(session.attached(0), Some(stale.incarnation));
        assert_eq!(session.take_output(&[5, 0x01, 0, 0, 0x11, 0x22], 6), None);
    }

    #[test]
    fn retire_all_releases_every_admitted_source_in_slot_order() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.reconcile(&inventory([
            Some(ds4(0, 1).unwrap()),
            Some(ds4(1, 2).unwrap()),
            None,
            Some(ds4(3, 4).unwrap()),
        ]));
        let outbound = session.retire_all();
        assert_eq!(outbound.len(), 6);
        assert!(matches!(
            outbound[0],
            HidOutbound::Release { low_id: 6, .. }
        ));
        assert_eq!(outbound[1], HidOutbound::Removal { low_id: 6 });
        assert!(matches!(
            outbound[2],
            HidOutbound::Release { low_id: 7, .. }
        ));
        assert_eq!(outbound[3], HidOutbound::Removal { low_id: 7 });
        assert!(matches!(
            outbound[4],
            HidOutbound::Release { low_id: 9, .. }
        ));
        assert_eq!(outbound[5], HidOutbound::Removal { low_id: 9 });
        assert!(session.retire_all().is_empty());
    }

    #[test]
    fn release_report_retains_last_contact_positions_but_clears_activity() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(0, 1).unwrap());
        let mut state = snapshot(0, 1);
        state.contacts[0] = SonyContact {
            active: true,
            x: 32767,
            y: 32767,
        };
        let _ = session.build_report(&state);
        let (_, release) = session.release_snapshot(0).unwrap();
        assert_eq!(release[35], 0x81);
        assert_eq!(&release[35..39], &[0x81, 0x7f, 0xe7, 0x3a]);
        assert_eq!(&release[1..5], &[128, 128, 128, 128]);
    }

    #[test]
    fn parent_review_release_retains_last_observation_counter() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(0, 1).unwrap());
        let mut state = snapshot(0, 1);
        state.observed_at_us = 1_000_000;
        state.buttons = 0x1000;
        state.contacts[0].active = true;
        let (_, prior) = session.build_report(&state).unwrap();
        let (_, release) = session.release_snapshot(0).unwrap();
        assert_ne!(&prior[10..12], &[0, 0]);
        assert_eq!(
            &release[10..12],
            &prior[10..12],
            "capture release must carry the chosen last-observation timestamp, not zero"
        );
        assert_eq!(release[34], prior[34]);
    }

    #[test]
    fn release_snapshot_is_unavailable_for_an_unpublished_slot() {
        let mut session = session_with_server_mask(0x5);
        assert!(session.release_snapshot(2).is_none());
    }

    #[test]
    fn retirement_release_keeps_the_last_contact_positions() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(1, 3).unwrap());
        let mut state = snapshot(1, 3);
        state.contacts[0] = SonyContact {
            active: true,
            x: 16384,
            y: -16384,
        };
        let _ = session.build_report(&state);
        let outbound = session.retire(1);
        let HidOutbound::Release { low_id, bytes } = &outbound[0] else {
            panic!("retirement must release before removal");
        };
        assert_eq!(*low_id, 7);
        assert_eq!(&bytes[35..39], &[0x81, 0x9f, 0xb5, 0x0e]);
        assert_eq!(&bytes[1..5], &[128, 128, 128, 128]);
        assert_eq!(outbound[1], HidOutbound::Removal { low_id: 7 });
    }

    #[test]
    fn report_building_rejects_a_slot_the_source_map_cannot_index() {
        let mut session = session_with_server_mask(0x5);
        let _ = session.adopt(ds4(0, 1).unwrap());
        let mut out_of_range = snapshot(0, 1);
        out_of_range.slot = 9;
        assert!(session.build_report(&out_of_range).is_none());
        assert!(session.release_snapshot(9).is_none());
        assert_eq!(session.take_output(&[5, 0x01, 0, 0, 0x40, 0x80], 15), None);
    }

    #[test]
    fn adoption_refuses_a_claim_the_slot_bounds_cannot_hold() {
        let mut session = session_with_server_mask(0x5);
        let claim = SdlDeviceClaim {
            slot: 9,
            incarnation: 1,
            vendor: SONY_VENDOR,
            product: 0x05c4,
        };
        assert_eq!(session.adopt(claim).unwrap(), HidAttachment::Refused);
        assert_eq!(session.rich_slot_mask(), 0);
    }

    #[test]
    fn target_selection_stays_ds4_for_every_admitted_sony_source() {
        let claim = ds5(0, 1).unwrap();
        assert_eq!(claim.sony_product(), Some((SonyTarget::Ds4, true)));
        let ds4_claim = ds4(0, 1).unwrap();
        assert_eq!(ds4_claim.sony_product(), Some((SonyTarget::Ds4, false)));
    }
}
