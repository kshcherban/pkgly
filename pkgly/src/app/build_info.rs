// ABOUTME: Exposes compile-time package build metadata to API and startup logs.
// ABOUTME: Normalizes optional source revision identifiers into short commit IDs.
const SHORT_COMMIT_ID_LEN: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildInfo {
    pub(crate) version: &'static str,
    pub(crate) commit_id: Option<String>,
}

pub(crate) fn current_build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        commit_id: normalize_commit_id(option_env!("PKGLY_COMMIT_ID")),
    }
}

pub(crate) fn normalize_commit_id(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.len() < SHORT_COMMIT_ID_LEN {
        return None;
    }
    if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(value[..SHORT_COMMIT_ID_LEN].to_string())
}

#[cfg(test)]
mod tests;
