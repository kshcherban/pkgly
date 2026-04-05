#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn test_max_upload_from_str() {
    {
        let max_upload = MaxUpload::from_str("100").unwrap();
        assert_eq!(
            max_upload,
            MaxUpload::Limit(ConfigSize {
                size: 100,
                unit: SizeUnit::Bytes,
            })
        );
    }
    {
        let max_upload = MaxUpload::from_str("100b").unwrap();
        assert_eq!(
            max_upload,
            MaxUpload::Limit(ConfigSize {
                size: 100,
                unit: SizeUnit::Bytes,
            })
        );
    }
    {
        let max_upload = MaxUpload::from_str("100KiB").unwrap();
        assert_eq!(
            max_upload,
            MaxUpload::Limit(ConfigSize {
                size: 100,
                unit: SizeUnit::Kibibytes,
            })
        );
    }
    {
        let max_upload = MaxUpload::from_str("100MiB").unwrap();
        assert_eq!(
            max_upload,
            MaxUpload::Limit(ConfigSize {
                size: 100,
                unit: SizeUnit::Mebibytes,
            })
        );
    }
}
#[test]
fn test_unlimited() {
    {
        let max_upload = MaxUpload::from_str("unlimited").unwrap();
        assert_eq!(max_upload, MaxUpload::Unlimited);
    }
    {
        let max_upload = MaxUpload::from_str("Unlimited").unwrap();
        assert_eq!(max_upload, MaxUpload::Unlimited);
    }
}
