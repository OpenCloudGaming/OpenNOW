pub const APPLICATION_VERSION: &str = match option_env!("OPENNOW_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_version_is_a_numeric_release_triplet() {
        let parts = APPLICATION_VERSION.split('.').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        assert!(
            parts
                .iter()
                .all(|part| { !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()) })
        );
    }
}
