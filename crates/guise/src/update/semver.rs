//! Version parsing and comparison for release tags.

/// Parse `major.minor.patch`, tolerating a leading `v` and extra fields.
fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim().trim_start_matches('v');
    let mut fields = version
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u64>().ok());
    Some((fields.next()??, fields.next()??, fields.next()??))
}

/// Whether `latest` is strictly newer than `current`. Anything that doesn't
/// parse as a version is never newer — an unreadable tag must not be offered as
/// an update.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(new), Some(old)) => new > old,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_detected() {
        assert!(is_newer("1.21.0", "1.20.0"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(is_newer("1.20.1", "1.20.0"));
        assert!(is_newer("v1.21.0", "1.20.0")); // tolerates a leading v
    }

    #[test]
    fn same_or_older_is_not_newer() {
        assert!(!is_newer("1.20.0", "1.20.0"));
        assert!(!is_newer("1.19.0", "1.20.0"));
        assert!(!is_newer("nonsense", "1.20.0"));
        assert!(!is_newer("1.20.0", "garbage"));
    }

    #[test]
    fn extra_fields_are_tolerated() {
        assert!(is_newer("1.21.0-beta.1", "1.20.0"));
        assert!(is_newer("1.21.0+build5", "1.20.9"));
    }
}
