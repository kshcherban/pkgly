// ABOUTME: Classifies IP addresses as globally routable for outbound traffic policy.
// ABOUTME: Shared by the Pkgly HTTP egress policy and the S3 storage egress resolver.
use std::net::IpAddr;

fn is_in_ipv6_network(address: u128, network: u128, prefix: u32) -> bool {
    address >> (128 - prefix) == network >> (128 - prefix)
}

fn is_ietf_global_ipv6_exception(address: u128) -> bool {
    matches!(
        address,
        0x2001_0001_0000_0000_0000_0000_0000_0001..=0x2001_0001_0000_0000_0000_0000_0000_0003
    ) || is_in_ipv6_network(address, 0x2001_0003_0000_0000_0000_0000_0000_0000, 32)
        || is_in_ipv6_network(address, 0x2001_0004_0112_0000_0000_0000_0000_0000, 48)
        || is_in_ipv6_network(address, 0x2001_0020_0000_0000_0000_0000_0000_0000, 28)
        || is_in_ipv6_network(address, 0x2001_0030_0000_0000_0000_0000_0000_0000, 28)
}

/// Returns true when `address` is globally routable.
///
/// Blocks loopback, private, link-local, multicast, unspecified, documentation,
/// benchmark, carrier-grade NAT, broadcast, all IPv4-mapped IPv6 addresses,
/// ULA, link-local, multicast, and reserved or documentation IPv6 ranges.
pub fn is_global(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [a, b, c, d] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113)
                || a >= 224
                || (a == 255 && b == 255 && c == 255 && d == 255))
        }
        IpAddr::V6(ip) => {
            if ip.to_ipv4().is_some() {
                return false;
            }
            let value = u128::from(ip);
            let global_unicast =
                is_in_ipv6_network(value, 0x2000_0000_0000_0000_0000_0000_0000_0000, 3);
            let ietf_assignments =
                is_in_ipv6_network(value, 0x2001_0000_0000_0000_0000_0000_0000_0000, 23);
            global_unicast
                && (!ietf_assignments || is_ietf_global_ipv6_exception(value))
                && !is_in_ipv6_network(value, 0x2001_0db8_0000_0000_0000_0000_0000_0000, 32)
                && !is_in_ipv6_network(value, 0x2002_0000_0000_0000_0000_0000_0000_0000, 16)
                && !is_in_ipv6_network(value, 0x3fff_0000_0000_0000_0000_0000_0000_0000, 20)
        }
    }
}

#[cfg(test)]
mod tests;
