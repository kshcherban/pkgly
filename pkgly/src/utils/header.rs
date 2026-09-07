// ABOUTME: Provides typed accessors for HTTP header values and maps.
// ABOUTME: Preserves empty-header handling while reducing duplicate conversions.
use http::{HeaderName, HeaderValue, header::ToStrError};
use tracing::warn;
pub mod date_time;
/// Extension trait for [http::HeaderValue]
pub trait HeaderValueExt {
    /// Converts the header value to a string
    fn to_string(&self) -> Result<String, ToStrError>;
    /// Parses the header value into a type Over the [TryFrom] trait
    ///
    /// Error must be convertible from [ToStrError]
    fn parsed<T, E>(&self) -> Result<T, E>
    where
        T: TryFrom<String, Error = E>,
        E: From<ToStrError>;
}
impl HeaderValueExt for HeaderValue {
    fn to_string(&self) -> Result<String, ToStrError> {
        self.to_str().map(|x| x.to_string())
    }

    fn parsed<T, E>(&self) -> Result<T, E>
    where
        T: TryFrom<String, Error = E>,
        E: From<ToStrError>,
    {
        let value = self.to_string()?;
        T::try_from(value)
    }
}

pub trait HeaderMapExt {
    fn get_string_ignore_empty(&self, key: &HeaderName) -> Option<String>;
    fn get_str_ignore_empty<'headers>(&'headers self, key: &HeaderName) -> Option<&'headers str>;
}

impl HeaderMapExt for http::HeaderMap {
    fn get_string_ignore_empty(&self, header: &HeaderName) -> Option<String> {
        self.get_str_ignore_empty(header).map(str::to_owned)
    }

    fn get_str_ignore_empty<'headers>(&'headers self, key: &HeaderName) -> Option<&'headers str> {
        self.get(key).and_then(|v| v.to_str().ok()).and_then(|v| {
            if v.is_empty() {
                warn!(?key, "Empty header Value",);
                None
            } else {
                Some(v)
            }
        })
    }
}
