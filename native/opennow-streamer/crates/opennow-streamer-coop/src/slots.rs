//! Host-side ownership of GFN controller slots 1..3 by co-op guests.
//!
//! Pure state: every transition returns the ordered actions the host applies through the
//! NVST input codec, which owns the registration bitmap and the wire framing.

use std::time::{Duration, Instant};

use crate::input::{GamepadState, InputRecord, is_newer_seq};

pub const MAX_GUESTS: usize = 3;
pub const INPUT_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotAction {
    /// Register exactly these guest slots (bit n = slot n).
    Slots(u8),
    Gamepad {
        slot: u8,
        state: GamepadState,
    },
    Neutral(u8),
}

/// Why a guest's input record did not become a slot action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDrop {
    /// The peer is not a member with a connected slot.
    SlotNotOwned,
    Muted,
    /// The sequence number is not newer than the last accepted one.
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    InvalidSlot,
    UnknownPeer,
    Full,
    DuplicatePeer,
}

#[derive(Debug, Clone)]
struct GuestSlot {
    peer: String,
    slot: u8,
    muted: bool,
    connected: bool,
    last_seq: Option<u32>,
    last_input_at: Option<Instant>,
    held: bool,
}

#[derive(Debug, Default)]
pub struct SlotTable {
    guests: Vec<GuestSlot>,
}

fn valid_slot(slot: u8) -> Result<u8, SlotError> {
    (1..=3)
        .contains(&slot)
        .then_some(slot)
        .ok_or(SlotError::InvalidSlot)
}

impl SlotTable {
    pub fn len(&self) -> usize {
        self.guests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.guests.is_empty()
    }

    /// Slots registered with GFN: connected guests only.
    pub fn mask(&self) -> u8 {
        self.guests
            .iter()
            .filter(|guest| guest.connected)
            .fold(0, |mask, guest| mask | (1 << guest.slot))
    }

    pub fn slot_of(&self, peer: &str) -> Option<u8> {
        self.find(peer).map(|guest| guest.slot)
    }

    pub fn is_muted(&self, peer: &str) -> bool {
        self.find(peer).is_some_and(|guest| guest.muted)
    }

    pub fn owner_of(&self, slot: u8) -> Option<&str> {
        self.guests
            .iter()
            .find(|guest| guest.slot == slot && guest.connected)
            .map(|guest| guest.peer.as_str())
    }

    fn find(&self, peer: &str) -> Option<&GuestSlot> {
        self.guests.iter().find(|guest| guest.peer == peer)
    }

    fn find_mut(&mut self, peer: &str) -> Result<&mut GuestSlot, SlotError> {
        self.guests
            .iter_mut()
            .find(|guest| guest.peer == peer)
            .ok_or(SlotError::UnknownPeer)
    }

    /// Adds a guest that is not yet connected; a taken slot is swapped as in the backend.
    pub fn add(&mut self, peer: &str, slot: u8) -> Result<Vec<SlotAction>, SlotError> {
        let slot = valid_slot(slot)?;
        if self.find(peer).is_some() {
            return Err(SlotError::DuplicatePeer);
        }
        if self.guests.len() == MAX_GUESTS {
            return Err(SlotError::Full);
        }
        let mut actions = Vec::new();
        if let Some(index) = self.guests.iter().position(|guest| guest.slot == slot) {
            let free = (1..=3)
                .find(|candidate| self.guests.iter().all(|guest| guest.slot != *candidate))
                .ok_or(SlotError::Full)?;
            let displaced = &mut self.guests[index];
            if displaced.connected {
                actions.push(SlotAction::Neutral(displaced.slot));
            }
            displaced.slot = free;
            displaced.held = false;
            actions.push(SlotAction::Slots(self.mask()));
        }
        self.guests.push(GuestSlot {
            peer: peer.to_owned(),
            slot,
            muted: false,
            connected: false,
            last_seq: None,
            last_input_at: None,
            held: false,
        });
        Ok(actions)
    }

    /// The guest's link is up: its slot appears to the game.
    pub fn connected(&mut self, peer: &str, now: Instant) -> Result<Vec<SlotAction>, SlotError> {
        let guest = self.find_mut(peer)?;
        if guest.connected {
            return Ok(Vec::new());
        }
        guest.connected = true;
        guest.last_input_at = Some(now);
        guest.held = false;
        Ok(vec![SlotAction::Slots(self.mask())])
    }

    /// The link dropped but the guest may reconnect: keep the slot, release its buttons.
    pub fn disconnected(&mut self, peer: &str) -> Result<Vec<SlotAction>, SlotError> {
        let guest = self.find_mut(peer)?;
        if !guest.connected {
            return Ok(Vec::new());
        }
        guest.held = false;
        Ok(vec![SlotAction::Neutral(guest.slot)])
    }

    pub fn input(
        &mut self,
        peer: &str,
        record: InputRecord,
        now: Instant,
    ) -> Result<SlotAction, InputDrop> {
        let guest = self
            .find_mut(peer)
            .ok()
            .filter(|guest| guest.connected)
            .ok_or(InputDrop::SlotNotOwned)?;
        if guest.muted {
            return Err(InputDrop::Muted);
        }
        if guest
            .last_seq
            .is_some_and(|latest| !is_newer_seq(record.seq, latest))
        {
            return Err(InputDrop::Stale);
        }
        guest.last_seq = Some(record.seq);
        guest.last_input_at = Some(now);
        guest.held = !record.state.is_neutral();
        Ok(SlotAction::Gamepad {
            slot: guest.slot,
            state: record.state,
        })
    }

    pub fn set_muted(&mut self, peer: &str, muted: bool) -> Result<Vec<SlotAction>, SlotError> {
        let guest = self.find_mut(peer)?;
        if guest.muted == muted {
            return Ok(Vec::new());
        }
        guest.muted = muted;
        guest.held = false;
        Ok(if muted && guest.connected {
            vec![SlotAction::Neutral(guest.slot)]
        } else {
            Vec::new()
        })
    }

    /// Moves a guest; the old slot is released with a neutral state before re-registration.
    pub fn move_to(&mut self, peer: &str, slot: u8) -> Result<Vec<SlotAction>, SlotError> {
        let slot = valid_slot(slot)?;
        let index = self
            .guests
            .iter()
            .position(|guest| guest.peer == peer)
            .ok_or(SlotError::UnknownPeer)?;
        let old = self.guests[index].slot;
        if old == slot {
            return Ok(Vec::new());
        }
        let mut actions = Vec::new();
        if self.guests[index].connected {
            actions.push(SlotAction::Neutral(old));
        }
        if let Some(other) = self.guests.iter_mut().find(|guest| guest.slot == slot) {
            if other.connected {
                actions.push(SlotAction::Neutral(slot));
            }
            other.slot = old;
            other.held = false;
        }
        let guest = &mut self.guests[index];
        guest.slot = slot;
        guest.held = false;
        actions.push(SlotAction::Slots(self.mask()));
        Ok(actions)
    }

    pub fn remove(&mut self, peer: &str) -> Result<Vec<SlotAction>, SlotError> {
        let index = self
            .guests
            .iter()
            .position(|guest| guest.peer == peer)
            .ok_or(SlotError::UnknownPeer)?;
        let guest = self.guests.remove(index);
        Ok(if guest.connected {
            vec![
                SlotAction::Neutral(guest.slot),
                SlotAction::Slots(self.mask()),
            ]
        } else {
            Vec::new()
        })
    }

    /// Releases slots whose guest stopped sending for 'INPUT_TIMEOUT' while holding input.
    pub fn poll(&mut self, now: Instant) -> Vec<SlotAction> {
        let mut actions = Vec::new();
        for guest in &mut self.guests {
            if guest.connected
                && guest.held
                && guest
                    .last_input_at
                    .is_some_and(|last| now.saturating_duration_since(last) >= INPUT_TIMEOUT)
            {
                guest.held = false;
                actions.push(SlotAction::Neutral(guest.slot));
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seq: u32, buttons: u16) -> InputRecord {
        InputRecord {
            seq,
            state: GamepadState {
                bitmap: 1,
                buttons,
                ..GamepadState::default()
            },
            ts_ms: 0,
        }
    }

    fn pressed(slot: u8, buttons: u16) -> SlotAction {
        SlotAction::Gamepad {
            slot,
            state: record(0, buttons).state,
        }
    }

    #[test]
    fn join_registers_only_after_the_link_connects() {
        let now = Instant::now();
        let mut table = SlotTable::default();
        assert_eq!(table.add("a", 1).unwrap(), []);
        assert_eq!(table.mask(), 0);
        assert_eq!(
            table.input("a", record(1, 1), now),
            Err(InputDrop::SlotNotOwned)
        );
        assert_eq!(
            table.connected("a", now).unwrap(),
            [SlotAction::Slots(0b0010)]
        );
        assert_eq!(table.input("a", record(1, 1), now), Ok(pressed(1, 1)));
        assert_eq!(table.owner_of(1), Some("a"));
        assert_eq!(table.add("a", 2), Err(SlotError::DuplicatePeer));
        assert_eq!(table.add("b", 0), Err(SlotError::InvalidSlot));
    }

    #[test]
    fn stale_and_duplicate_sequences_are_dropped() {
        let now = Instant::now();
        let mut table = SlotTable::default();
        table.add("a", 2).unwrap();
        table.connected("a", now).unwrap();
        assert!(table.input("a", record(u32::MAX, 4), now).is_ok());
        assert_eq!(table.input("a", record(0, 5), now), Ok(pressed(2, 5)));
        assert_eq!(table.input("a", record(0, 6), now), Err(InputDrop::Stale));
        assert_eq!(
            table.input("a", record(u32::MAX - 3, 7), now),
            Err(InputDrop::Stale)
        );
    }

    #[test]
    fn mute_drops_input_and_releases_the_slot() {
        let now = Instant::now();
        let mut table = SlotTable::default();
        table.add("a", 3).unwrap();
        table.connected("a", now).unwrap();
        table.input("a", record(1, 8), now).unwrap();
        assert_eq!(
            table.set_muted("a", true).unwrap(),
            [SlotAction::Neutral(3)]
        );
        assert_eq!(table.input("a", record(2, 8), now), Err(InputDrop::Muted));
        assert_eq!(
            table.mask(),
            0b1000,
            "a muted guest keeps its controller registered"
        );
        assert_eq!(table.set_muted("a", false).unwrap(), []);
        assert_eq!(table.input("a", record(3, 8), now), Ok(pressed(3, 8)));
    }

    #[test]
    fn slot_move_and_swap_release_old_slots_first() {
        let now = Instant::now();
        let mut table = SlotTable::default();
        table.add("a", 1).unwrap();
        table.add("b", 2).unwrap();
        table.connected("a", now).unwrap();
        table.connected("b", now).unwrap();
        assert_eq!(
            table.move_to("a", 3).unwrap(),
            [SlotAction::Neutral(1), SlotAction::Slots(0b1100)]
        );
        assert_eq!(
            table.move_to("a", 2).unwrap(),
            [
                SlotAction::Neutral(3),
                SlotAction::Neutral(2),
                SlotAction::Slots(0b1100)
            ]
        );
        assert_eq!((table.slot_of("a"), table.slot_of("b")), (Some(2), Some(3)));
        assert_eq!(table.input("b", record(1, 1), now), Ok(pressed(3, 1)));
    }

    #[test]
    fn leave_and_disconnect_release_with_neutral_state() {
        let now = Instant::now();
        let mut table = SlotTable::default();
        table.add("a", 1).unwrap();
        table.add("b", 2).unwrap();
        table.connected("a", now).unwrap();
        table.connected("b", now).unwrap();
        assert_eq!(table.disconnected("b").unwrap(), [SlotAction::Neutral(2)]);
        assert_eq!(
            table.input("z", record(1, 1), now),
            Err(InputDrop::SlotNotOwned)
        );
        assert_eq!(table.mask(), 0b0110);
        assert_eq!(
            table.remove("a").unwrap(),
            [SlotAction::Neutral(1), SlotAction::Slots(0b0100)]
        );
        assert_eq!(table.owner_of(1), None);
        assert_eq!(table.remove("a"), Err(SlotError::UnknownPeer));
    }

    #[test]
    fn input_timeout_releases_held_buttons_once() {
        let start = Instant::now();
        let mut table = SlotTable::default();
        table.add("a", 1).unwrap();
        table.connected("a", start).unwrap();
        table.input("a", record(1, 0x10), start).unwrap();
        assert!(table.poll(start + Duration::from_millis(499)).is_empty());
        assert_eq!(table.poll(start + INPUT_TIMEOUT), [SlotAction::Neutral(1)]);
        assert!(table.poll(start + Duration::from_secs(2)).is_empty());
        table
            .input("a", record(2, 0), start + Duration::from_secs(2))
            .unwrap();
        assert!(
            table.poll(start + Duration::from_secs(5)).is_empty(),
            "neutral input is not held"
        );
    }

    #[test]
    fn table_is_bounded_to_three_guests() {
        let mut table = SlotTable::default();
        table.add("a", 1).unwrap();
        table.add("b", 2).unwrap();
        table.add("c", 3).unwrap();
        assert_eq!(table.add("d", 1), Err(SlotError::Full));
    }
}
