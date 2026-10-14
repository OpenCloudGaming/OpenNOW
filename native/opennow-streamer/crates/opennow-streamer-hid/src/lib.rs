use std::fmt;

pub mod ds4;
pub mod generic;
pub mod runtime;
pub mod session;

pub use ds4::{DS4_INTERFACE, DS4_LOW_ID_BASE, DS4_REPORT_BYTES, Ds4ReportState};
pub use generic::{
    DEVICE_ID_MASK_BITS, GENERIC_ATTACH_TIMEOUT, GENERIC_DEVICE_CHANGE_FLAGS,
    GENERIC_DEVICE_ID_DISABLED, GENERIC_DEVICE_ID_SILENT, GENERIC_DRAIN_PER_DEVICE,
    GENERIC_LANE_CAPACITY, GENERIC_OUTPUT_MAILBOX, GENERIC_REQUEST_ID_BASE,
    GENERIC_RESPONSE_STATUS_ACCEPTED, GenericAdmission, GenericAttachState, GenericCounters,
    GenericHidDevice, GenericHidIdentity, GenericHidSession, GenericOutbound, GenericOutputReport,
    GenericReport, MAX_GENERIC_DEVICES, MAX_GENERIC_REPORT_BYTES, ResponseOutcome,
    device_id_mask_bit, generic_request_id, generic_request_index,
};
pub use runtime::{
    EndpointToken, HidRuntime, IngressItem, InventoryUpdate, LaneFault, ReleaseRequest,
    SessionBinding, SourceMode,
};
pub use session::{HidAttachment, HidOutbound, HidSession, SonyRumble};
pub const SONY_VENDOR: u16 = 0x054c;
pub const SONY_DS4_PRODUCTS: [u16; 3] = [0x05c4, 0x09cc, 0x0ba0];
pub const SONY_DS5_PRODUCTS: [u16; 2] = [0x0ce6, 0x0df2];

pub const MAX_SOURCES: usize = 4;
pub const CONTACTS_PER_SOURCE: usize = 2;
pub const MAX_QUEUED_STATES: usize = 64;
pub const MAX_QUEUED_STATE_BYTES: usize = 8 * 1024;
pub const DRAIN_PER_ITERATION: usize = 32;
pub const MAX_EXACT_INCARNATION: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonyTarget {
    Ds4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SonyCapability {
    pub synthesized_ds4_support: bool,
    pub server_mask: u32,
    pub ds4_source_allowed: bool,
    pub dualsense_source_allowed: bool,
    pub cross_synthesize_ds5_to_ds4: bool,
}

impl Default for SonyCapability {
    fn default() -> Self {
        Self {
            synthesized_ds4_support: true,
            server_mask: 0,
            ds4_source_allowed: true,
            dualsense_source_allowed: true,
            cross_synthesize_ds5_to_ds4: true,
        }
    }
}

impl SonyCapability {
    pub const SERVER_DS4_TARGET_BIT: u32 = 0x4;
    pub const SERVER_GAME_PERMISSION_BIT: u32 = 0x1;

    pub fn server_supports_ds4(self) -> bool {
        self.server_mask & Self::SERVER_DS4_TARGET_BIT != 0
    }

    pub fn game_allows_ds4(self) -> bool {
        self.server_mask & Self::SERVER_GAME_PERMISSION_BIT != 0
    }

    pub fn admits(self, target: SonyTarget, cross_synthesis: bool) -> bool {
        self.synthesized_ds4_support
            && self.server_supports_ds4()
            && self.game_allows_ds4()
            && match (target, cross_synthesis) {
                (SonyTarget::Ds4, false) => self.ds4_source_allowed,
                (SonyTarget::Ds4, true) => {
                    self.dualsense_source_allowed && self.cross_synthesize_ds5_to_ds4
                }
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdlDeviceClaim {
    pub slot: u8,
    pub incarnation: u64,
    pub vendor: u16,
    pub product: u16,
}

impl SdlDeviceClaim {
    pub fn new(slot: u8, incarnation: u64, vendor: u16, product: u16) -> Option<Self> {
        if usize::from(slot) >= MAX_SOURCES
            || incarnation == 0
            || incarnation > MAX_EXACT_INCARNATION
        {
            return None;
        }
        Some(Self {
            slot,
            incarnation,
            vendor,
            product,
        })
    }

    pub fn sony_product(self) -> Option<(SonyTarget, bool)> {
        if self.vendor != SONY_VENDOR {
            return None;
        }
        if SONY_DS4_PRODUCTS.contains(&self.product) {
            return Some((SonyTarget::Ds4, false));
        }
        if SONY_DS5_PRODUCTS.contains(&self.product) {
            return Some((SonyTarget::Ds4, true));
        }
        None
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SonyContact {
    pub active: bool,
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SonySnapshot {
    pub slot: u8,
    pub incarnation: u64,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
    pub touchpad_click: bool,
    pub contacts: [SonyContact; CONTACTS_PER_SOURCE],
    pub observed_at_us: u64,
}

impl SonySnapshot {
    pub fn neutral(slot: u8, incarnation: u64, observed_at_us: u64) -> Self {
        Self {
            slot,
            incarnation,
            buttons: 0,
            left_trigger: 0,
            right_trigger: 0,
            left_stick_x: 0,
            left_stick_y: 0,
            right_stick_x: 0,
            right_stick_y: 0,
            touchpad_click: false,
            contacts: [SonyContact::default(); CONTACTS_PER_SOURCE],
            observed_at_us,
        }
    }

    pub fn validate(self) -> Result<Self, SonySnapshotError> {
        if usize::from(self.slot) >= MAX_SOURCES {
            return Err(SonySnapshotError::SlotOutOfRange(self.slot));
        }
        if self.incarnation == 0 {
            return Err(SonySnapshotError::MissingIncarnation);
        }
        if self.incarnation > MAX_EXACT_INCARNATION {
            return Err(SonySnapshotError::IncarnationOutOfRange(self.incarnation));
        }
        Ok(self)
    }

    pub fn is_neutral(self) -> bool {
        let centered = |axis: i16| axis == 0;
        self.buttons == 0
            && self.left_trigger == 0
            && self.right_trigger == 0
            && centered(self.left_stick_x)
            && centered(self.left_stick_y)
            && centered(self.right_stick_x)
            && centered(self.right_stick_y)
            && !self.touchpad_click
            && self.contacts.iter().all(|contact| !contact.active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonySnapshotError {
    SlotOutOfRange(u8),
    MissingIncarnation,
    IncarnationOutOfRange(u64),
}

impl fmt::Display for SonySnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotOutOfRange(slot) => {
                write!(formatter, "sony snapshot slot {slot} is out of range")
            }
            Self::MissingIncarnation => {
                formatter.write_str("sony snapshot has no source incarnation")
            }
            Self::IncarnationOutOfRange(incarnation) => {
                write!(
                    formatter,
                    "sony snapshot incarnation {incarnation} exceeds the exactly representable range"
                )
            }
        }
    }
}

impl std::error::Error for SonySnapshotError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotAdmission {
    Admitted,
    AdmittedInactive,
    Unbound,
    Faulted,
    StaleSource,
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryOutcome {
    Replaced,
    DuplicateSlot,
    DuplicateIncarnation,
    MalformedEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryView {
    claims: [Option<SdlDeviceClaim>; MAX_SOURCES],
}

impl InventoryView {
    pub fn claims(&self) -> &[Option<SdlDeviceClaim>; MAX_SOURCES] {
        &self.claims
    }

    pub fn claim(&self, slot: u8) -> Option<SdlDeviceClaim> {
        self.claims.get(usize::from(slot)).copied().flatten()
    }

    pub fn incarnation(&self, slot: u8) -> Option<u64> {
        self.claim(slot).map(|claim| claim.incarnation)
    }

    pub fn replace(&mut self, claims: &[Option<SdlDeviceClaim>]) -> InventoryOutcome {
        if claims.len() > MAX_SOURCES {
            return InventoryOutcome::MalformedEntry;
        }
        let mut next = [None; MAX_SOURCES];
        let mut seen_incarnations: [u64; MAX_SOURCES] = [0; MAX_SOURCES];
        for (seen, claim) in claims.iter().flatten().enumerate().take(MAX_SOURCES) {
            if usize::from(claim.slot) >= MAX_SOURCES || claim.incarnation == 0 {
                return InventoryOutcome::MalformedEntry;
            }
            if next[usize::from(claim.slot)].is_some() {
                return InventoryOutcome::DuplicateSlot;
            }
            if seen_incarnations[..seen].contains(&claim.incarnation) {
                return InventoryOutcome::DuplicateIncarnation;
            }
            seen_incarnations[seen] = claim.incarnation;
            next[usize::from(claim.slot)] = Some(*claim);
        }
        self.claims = next;
        InventoryOutcome::Replaced
    }

    pub fn rich_slot_mask(&self) -> u16 {
        let mut mask = 0_u16;
        for (slot, claim) in self.claims.iter().enumerate() {
            if claim.is_some_and(|claim| claim.sony_product().is_some()) {
                mask |= 1 << slot;
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds4(slot: u8, incarnation: u64) -> Option<SdlDeviceClaim> {
        SdlDeviceClaim::new(slot, incarnation, SONY_VENDOR, 0x05c4)
    }

    fn xbox(slot: u8, incarnation: u64) -> Option<SdlDeviceClaim> {
        SdlDeviceClaim::new(slot, incarnation, 0x045e, 0x02ea)
    }

    #[test]
    fn claim_rejects_out_of_range_slot_and_missing_incarnation() {
        assert!(SdlDeviceClaim::new(4, 1, SONY_VENDOR, 0x05c4).is_none());
        assert!(SdlDeviceClaim::new(0, 0, SONY_VENDOR, 0x05c4).is_none());
        assert!(SdlDeviceClaim::new(3, 7, SONY_VENDOR, 0x05c4).is_some());
        assert!(SdlDeviceClaim::new(3, MAX_EXACT_INCARNATION, SONY_VENDOR, 0x05c4).is_some());
        assert!(SdlDeviceClaim::new(3, MAX_EXACT_INCARNATION + 1, SONY_VENDOR, 0x05c4).is_none());
        assert!(SdlDeviceClaim::new(3, u64::MAX, SONY_VENDOR, 0x05c4).is_none());
    }

    #[test]
    fn sony_product_classifies_ds4_and_ds5_only_for_the_sony_vendor() {
        let ds4 = SdlDeviceClaim::new(0, 1, SONY_VENDOR, 0x09cc).unwrap();
        assert_eq!(ds4.sony_product(), Some((SonyTarget::Ds4, false)));
        let ds5 = SdlDeviceClaim::new(0, 1, SONY_VENDOR, 0x0ce6).unwrap();
        assert_eq!(ds5.sony_product(), Some((SonyTarget::Ds4, true)));
        let edge = SdlDeviceClaim::new(0, 1, SONY_VENDOR, 0x0df2).unwrap();
        assert_eq!(edge.sony_product(), Some((SonyTarget::Ds4, true)));
        let other = SdlDeviceClaim::new(0, 1, SONY_VENDOR, 0x0eee).unwrap();
        assert_eq!(other.sony_product(), None);
        let clone = SdlDeviceClaim::new(0, 1, 0x045e, 0x0ce6).unwrap();
        assert_eq!(clone.sony_product(), None);
    }

    #[test]
    fn inventory_replacement_is_atomic_and_rejects_malformed_entries() {
        let mut inventory = InventoryView::default();
        let good = [ds4(0, 11), xbox(1, 12), None, ds4(3, 14)];
        assert_eq!(inventory.replace(&good), InventoryOutcome::Replaced);
        assert_eq!(inventory.incarnation(0), Some(11));
        assert_eq!(inventory.rich_slot_mask(), 0b1001);

        let mut out_of_range = [None; MAX_SOURCES];
        out_of_range[0] = Some(SdlDeviceClaim {
            slot: 255,
            incarnation: 21,
            vendor: SONY_VENDOR,
            product: 0x05c4,
        });
        assert_eq!(
            inventory.replace(&out_of_range),
            InventoryOutcome::MalformedEntry
        );
        assert_eq!(inventory.incarnation(0), Some(11));

        let mut duplicate_port = [None; MAX_SOURCES];
        duplicate_port[0] = ds4(0, 31);
        duplicate_port[1] = Some(SdlDeviceClaim {
            slot: 0,
            incarnation: 32,
            vendor: SONY_VENDOR,
            product: 0x05c4,
        });
        assert_eq!(
            inventory.replace(&duplicate_port),
            InventoryOutcome::DuplicateSlot
        );
        assert_eq!(inventory.incarnation(0), Some(11));

        let mut duplicate_incarnation = [None; MAX_SOURCES];
        duplicate_incarnation[0] = ds4(0, 41);
        duplicate_incarnation[1] = xbox(1, 41);
        assert_eq!(
            inventory.replace(&duplicate_incarnation),
            InventoryOutcome::DuplicateIncarnation
        );
        assert_eq!(inventory.incarnation(0), Some(11));

        let mut zero_incarnation = [None; MAX_SOURCES];
        zero_incarnation[0] = Some(SdlDeviceClaim {
            slot: 0,
            incarnation: 0,
            vendor: SONY_VENDOR,
            product: 0x05c4,
        });
        assert_eq!(
            inventory.replace(&zero_incarnation),
            InventoryOutcome::MalformedEntry
        );

        let oversized = [None; MAX_SOURCES + 1];
        assert_eq!(
            inventory.replace(&oversized),
            InventoryOutcome::MalformedEntry
        );
        assert_eq!(inventory.incarnation(0), Some(11));
        assert_eq!(inventory.incarnation(3), Some(14));
    }

    #[test]
    fn inventory_entries_may_arrive_in_any_order() {
        let mut inventory = InventoryView::default();
        let reordered = [None, ds4(1, 5), None, None];
        assert_eq!(inventory.replace(&reordered), InventoryOutcome::Replaced);
        assert_eq!(inventory.incarnation(1), Some(5));
        let shifted = [
            Some(SdlDeviceClaim {
                slot: 3,
                ..ds4(3, 6).unwrap()
            }),
            None,
            None,
            None,
        ];
        assert_eq!(inventory.replace(&shifted), InventoryOutcome::Replaced);
        assert_eq!(inventory.incarnation(3), Some(6));
        assert_eq!(inventory.incarnation(1), None);
    }

    #[test]
    fn snapshot_validation_rejects_bad_slot_and_missing_incarnation() {
        let snapshot = SonySnapshot::neutral(4, 1, 0);
        assert_eq!(
            snapshot.validate(),
            Err(SonySnapshotError::SlotOutOfRange(4))
        );
        let snapshot = SonySnapshot::neutral(2, 0, 0);
        assert_eq!(
            snapshot.validate(),
            Err(SonySnapshotError::MissingIncarnation)
        );
        assert!(SonySnapshot::neutral(2, 9, 10).validate().is_ok());
        assert!(
            SonySnapshot::neutral(2, MAX_EXACT_INCARNATION, 10)
                .validate()
                .is_ok()
        );
        assert_eq!(
            SonySnapshot::neutral(2, MAX_EXACT_INCARNATION + 1, 10).validate(),
            Err(SonySnapshotError::IncarnationOutOfRange(
                MAX_EXACT_INCARNATION + 1
            ))
        );
        assert_eq!(
            SonySnapshot::neutral(2, u64::MAX, 10).validate(),
            Err(SonySnapshotError::IncarnationOutOfRange(u64::MAX))
        );
    }

    #[test]
    fn neutral_snapshot_centers_axes_and_releases_contacts() {
        let snapshot = SonySnapshot::neutral(1, 3, 42);
        assert_eq!(snapshot.buttons, 0);
        assert_eq!(snapshot.left_stick_x, 0);
        assert_eq!(snapshot.observed_at_us, 42);
        assert!(snapshot.contacts.iter().all(|contact| !contact.active));
    }
}
