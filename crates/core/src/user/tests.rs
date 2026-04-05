#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn test_username() {
    let username = Username::new("test".to_string()).unwrap();
    assert_eq!(username.to_string(), "test");
    assert!(Username::new("te".to_string()).is_err());
    assert!(Username::new("testtesttesttesttesttesttesttesttest".to_string()).is_err());
    assert!(Username::new("test$".to_string()).is_err());
}
