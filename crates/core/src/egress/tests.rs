// ABOUTME: Tests the global-routability classifier shared by egress policies.
// ABOUTME: Covers IPv4 and IPv6 reserved, private, and mapped ranges.
#![allow(clippy::expect_used)]

use super::is_global;
use std::net::IpAddr;

fn ipv4(value: &str) -> IpAddr {
    value.parse().expect("valid ipv4 literal")
}

fn ipv6(value: &str) -> IpAddr {
    value.parse().expect("valid ipv6 literal")
}

#[test]
fn blocks_ipv4_reserved_ranges() {
    for value in [
        "0.0.0.0",
        "10.0.0.1",
        "100.64.0.1",
        "100.127.255.254",
        "127.0.0.1",
        "169.254.169.254",
        "172.16.0.1",
        "172.31.255.254",
        "192.0.0.1",
        "192.0.2.1",
        "192.168.0.1",
        "192.88.99.1",
        "198.18.0.1",
        "198.19.255.254",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "239.255.255.255",
        "240.0.0.1",
        "255.255.255.255",
    ] {
        assert!(!is_global(ipv4(value)), "{value} must not be global");
    }
}

#[test]
fn allows_ipv4_global_addresses() {
    for value in [
        "8.8.8.8",
        "1.1.1.1",
        "93.184.216.34",
        "198.51.101.1",
        "203.0.112.1",
    ] {
        assert!(is_global(ipv4(value)), "{value} must be global");
    }
}

#[test]
fn blocks_ipv6_reserved_ranges() {
    for value in [
        "::",
        "::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:169.254.169.254",
        "::ffff:8.8.8.8",
        "64:ff9b:1::1",
        "100::1",
        "100:0:0:1::1",
        "fc00::1",
        "fd12:3456:789a::1",
        "fe80::1",
        "ff00::1",
        "ff02::1",
        "2001:2::1",
        "2001:db8::1",
        "2001::1",
        "2002::1",
        "3fff::1",
        "5f00::1",
    ] {
        assert!(!is_global(ipv6(value)), "{value} must not be global");
    }
}

#[test]
fn allows_ipv6_global_addresses() {
    for value in [
        "2001:1::1",
        "2001:3::1",
        "2001:4:112::1",
        "2001:4860:4860::8888",
        "2606:4700:4700::1111",
        "2a00:1450:4007:810::200e",
    ] {
        assert!(is_global(ipv6(value)), "{value} must be global");
    }
}
