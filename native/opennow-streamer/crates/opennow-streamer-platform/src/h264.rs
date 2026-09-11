use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct H264ParameterSets {
    units: BTreeMap<(u8, u8), Vec<u8>>,
}

impl H264ParameterSets {
    pub fn retain(&mut self, access_unit: &[u8]) -> Result<(), String> {
        let mut updates = BTreeMap::new();
        for unit in openh264::nal_units(access_unit) {
            let Some(&header) = unit.get(3) else {
                continue;
            };
            let kind = header & 0x1f;
            if !matches!(kind, 7 | 8) {
                continue;
            }
            if unit.len() > 64 * 1024 {
                return Err("H.264 parameter set exceeds 64 KiB".to_owned());
            }
            let id = parameter_set_id(kind, &unit[4..])
                .ok_or_else(|| "invalid H.264 parameter-set identifier".to_owned())?;
            updates.insert((kind, id), unit);
        }
        for (key, unit) in updates {
            if self.units.get(&key).is_none_or(|cached| cached != unit) {
                self.units.insert(key, unit.to_vec());
            }
        }
        Ok(())
    }

    pub fn replay(&self) -> Vec<u8> {
        self.units.values().flatten().copied().collect()
    }
}

fn parameter_set_id(kind: u8, bytes: &[u8]) -> Option<u8> {
    let mut zeroes = 0;
    let rbsp = bytes.iter().copied().filter(|&byte| {
        if zeroes == 2 && byte == 3 {
            zeroes = 0;
            return false;
        }
        zeroes = if byte == 0 { (zeroes + 1).min(2) } else { 0 };
        true
    });
    let mut bits = rbsp
        .flat_map(|byte| (0..8).rev().map(move |shift| (byte >> shift) & 1))
        .skip(if kind == 7 { 24 } else { 0 });
    let mut leading_zeroes = 0;
    while bits.next()? == 0 {
        leading_zeroes += 1;
        if leading_zeroes > 8 {
            return None;
        }
    }
    let mut value = 1_u16;
    for _ in 0..leading_zeroes {
        value = (value << 1) | u16::from(bits.next()?);
    }
    let id = u8::try_from(value - 1).ok()?;
    (kind != 7 || id < 32).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_sets_replace_only_matching_ids_and_replay_sequence_sets_first() {
        let mut sets = H264ParameterSets::default();
        sets.retain(&[0, 0, 1, 0x68, 0x80]).unwrap();
        sets.retain(&[0, 0, 1, 0x67, 66, 0, 30, 0x80]).unwrap();
        sets.retain(&[0, 0, 1, 0x68, 0x40]).unwrap();
        sets.retain(&[0, 0, 1, 0x68, 0xa0]).unwrap();
        assert_eq!(sets.units.len(), 3);
        assert_eq!(
            sets.replay(),
            [
                0, 0, 1, 0x67, 66, 0, 30, 0x80, 0, 0, 1, 0x68, 0xa0, 0, 0, 1, 0x68, 0x40
            ]
        );
    }

    #[test]
    fn invalid_parameter_set_does_not_replace_cached_configuration() {
        let mut sets = H264ParameterSets::default();
        sets.retain(&[0, 0, 1, 0x68, 0x80]).unwrap();
        let before = sets.replay();
        assert!(sets.retain(&[0, 0, 1, 0x68, 0xa0, 0, 0, 1, 0x67]).is_err());
        assert_eq!(sets.replay(), before);
        assert!(sets.retain(&[0, 0, 1, 0x68, 0, 0, 0]).is_err());
        assert_eq!(sets.replay(), before);
    }

    #[test]
    fn parameter_set_ids_are_bounded_and_skip_emulation_prevention() {
        assert_eq!(parameter_set_id(7, &[66, 0, 30, 0x80]), Some(0));
        assert_eq!(parameter_set_id(7, &[0, 0, 3, 0, 0x80]), Some(0));
        assert_eq!(parameter_set_id(7, &[66, 0, 30, 0x04, 0x00]), Some(31));
        assert_eq!(parameter_set_id(7, &[66, 0, 30, 0x04, 0x20]), None);
        assert_eq!(parameter_set_id(8, &[0, 0x80, 0]), Some(255));
        assert_eq!(parameter_set_id(8, &[0, 0x80, 0x80]), None);
    }

    #[test]
    fn oversized_parameter_set_does_not_change_the_cache() {
        let mut sets = H264ParameterSets::default();
        sets.retain(&[0, 0, 1, 0x68, 0x80]).unwrap();
        let before = sets.replay();
        let mut oversized = vec![0xff; 64 * 1024 + 1];
        oversized[..5].copy_from_slice(&[0, 0, 1, 0x68, 0x80]);
        assert!(sets.retain(&oversized).is_err());
        assert_eq!(sets.replay(), before);
    }

    #[test]
    fn mixed_annex_b_prefixes_retain_the_same_parameter_set_ids() {
        let mut sets = H264ParameterSets::default();
        sets.retain(&[0, 0, 0, 1, 0x68, 0x80, 0, 0, 1, 0x67, 66, 0, 30, 0x80])
            .unwrap();
        let expected = [0, 0, 1, 0x67, 66, 0, 30, 0x80, 0, 0, 1, 0x68, 0x80];
        assert_eq!(sets.replay(), expected);
        sets.retain(&[0, 0, 0, 1, 0x67, 66, 0, 30, 0x80]).unwrap();
        sets.retain(&[0, 0, 1, 0x68, 0x80]).unwrap();
        assert_eq!(sets.replay(), expected);
        sets.retain(&[0, 0, 1, 0x67, 66, 0, 30, 0x80, 0, 0, 0, 1, 0x68, 0x80])
            .unwrap();
        assert_eq!(sets.units.len(), 2);
        let replay = sets.replay();
        let kinds: Vec<u8> = openh264::nal_units(&replay)
            .map(|unit| unit[3] & 0x1f)
            .collect();
        assert_eq!(kinds, [7, 8]);
    }
}
