use super::*;

fn security(path: &Path) -> (String, bool) {
    let file = open_directory(path, READ_CONTROL).unwrap();
    let mut descriptor = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        },
        0
    );
    let descriptor = Descriptor(descriptor);
    let mut text = std::ptr::null_mut();
    assert_ne!(
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor.0,
                1,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut text,
                std::ptr::null_mut(),
            )
        },
        0
    );
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let sddl = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).unwrap();
    unsafe { LocalFree(text.cast()) };
    let mut control = 0;
    let mut revision = 0;
    assert_ne!(
        unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) },
        0
    );
    (sddl, control & SE_DACL_PROTECTED != 0)
}

fn permissive_parent(path: &Path) {
    let descriptor = Descriptor::from_sddl("D:P(A;OICI;FA;;;OW)(A;OICI;GR;;;WD)").unwrap();
    create_directory(path, &descriptor).unwrap();
}

fn assert_private(path: &Path, protected: bool) {
    let (sddl, actual_protected) = security(path);
    assert_eq!(actual_protected, protected, "{sddl}");
    let dacl = sddl.split("D:").nth(1).unwrap();
    assert_eq!(dacl.matches('(').count(), 1, "{sddl}");
    assert!(dacl.contains(";FA;;;OW)"), "{sddl}");
}

#[test]
fn new_private_directory_blocks_inherited_parent_read_access() {
    let temp = tempfile::TempDir::new().unwrap();
    let parent = temp.path().join("profile");
    permissive_parent(&parent);
    let before = security(&parent);
    let path = parent.join("plugins").join("data").join("provider");
    super::super::private_dir(&path).unwrap();
    assert_private(&path, true);
    std::fs::create_dir(path.join("child")).unwrap();
    std::fs::write(path.join("child").join("secret"), b"fixture").unwrap();
    assert_private(&path.join("child"), false);
    assert_private(&path.join("child").join("secret"), false);
    assert_eq!(security(&parent), before);
}

#[test]
fn existing_private_directory_is_hardened_without_touching_contents_or_parent() {
    let temp = tempfile::TempDir::new().unwrap();
    let parent = temp.path().join("profile");
    permissive_parent(&parent);
    let path = parent.join("provider");
    std::fs::create_dir(&path).unwrap();
    assert!(security(&path).0.contains(";;;WD)"));
    let outside = parent.join("outside");
    std::fs::write(&outside, b"fixture").unwrap();
    std::fs::hard_link(&outside, path.join("linked-file")).unwrap();
    let before = security(&outside);
    let parent_before = security(&parent);
    super::super::private_dir(&path).unwrap();
    assert_private(&path, true);
    super::super::private_dir(&path).unwrap();
    assert_private(&path, true);
    assert_eq!(security(&outside), before);
    assert_eq!(security(&parent), parent_before);
    std::fs::write(path.join("new-secret"), b"fixture").unwrap();
    assert_private(&path.join("new-secret"), false);
}

#[test]
fn private_directory_rejects_files_and_hard_links() {
    let temp = tempfile::TempDir::new().unwrap();
    let outside = temp.path().join("outside");
    std::fs::write(&outside, b"fixture").unwrap();
    let linked = temp.path().join("linked");
    std::fs::hard_link(&outside, &linked).unwrap();
    let before = security(&outside);
    assert!(super::super::private_dir(&outside).is_err());
    assert!(super::super::private_dir(&linked).is_err());
    assert_eq!(security(&outside), before);
}

#[test]
fn private_directory_rejects_junctions_including_ancestors() {
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let before = security(&target);
    let link = temp.path().join("junction");
    assert!(
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .status()
            .unwrap()
            .success()
    );
    assert!(super::super::private_dir(&link).is_err());
    assert!(super::super::private_dir(&link.join("child")).is_err());
    assert!(!target.join("child").exists());
    assert_eq!(security(&target), before);
    std::fs::remove_dir(&link).unwrap();
}

#[test]
fn private_directory_rejects_an_owner_mismatch_before_changing_the_dacl() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("provider");
    super::super::private_dir(&path).unwrap();
    let before = security(&path);
    let file = open_directory(&path, READ_CONTROL | WRITE_DAC).unwrap();
    let system = Descriptor::from_sddl("O:SYD:P(A;OICI;FA;;;SY)").unwrap();
    let mut owner = std::ptr::null_mut();
    let mut defaulted = 0;
    assert_ne!(
        unsafe { GetSecurityDescriptorOwner(system.0, &mut owner, &mut defaulted) },
        0
    );
    assert!(harden_directory(&file, owner, &system).is_err());
    assert_eq!(security(&path), before);
}

#[test]
fn private_directory_handles_pin_the_directory_against_replacement() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("provider");
    super::super::private_dir(&path).unwrap();
    let file = open_directory(&path, READ_CONTROL | WRITE_DAC).unwrap();
    assert!(std::fs::rename(&path, temp.path().join("moved")).is_err());
    assert!(std::fs::remove_dir(&path).is_err());
    drop(file);
    std::fs::rename(&path, temp.path().join("moved")).unwrap();
}

#[test]
fn private_directory_can_be_checked_while_a_provider_holds_its_working_directory() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("provider");
    super::super::private_dir(&path).unwrap();
    let _working_directory = OpenOptions::new()
        .access_mode(FILE_TRAVERSE | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&path)
        .unwrap();
    super::super::private_dir(&path).unwrap();
    assert_private(&path, true);
}

#[test]
fn private_directory_preserves_explicit_user_acl_while_provider_holds_working_directory() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("provider");
    super::super::private_dir(&path).unwrap();
    let before = security(&path).0;
    let owner = before
        .strip_prefix("O:")
        .unwrap()
        .split("D:")
        .next()
        .unwrap();
    let descriptor = Descriptor::from_sddl(&format!("D:P(A;OICI;FA;;;{owner})")).unwrap();
    let file = open_directory(&path, READ_CONTROL | WRITE_DAC).unwrap();
    assert_eq!(
        unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                descriptor_acl(&descriptor).unwrap(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    drop(file);
    let before = security(&path);
    assert!(before.1);
    assert!(!before.0.contains(";;;OW)"));
    let _working_directory = OpenOptions::new()
        .access_mode(FILE_TRAVERSE | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&path)
        .unwrap();
    super::super::private_dir(&path).unwrap();
    assert_eq!(security(&path), before);
}

#[test]
fn private_directory_requires_persistent_acls() {
    assert!(require_persistent_acls(0).is_err());
    assert!(require_persistent_acls(FILE_PERSISTENT_ACLS).is_ok());
}
