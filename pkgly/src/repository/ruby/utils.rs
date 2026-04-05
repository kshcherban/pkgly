#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGemFileName {
    pub name: String,
    pub version: String,
    pub platform: Option<String>,
}

/// Parse a `.gem` file name into (name, version, optional platform).
///
/// The canonical RubyGems file naming scheme is:
/// - `{name}-{version}.gem`
/// - `{name}-{version}-{platform}.gem` (platform may contain `-`)
///
/// This parser is intentionally conservative and only accepts versions that
/// start with an ASCII digit.
pub fn parse_gem_file_name(file_name: &str) -> Option<ParsedGemFileName> {
    let base = file_name.strip_suffix(".gem")?;
    if base.is_empty() {
        return None;
    }

    for (idx, byte) in base.as_bytes().iter().enumerate().rev() {
        if *byte != b'-' {
            continue;
        }

        let (left, right_with_dash) = base.split_at(idx);
        let right = right_with_dash.strip_prefix('-')?;
        if left.is_empty() || right.is_empty() {
            continue;
        }

        let (version, platform) = match right.split_once('-') {
            Some((version, platform)) => {
                if platform.is_empty() {
                    continue;
                }
                (version, Some(platform))
            }
            None => (right, None),
        };

        let version_first = version.as_bytes().first().copied();
        if version.is_empty() || !matches!(version_first, Some(b'0'..=b'9')) {
            continue;
        }

        return Some(ParsedGemFileName {
            name: left.to_string(),
            version: version.to_string(),
            platform: platform.map(str::to_string),
        });
    }

    None
}
