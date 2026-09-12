use std::path::Path;

pub(super) fn create_private_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(path).map_err(|error| error.to_string())
    }
    #[cfg(windows)]
    {
        windows::create_private(path)
    }
}

pub(super) fn preserve_permissions(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
        if metadata.is_symlink() {
            return Err("Cannot preserve permissions through a user-data symlink".to_owned());
        }
        std::os::unix::fs::chown(destination, Some(metadata.uid()), Some(metadata.gid()))
            .map_err(|error| format!("Cannot preserve user-data ownership: {error}"))?;
        std::fs::set_permissions(destination, metadata.permissions())
            .map_err(|error| format!("Cannot preserve user-data permissions: {error}"))?;
        preserve_unix_acl(source, destination)
    }
    #[cfg(windows)]
    {
        windows::preserve(source, destination)
    }
}

#[cfg(target_os = "linux")]
fn preserve_unix_acl(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let source =
        std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .map_err(|error| error.to_string())?;
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        let size =
            unsafe { libc::getxattr(source.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        if size < 0 {
            let error = std::io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) {
                return Err(format!("Cannot inspect user-data ACL: {error}"));
            }
            if unsafe { libc::removexattr(destination.as_ptr(), name.as_ptr()) } < 0 {
                let error = std::io::Error::last_os_error();
                if !matches!(error.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) {
                    return Err(format!("Cannot remove inherited user-data ACL: {error}"));
                }
            }
            continue;
        }
        if size > 64 * 1024 {
            return Err("User-data ACL exceeds its size limit".to_owned());
        }
        let mut acl = vec![0u8; size as usize];
        let read = unsafe {
            libc::getxattr(
                source.as_ptr(),
                name.as_ptr(),
                acl.as_mut_ptr().cast(),
                acl.len(),
            )
        };
        if read != size {
            return Err("User-data ACL changed while it was being copied".to_owned());
        }
        if unsafe {
            libc::setxattr(
                destination.as_ptr(),
                name.as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        } < 0
        {
            return Err(format!(
                "Cannot preserve user-data ACL: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn preserve_unix_acl(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn copyfile(
            source: *const libc::c_char,
            destination: *const libc::c_char,
            state: *mut libc::c_void,
            flags: u32,
        ) -> i32;
    }
    let source =
        std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .map_err(|error| error.to_string())?;
    if unsafe {
        copyfile(
            source.as_ptr(),
            destination.as_ptr(),
            std::ptr::null_mut(),
            1,
        )
    } != 0
    {
        return Err(format!(
            "Cannot preserve user-data ACL: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::*;
    use windows_sys::Win32::Security::*;

    struct Descriptor(PSECURITY_DESCRIPTOR);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub(super) fn create_private(path: &Path) -> Result<(), String> {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, GetVolumeInformationByHandleW,
        };
        let parent = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(
                path.parent()
                    .ok_or("Private update directory has no parent")?,
            )
            .map_err(|error| error.to_string())?;
        let mut flags = 0;
        if unsafe {
            GetVolumeInformationByHandleW(
                parent.as_raw_handle(),
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
            return Err(format!(
                "Cannot verify update volume ACL support: {}",
                std::io::Error::last_os_error()
            ));
        }
        require_persistent_acls(flags)?;
        let text: Vec<_> = "D:P(A;OICI;FA;;;OW)"
            .encode_utf16()
            .chain(Some(0))
            .collect();
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
            return Err(std::io::Error::last_os_error().to_string());
        }
        let descriptor = Descriptor(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::CreateDirectoryW(
                wide(path).as_ptr(),
                &attributes,
            )
        } == 0
        {
            return Err(format!(
                "Cannot create private update directory: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn require_persistent_acls(flags: u32) -> Result<(), String> {
        if flags & windows_sys::Win32::System::SystemServices::FILE_PERSISTENT_ACLS == 0 {
            return Err("Update staging requires a filesystem with persistent ACLs; FAT and exFAT cannot protect the transaction".to_owned());
        }
        Ok(())
    }

    pub(super) fn preserve(source: &Path, destination: &Path) -> Result<(), String> {
        let mut owner = std::ptr::null_mut();
        let mut group = std::ptr::null_mut();
        let mut acl = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        let info =
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let code = unsafe {
            GetNamedSecurityInfoW(
                wide(source).as_ptr(),
                SE_FILE_OBJECT,
                info,
                &mut owner,
                &mut group,
                &mut acl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if code != 0 {
            return Err(format!(
                "Cannot inspect user-data security descriptor: Windows error {code}"
            ));
        }
        let _descriptor = Descriptor(descriptor);
        let code = unsafe {
            SetNamedSecurityInfoW(
                wide(destination).as_ptr(),
                SE_FILE_OBJECT,
                info | PROTECTED_DACL_SECURITY_INFORMATION,
                owner,
                group,
                acl,
                std::ptr::null_mut(),
            )
        };
        if code != 0 {
            return Err(format!(
                "Cannot preserve the original effective user-data DACL and ownership: Windows error {code}"
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn private_staging_requires_persistent_acl_support() {
            assert!(require_persistent_acls(0).is_err());
            assert!(
                require_persistent_acls(
                    !windows_sys::Win32::System::SystemServices::FILE_PERSISTENT_ACLS
                )
                .is_err()
            );
            assert!(
                require_persistent_acls(
                    windows_sys::Win32::System::SystemServices::FILE_PERSISTENT_ACLS
                )
                .is_ok()
            );
        }

        fn dacl(path: &Path) -> (Vec<Vec<u8>>, bool) {
            let mut acl = std::ptr::null_mut();
            let mut descriptor = std::ptr::null_mut();
            assert_eq!(
                unsafe {
                    GetNamedSecurityInfoW(
                        wide(path).as_ptr(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut acl,
                        std::ptr::null_mut(),
                        &mut descriptor,
                    )
                },
                0
            );
            let descriptor = Descriptor(descriptor);
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) },
                0
            );
            assert!(!acl.is_null());
            let mut entries = Vec::new();
            for index in 0..unsafe { (*acl).AceCount } {
                let mut ace = std::ptr::null_mut();
                assert_ne!(unsafe { GetAce(acl, index as u32, &mut ace) }, 0);
                let header = ace.cast::<ACE_HEADER>();
                let mut bytes = unsafe {
                    std::slice::from_raw_parts(ace.cast::<u8>(), (*header).AceSize as usize)
                }
                .to_vec();
                bytes[1] &= !(INHERITED_ACE as u8);
                entries.push(bytes);
            }
            (entries, control & SE_DACL_PROTECTED != 0)
        }

        #[test]
        fn private_transactions_and_preserved_profiles_keep_effective_dacls() {
            let directory = tempfile::TempDir::new().unwrap();
            let profile = directory.path().join("profile");
            create_private(&profile).unwrap();
            std::fs::write(profile.join("account.json"), b"private fixture data").unwrap();
            let root_before = dacl(&profile);
            let file_before = dacl(&profile.join("account.json"));
            assert!(root_before.1);
            let destination = directory.path().join("preserved");
            crate::update_apply::copy_user_data(
                &profile,
                &destination,
                &mut crate::update_apply::bundle::CopyBudget::new(),
            )
            .unwrap();
            let root_after = dacl(&destination);
            let file_after = dacl(&destination.join("account.json"));
            assert_eq!(root_before.0, root_after.0);
            assert_eq!(file_before.0, file_after.0);
            assert!(root_after.1);
            assert!(file_after.1);
        }
    }
}
