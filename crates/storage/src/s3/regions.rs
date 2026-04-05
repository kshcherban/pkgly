use aws_types::region::Region;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use strum::EnumIter;
use url::Url;
use utoipa::ToSchema;

#[derive(Clone, Debug, Eq, Copy, PartialEq, Serialize, Deserialize, ToSchema, EnumIter)]
pub enum S3StorageRegion {
    /// us-east-1
    UsEast1,
    /// us-east-2
    UsEast2,
    /// us-west-1
    UsWest1,
    /// us-west-2
    UsWest2,
    /// ca-central-1
    CaCentral1,
    /// af-south-1
    AfSouth1,
    /// ap-east-1
    ApEast1,
    /// ap-south-1
    ApSouth1,
    /// ap-northeast-1
    ApNortheast1,
    /// ap-northeast-2
    ApNortheast2,
    /// ap-northeast-3
    ApNortheast3,
    /// ap-southeast-1
    ApSoutheast1,
    /// ap-southeast-2
    ApSoutheast2,
    /// cn-north-1
    CnNorth1,
    /// cn-northwest-1
    CnNorthwest1,
    /// eu-north-1
    EuNorth1,
    /// eu-central-1
    EuCentral1,
    /// eu-central-2
    EuCentral2,
    /// eu-west-1
    EuWest1,
    /// eu-west-2
    EuWest2,
    /// eu-west-3
    EuWest3,
    /// il-central-1
    IlCentral1,
    /// me-south-1
    MeSouth1,
    /// sa-east-1
    SaEast1,
}
impl Display for S3StorageRegion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            S3StorageRegion::UsEast1 => "us-east-1",
            S3StorageRegion::UsEast2 => "us-east-2",
            S3StorageRegion::UsWest1 => "us-west-1",
            S3StorageRegion::UsWest2 => "us-west-2",
            S3StorageRegion::CaCentral1 => "ca-central-1",
            S3StorageRegion::AfSouth1 => "af-south-1",
            S3StorageRegion::ApEast1 => "ap-east-1",
            S3StorageRegion::ApSouth1 => "ap-south-1",
            S3StorageRegion::ApNortheast1 => "ap-northeast-1",
            S3StorageRegion::ApNortheast2 => "ap-northeast-2",
            S3StorageRegion::ApNortheast3 => "ap-northeast-3",
            S3StorageRegion::ApSoutheast1 => "ap-southeast-1",
            S3StorageRegion::ApSoutheast2 => "ap-southeast-2",
            S3StorageRegion::CnNorth1 => "cn-north-1",
            S3StorageRegion::CnNorthwest1 => "cn-northwest-1",
            S3StorageRegion::EuNorth1 => "eu-north-1",
            S3StorageRegion::EuCentral1 => "eu-central-1",
            S3StorageRegion::EuCentral2 => "eu-central-2",
            S3StorageRegion::EuWest1 => "eu-west-1",
            S3StorageRegion::EuWest2 => "eu-west-2",
            S3StorageRegion::EuWest3 => "eu-west-3",
            S3StorageRegion::IlCentral1 => "il-central-1",
            S3StorageRegion::MeSouth1 => "me-south-1",
            S3StorageRegion::SaEast1 => "sa-east-1",
        };
        f.write_str(value)
    }
}

impl From<S3StorageRegion> for Region {
    fn from(value: S3StorageRegion) -> Self {
        Region::new(value.to_string())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct CustomRegion {
    pub custom_region: Option<String>,
    pub endpoint: Url,
}
