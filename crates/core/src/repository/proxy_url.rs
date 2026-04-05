use std::fmt::Display;

use derive_more::derive::{AsRef, Deref};
use schemars::JsonSchema;
use serde::Serialize;
use tracing::{instrument, trace};
use url::Url;

use crate::storage::StoragePath;

#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Deref, AsRef)]
pub struct ProxyURL(String);

impl Serialize for ProxyURL {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ProxyURL {
    fn deserialize<D>(deserializer: D) -> Result<ProxyURL, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        ProxyURL::try_from(s).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<String> for ProxyURL {
    type Error = url::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut s = value;
        if s.ends_with("/") {
            s.pop();
        }
        let url = url::Url::parse(&s)?;
        trace!(url = %url, "Parsed URL");
        Ok(ProxyURL(s))
    }
}
impl Display for ProxyURL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl From<ProxyURL> for String {
    fn from(url: ProxyURL) -> String {
        url.0
    }
}
impl ProxyURL {
    /// Creates a URL from a proxyURL and a path
    #[instrument]
    pub fn add_storage_path(&self, path: StoragePath) -> Result<Url, url::ParseError> {
        let mut url = Url::parse(&self.0)?;
        let base_segments: Vec<String> = url
            .path_segments()
            .map(|segments| {
                segments
                    .filter(|segment| !segment.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        let mut extra_segments: Vec<String> = path
            .into_iter()
            .map(|segment| segment.to_string())
            .collect();

        if let (Some(last_base), Some(first_extra)) =
            (base_segments.last(), extra_segments.first_mut())
            && last_base == first_extra
        {
            extra_segments.remove(0);
        }

        if !extra_segments.is_empty() {
            let mut segments_mut = url
                .path_segments_mut()
                .map_err(|_| url::ParseError::RelativeUrlWithoutBase)?;
            for segment in &extra_segments {
                segments_mut.push(segment);
            }
            drop(segments_mut);
        }

        if let Some(query) = url.query() {
            trace!(url = %url, ?query, "Creating URL with query");
        } else {
            trace!(url = %url, "Creating URL");
        }
        Ok(url)
    }
}
