use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Component, Path, PathBuf};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, LocalFree};
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, FILE_PERSISTENT_ACLS, MAXIMUM_ALLOWED,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(text: &str) -> io::Result<Self> {
        let text: Vec<_> = text.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

pub(super) fn ensure(path: &Path) -> io::Result<()> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let path = std::path::absolute(path)?;
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut size = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut size,
        );
    }
    if size == 0 || size > 65536 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut user = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            user.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let owner = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
    unsafe { LocalFree(text.cast()) };
    let descriptor = Descriptor::from_sddl(&format!("O:{owner}D:P(A;OICI;FA;;;OW)"))?;
    let mut ancestors = Vec::new();
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let leaf = current == path;
        let access = FILE_READ_ATTRIBUTES | READ_CONTROL;
        let (mut directory, created) = match open_directory(&current, access) {
            Ok(directory) => (directory, false),
            Err(failure) if failure.kind() == io::ErrorKind::NotFound => {
                let parent = ancestors.last().ok_or(io::ErrorKind::InvalidInput)?;
                check_volume(parent)?;
                match create_directory(&current, &descriptor) {
                    Ok(()) => (),
                    Err(failure) if failure.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32) => {
                    }
                    Err(failure) => return Err(failure),
                }
                (open_directory(&current, access)?, true)
            }
            Err(failure) => return Err(failure),
        };
        check_directory(&directory)?;
        if leaf || created {
            check_volume(&directory)?;
            if !has_private_acl(&directory, sid)? {
                drop(directory);
                directory = open_directory(&current, MAXIMUM_ALLOWED)?;
                harden_directory(&directory, sid, &descriptor)?;
            }
        }
        ancestors.push(directory);
    }
    Ok(())
}

fn open_directory(path: &Path, access: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access | FILE_LIST_DIRECTORY)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

fn create_directory(path: &Path, descriptor: &Descriptor) -> io::Result<()> {
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if wide[..wide.len() - 1].contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn check_volume(directory: &File) -> io::Result<()> {
    let mut flags = 0;
    if unsafe {
        GetVolumeInformationByHandleW(
            directory.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut flags,
            std::ptr::null_mut(),
            0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    require_persistent_acls(flags)
}

fn require_persistent_acls(flags: u32) -> io::Result<()> {
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err(io::ErrorKind::Unsupported.into());
    }
    Ok(())
}

fn check_directory(directory: &File) -> io::Result<()> {
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(directory.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

fn has_private_acl(directory: &File, user: PSID) -> io::Result<bool> {
    check_directory(directory)?;
    let mut owner = std::ptr::null_mut();
    let mut acl = std::ptr::null_mut();
    let mut existing = std::ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            directory.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut existing,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let existing = Descriptor(existing);
    if owner.is_null() || unsafe { EqualSid(owner, user) } == 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(existing.0, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if acl.is_null()
        || control & SE_DACL_PROTECTED == 0
        || unsafe { IsValidAcl(acl) } == 0
        || unsafe { (*acl).AceCount } != 1
    {
        return Ok(false);
    }
    let mut ace = std::ptr::null_mut();
    if unsafe { GetAce(acl, 0, &mut ace) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let header = unsafe { &*ace.cast::<ACE_HEADER>() };
    if u32::from(header.AceType) != ACCESS_ALLOWED_ACE_TYPE
        || u32::from(header.AceFlags) & !INHERITED_ACE != OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
    {
        return Ok(false);
    }
    let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
    let trustee = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
    Ok(unsafe {
        allowed.Mask == FILE_ALL_ACCESS
            && IsValidSid(trustee) != 0
            && (EqualSid(trustee, user) != 0
                || IsWellKnownSid(trustee, WinCreatorOwnerRightsSid) != 0)
    })
}

fn descriptor_acl(descriptor: &Descriptor) -> io::Result<*mut ACL> {
    let mut dacl = std::ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
        == 0
        || present == 0
        || dacl.is_null()
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(dacl)
}

fn harden_directory(directory: &File, user: PSID, descriptor: &Descriptor) -> io::Result<()> {
    if has_private_acl(directory, user)? {
        return Ok(());
    }
    let result = unsafe {
        SetSecurityInfo(
            directory.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            descriptor_acl(descriptor)?,
            std::ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    Ok(())
}

#[cfg(test)]
#[path = "private_directory_windows_tests.rs"]
mod tests;
