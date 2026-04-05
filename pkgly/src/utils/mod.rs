use digestible::{Digester, Digestible, IntoBase64, byteorder::NativeEndian};
use http::HeaderValue;
use sha2_0_11::Digest;
pub mod requests;
pub mod response;
pub use response::*;
pub mod header;
pub mod other;
pub mod request_logging;
pub mod upstream;
pub use requests::*;

use self::builder::error::ResponseBuildError;

pub fn generate_etag(data: &impl Digestible) -> Result<HeaderValue, ResponseBuildError> {
    let hasher = sha2_0_11::Sha256::new().into_base64();
    let result = hasher.digest::<NativeEndian>(data);

    Ok(HeaderValue::try_from(result)?)
}
