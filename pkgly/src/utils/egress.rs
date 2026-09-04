// ABOUTME: Defines the outbound network policy and DNS resolver used by Pkgly.
// ABOUTME: Blocks non-global destinations unless an explicit host or CIDR exception exists.
use std::{
    net::IpAddr,
    sync::{Arc, OnceLock},
};

use ahash::{HashSet, HashSetExt};
use ipnet::IpNet;
use parking_lot::RwLock;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::app::config::EgressSettings;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EgressPolicyError {
    #[error("unsupported outbound URL scheme")]
    UnsupportedScheme,
    #[error("outbound URL host is missing")]
    MissingHost,
    #[error("outbound URL credentials are not allowed")]
    Credentials,
    #[error("outbound destination is blocked by egress policy")]
    Blocked,
    #[error("invalid egress CIDR: {0}")]
    InvalidCidr(String),
}

#[derive(Debug, Clone)]
pub struct EgressPolicy {
    allowed_hosts: HashSet<String>,
    allowed_cidrs: Vec<IpNet>,
}

impl EgressPolicy {
    pub fn from_settings(settings: &EgressSettings) -> Result<Self, EgressPolicyError> {
        let allowed_cidrs = settings
            .allowed_cidrs
            .iter()
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| EgressPolicyError::InvalidCidr(value.clone()))
            })
            .collect::<Result<Vec<IpNet>, _>>()?;
        let allowed_hosts = settings
            .allowed_hosts
            .iter()
            .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
            .filter(|host| !host.is_empty())
            .collect();
        Ok(Self {
            allowed_hosts,
            allowed_cidrs,
        })
    }

    pub fn validate_url(&self, url: &url::Url) -> Result<(), EgressPolicyError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(EgressPolicyError::UnsupportedScheme);
        }
        let host = url.host_str().ok_or(EgressPolicyError::MissingHost)?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(EgressPolicyError::Credentials);
        }
        match url.host() {
            Some(url::Host::Ipv4(ip)) => self.validate_address(host, IpAddr::V4(ip)),
            Some(url::Host::Ipv6(ip)) => self.validate_address(host, IpAddr::V6(ip)),
            Some(url::Host::Domain(_)) => Ok(()),
            None => Err(EgressPolicyError::MissingHost),
        }
    }

    fn validate_address(&self, host: &str, address: IpAddr) -> Result<(), EgressPolicyError> {
        let host_allowed = self
            .allowed_hosts
            .contains(&host.trim_end_matches('.').to_ascii_lowercase());
        let range_allowed = self
            .allowed_cidrs
            .iter()
            .any(|network| network.contains(&address));
        if nr_core::egress::is_global(address) || host_allowed || range_allowed {
            Ok(())
        } else {
            Err(EgressPolicyError::Blocked)
        }
    }
}

static GLOBAL_POLICY: OnceLock<Arc<RwLock<EgressPolicy>>> = OnceLock::new();

fn runtime_policy(settings: &EgressSettings) -> Result<EgressPolicy, EgressPolicyError> {
    let policy = EgressPolicy::from_settings(settings)?;
    #[cfg(test)]
    let policy = {
        let mut policy = policy;
        if let Ok(test_server) = "127.0.0.1/32".parse() {
            policy.allowed_cidrs.push(test_server);
        }
        policy
    };
    Ok(policy)
}

pub fn install(settings: &EgressSettings) -> Result<(), EgressPolicyError> {
    let policy = runtime_policy(settings)?;
    let lock = GLOBAL_POLICY.get_or_init(|| Arc::new(RwLock::new(policy.clone())));
    *lock.write() = policy;
    Ok(())
}

fn global() -> EgressPolicy {
    let lock = GLOBAL_POLICY.get_or_init(|| {
        Arc::new(RwLock::new(
            runtime_policy(&EgressSettings::default()).unwrap_or_else(|_| EgressPolicy {
                allowed_hosts: HashSet::new(),
                allowed_cidrs: Vec::new(),
            }),
        ))
    });
    lock.read().clone()
}

pub fn validate_url(url: &url::Url) -> Result<(), EgressPolicyError> {
    global().validate_url(url)
}

/// Validates a repository proxy route URL against the egress policy.
pub fn validate_proxy_url(
    value: &nr_core::repository::proxy_url::ProxyURL,
) -> Result<(), EgressPolicyError> {
    let parsed = url::Url::parse(value.as_str()).map_err(|_| EgressPolicyError::MissingHost)?;
    validate_url(&parsed)
}

/// Validates every proxy route URL, rejecting any that violates the policy.
pub fn validate_proxy_urls<'a>(
    urls: impl IntoIterator<Item = &'a nr_core::repository::proxy_url::ProxyURL>,
) -> Result<(), EgressPolicyError> {
    for url in urls {
        validate_proxy_url(url)?;
    }
    Ok(())
}

/// Marker error surfacing through DNS resolution and transport layers when a
/// destination is blocked by the egress policy. Detected via the source chain.
#[derive(Debug)]
pub struct EgressBlockedError;

impl std::fmt::Display for EgressBlockedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "outbound destination is blocked by egress policy")
    }
}

impl std::error::Error for EgressBlockedError {}

/// Returns true when the error chain contains the egress policy marker.
pub fn is_egress_blocked<E: std::error::Error + 'static>(error: &E) -> bool {
    fn walk(source: &(dyn std::error::Error + 'static)) -> bool {
        if source.downcast_ref::<EgressBlockedError>().is_some() {
            return true;
        }
        source.source().is_some_and(walk)
    }
    walk(error)
}

#[derive(Debug, Clone)]
pub struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let policy = global();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                return Err(
                    Box::new(EgressBlockedError) as Box<dyn std::error::Error + Send + Sync>
                );
            }
            for address in &addresses {
                policy.validate_address(&host, address.ip()).map_err(
                    |_| -> Box<dyn std::error::Error + Send + Sync> {
                        Box::new(EgressBlockedError)
                    },
                )?;
            }
            let addrs: Addrs = Box::new(addresses.into_iter());
            Ok(addrs)
        })
    }
}

pub fn resolver() -> Arc<SafeResolver> {
    Arc::new(SafeResolver)
}

#[cfg(test)]
mod tests;
