pub const APPLICATION_VERSION: &str = match option_env!("OPENNOW_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub fn update_channel(version: &str) -> &'static str {
    if semver::Version::parse(version)
        .is_ok_and(|version| version.pre.as_str().split('.').next() == Some("nightly"))
    {
        "nightly"
    } else {
        "stable"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_version_is_semver() {
        semver::Version::parse(APPLICATION_VERSION).expect("application version must be semver");
    }

    #[test]
    fn update_channel_follows_build_identity() {
        assert_eq!(update_channel("1.0.0-nightly.123.2"), "nightly");
        for version in ["1.0.0", "1.0.0-beta.1", "1.0.0+nightly", "invalid"] {
            assert_eq!(update_channel(version), "stable");
        }
    }

    #[test]
    fn nightly_version_preserves_run_and_attempt() {
        let version = semver::Version::parse("1.0.0-nightly.123456.2").unwrap();
        assert_eq!((version.major, version.minor, version.patch), (1, 0, 0));
        assert_eq!(version.pre.as_str(), "nightly.123456.2");
        assert_eq!(version.to_string(), "1.0.0-nightly.123456.2");
    }
}
