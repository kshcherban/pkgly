// ABOUTME: Tests outbound destination classification and explicit exceptions.
// ABOUTME: Covers private, global, hostname, and CIDR policy behavior.
use super::*;

fn policy() -> EgressPolicy {
    EgressPolicy::from_settings(&EgressSettings {
        allowed_hosts: vec!["internal.example".into()],
        allowed_cidrs: vec!["127.0.0.0/8".into()],
    })
    .unwrap()
}

#[test]
fn blocks_private_literal_without_exception() {
    let policy = EgressPolicy::from_settings(&EgressSettings::default()).unwrap();
    assert_eq!(
        policy.validate_url(&url::Url::parse("http://127.0.0.1/").unwrap()),
        Err(EgressPolicyError::Blocked)
    );
}

#[test]
fn blocks_noncanonical_loopback_ipv4_literals() {
    let policy = EgressPolicy::from_settings(&EgressSettings::default()).unwrap();
    for value in ["http://127.1/", "http://2130706433/", "http://0x7f000001/"] {
        let url = url::Url::parse(value).unwrap();
        assert_eq!(policy.validate_url(&url), Err(EgressPolicyError::Blocked));
    }
}

#[test]
fn blocks_private_ipv4_mapped_ipv6_without_exception() {
    let policy = EgressPolicy::from_settings(&EgressSettings::default()).unwrap();
    let url = url::Url::parse("http://[::ffff:127.0.0.1]/").unwrap();
    assert_eq!(policy.validate_url(&url), Err(EgressPolicyError::Blocked));
}

#[test]
fn allows_global_literal_and_explicit_cidr() {
    let policy = policy();
    assert!(
        policy
            .validate_url(&url::Url::parse("https://8.8.8.8/").unwrap())
            .is_ok()
    );
    assert!(
        policy
            .validate_url(&url::Url::parse("http://127.0.0.1/").unwrap())
            .is_ok()
    );
}

#[test]
fn rejects_credentials_and_non_http_schemes() {
    let policy = policy();
    assert_eq!(
        policy.validate_url(&url::Url::parse("ftp://8.8.8.8/").unwrap()),
        Err(EgressPolicyError::UnsupportedScheme)
    );
    assert_eq!(
        policy.validate_url(&url::Url::parse("http://user@8.8.8.8/").unwrap()),
        Err(EgressPolicyError::Credentials)
    );
}

#[derive(Debug)]
struct Wrapper(Box<dyn std::error::Error + Send + Sync>);

impl std::fmt::Display for Wrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wrapped")
    }
}

impl std::error::Error for Wrapper {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[test]
fn egress_blocked_detection_walks_source_chain() {
    let blocked = Wrapper(Box::new(EgressBlockedError));
    let doubly_wrapped = Wrapper(Box::new(blocked));
    assert!(is_egress_blocked(&doubly_wrapped));
    assert!(!is_egress_blocked(&Wrapper(Box::new(
        std::io::Error::other("network down")
    ))));
}
