#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn test_host_name_port() {
    {
        let config = DatabaseConfig::default();
        let (host, port) = config.host_name_port().unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 5432);
    }
    {
        let config = DatabaseConfig {
            host: "localhost:5433".to_string(),
            port: None,
            ..DatabaseConfig::default()
        };
        let (host, port) = config.host_name_port().unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 5433);
    }
}
