use crate::format::FormatError;

pub(crate) const AUDIO_OUTPUT_DEVICE_PREFIX: &str = "coreaudio:";
pub(crate) const MAX_AUDIO_OUTPUT_DEVICE_ID_BYTES: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioOutputDevice {
    pub id: String,
    pub name: String,
}

pub(crate) fn validate_audio_output_device(id: Option<&str>) -> Result<(), FormatError> {
    let Some(id) = id else {
        return Ok(());
    };
    if id.len() > MAX_AUDIO_OUTPUT_DEVICE_ID_BYTES
        || id.contains('\0')
        || !id
            .strip_prefix(AUDIO_OUTPUT_DEVICE_PREFIX)
            .is_some_and(|uid| !uid.is_empty())
    {
        return Err(FormatError::InvalidAudioOutputDevice);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_default_and_bounded_coreaudio_uids() {
        assert_eq!(validate_audio_output_device(None), Ok(()));
        assert_eq!(
            validate_audio_output_device(Some("coreaudio:BuiltInSpeakerDevice")),
            Ok(())
        );
        let id = format!("coreaudio:{}", "x".repeat(1014));
        assert_eq!(validate_audio_output_device(Some(&id)), Ok(()));
        assert_eq!(
            validate_audio_output_device(Some(&format!("{id}x"))),
            Err(FormatError::InvalidAudioOutputDevice)
        );
    }

    #[test]
    fn rejects_empty_foreign_and_nul_device_ids() {
        for id in [
            "",
            "coreaudio:",
            "BuiltInSpeakerDevice",
            "sdl:1",
            "coreaudio:a\0b",
        ] {
            assert_eq!(
                validate_audio_output_device(Some(id)),
                Err(FormatError::InvalidAudioOutputDevice)
            );
        }
        let id = format!("coreaudio:{}", "é".repeat(508));
        assert_eq!(
            validate_audio_output_device(Some(&id)),
            Err(FormatError::InvalidAudioOutputDevice)
        );
    }
}
