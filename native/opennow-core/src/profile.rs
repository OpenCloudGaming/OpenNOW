use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

static CREDENTIAL_NAMESPACE: OnceLock<Option<String>> = OnceLock::new();

pub fn name() -> io::Result<Option<String>> {
    validate_name(env::var_os("OPENNOW_PROFILE").as_deref()).map(|name| name.map(str::to_owned))
}

pub(crate) fn validate_name(value: Option<&OsStr>) -> io::Result<Option<&str>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let name = value.to_str().filter(|name| {
        (1..=64).contains(&name.len())
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    });
    name.map(Some).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "OPENNOW_PROFILE must match [A-Za-z0-9][A-Za-z0-9_-]{0,63}",
        )
    })
}

pub fn initialize_credential_namespace(data_dir: &Path) -> io::Result<()> {
    let namespace = name()?
        .map(|_| credential_namespace(data_dir))
        .transpose()?;
    CREDENTIAL_NAMESPACE
        .set(namespace)
        .map_err(|_| io::Error::other("Credential namespace was already initialized"))
}

fn credential_namespace(data_dir: &Path) -> io::Result<String> {
    let directory = fs::canonicalize(data_dir)?;
    let digest = Sha256::digest(directory.as_os_str().as_encoded_bytes());
    Ok(format!("{digest:x}"))
}

pub fn credential_service_name(service: &str) -> String {
    scoped_credential_service_name(
        service,
        CREDENTIAL_NAMESPACE.get().and_then(Option::as_deref),
    )
}

fn scoped_credential_service_name(service: &str, namespace: Option<&str>) -> String {
    match namespace {
        Some(namespace) => format!("{service}.profile.{namespace}"),
        None => service.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_profile_validation_rejects_paths_and_invalid_environment_values() {
        assert_eq!(validate_name(None).unwrap(), None);
        for valid in ["a", "0", "Player_2-test", &"A".repeat(64)] {
            assert_eq!(validate_name(Some(OsStr::new(valid))).unwrap(), Some(valid));
        }
        for invalid in [
            "",
            ".",
            "..",
            "../other",
            "/absolute",
            "a/b",
            "a\\b",
            "a.b",
            "-name",
            "_name",
            " name",
            "name ",
            "na:me",
            "pläyer",
            &"a".repeat(65),
        ] {
            assert_eq!(
                validate_name(Some(OsStr::new(invalid))).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert!(validate_name(Some(OsStr::from_bytes(&[0xff]))).is_err());
        }
    }

    #[test]
    fn named_profile_credential_namespaces_use_canonical_directory_identity() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("profiles/one");
        let second = directory.path().join("profiles/two");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let namespace = credential_namespace(&first).unwrap();
        assert_eq!(namespace.len(), 64);
        assert_ne!(namespace, credential_namespace(&second).unwrap());
        assert_eq!(namespace, credential_namespace(&first.join(".")).unwrap());
        assert!(credential_namespace(&directory.path().join("missing")).is_err());
        #[cfg(unix)]
        {
            let alias = directory.path().join("alias");
            std::os::unix::fs::symlink(&first, &alias).unwrap();
            assert_eq!(namespace, credential_namespace(&alias).unwrap());
        }
        for service in [
            "app.opennow.auth",
            "app.opennow.auth.session-keys",
            "app.opennow.push",
        ] {
            assert_eq!(scoped_credential_service_name(service, None), service);
            assert_eq!(
                scoped_credential_service_name(service, Some(&namespace)),
                format!("{service}.profile.{namespace}")
            );
        }
    }
}
