use std::mem;
use std::ptr::{self, NonNull};

use objc2_core_audio::{
    AudioDeviceID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectPropertyAddress, AudioObjectPropertyScope, AudioObjectPropertySelector,
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyStreams, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
};
use objc2_core_foundation::{CFRetained, CFString};

use crate::audio_device::{
    AUDIO_OUTPUT_DEVICE_PREFIX, AudioOutputDevice, MAX_AUDIO_OUTPUT_DEVICE_ID_BYTES,
    validate_audio_output_device,
};

use super::BackendError;
use super::audio::check_status;

const MAX_AUDIO_DEVICES: usize = 256;

pub fn audio_output_devices() -> Result<Vec<AudioOutputDevice>, BackendError> {
    output_device_ids()?
        .into_iter()
        .map(|device| {
            let id = format!(
                "{AUDIO_OUTPUT_DEVICE_PREFIX}{}",
                string_property(device, kAudioDevicePropertyDeviceUID)?
            );
            validate_audio_output_device(Some(&id))?;
            let name = string_property(device, kAudioObjectPropertyName)?;
            Ok(AudioOutputDevice { id, name })
        })
        .collect()
}

pub(super) fn resolve_output_device(id: &str) -> Result<AudioDeviceID, BackendError> {
    validate_audio_output_device(Some(id))?;
    let uid = &id[AUDIO_OUTPUT_DEVICE_PREFIX.len()..];
    for device in output_device_ids()? {
        if string_property(device, kAudioDevicePropertyDeviceUID)? == uid {
            return Ok(device);
        }
    }
    Err(BackendError::AudioOutputDeviceUnavailable(id.to_owned()))
}

fn output_device_ids() -> Result<Vec<AudioDeviceID>, BackendError> {
    let devices = object_ids(
        kAudioObjectSystemObject as u32,
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
    )?;
    let mut outputs = Vec::with_capacity(devices.len());
    for device in devices {
        if !object_ids(
            device,
            kAudioDevicePropertyStreams,
            kAudioObjectPropertyScopeOutput,
        )?
        .is_empty()
        {
            outputs.push(device);
        }
    }
    Ok(outputs)
}

fn object_ids(
    object: u32,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> Result<Vec<u32>, BackendError> {
    let mut address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut size = 0;
    check_status("AudioObjectGetPropertyDataSize", unsafe {
        AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(&mut address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
        )
    })?;
    if size as usize > MAX_AUDIO_DEVICES * mem::size_of::<u32>()
        || size as usize % mem::size_of::<u32>() != 0
    {
        return Err(BackendError::InvalidAudioDeviceData);
    }
    if size == 0 {
        return Ok(Vec::new());
    }
    let mut ids = vec![0u32; size as usize / mem::size_of::<u32>()];
    let capacity = size;
    check_status("AudioObjectGetPropertyData", unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(ids.as_mut_ptr()).unwrap().cast(),
        )
    })?;
    if size > capacity || size as usize % mem::size_of::<u32>() != 0 {
        return Err(BackendError::InvalidAudioDeviceData);
    }
    ids.truncate(size as usize / mem::size_of::<u32>());
    Ok(ids)
}

fn string_property(
    object: u32,
    selector: AudioObjectPropertySelector,
) -> Result<String, BackendError> {
    let mut address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value: *mut CFString = ptr::null_mut();
    let mut size = mem::size_of_val(&value) as u32;
    check_status("AudioObjectGetPropertyData(CFString)", unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    })?;
    let value = NonNull::new(value).ok_or(BackendError::InvalidAudioDeviceData)?;
    let value = unsafe { CFRetained::from_raw(value) };
    if size as usize != mem::size_of::<*mut CFString>()
        || value.length() > MAX_AUDIO_OUTPUT_DEVICE_ID_BYTES as isize
    {
        return Err(BackendError::InvalidAudioDeviceData);
    }
    let value = value.to_string();
    if value.is_empty() || value.len() > MAX_AUDIO_OUTPUT_DEVICE_ID_BYTES || value.contains('\0') {
        return Err(BackendError::InvalidAudioDeviceData);
    }
    Ok(value)
}
