use async_trait::async_trait;
use http::{Method, StatusCode};
use nr_core::storage::StoragePath;
use parking_lot::RwLock;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

use super::config::VirtualResolutionOrder;
use crate::repository::RepoResponse;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVirtualMember {
    pub repository_id: Uuid,
    pub repository_name: String,
    pub priority: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct VirtualResolutionCache {
    ttl: Duration,
    entries: Arc<RwLock<ahash::HashMap<String, CacheEntry>>>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    member_id: Uuid,
    stored_at: Instant,
}

impl VirtualResolutionCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Arc::new(RwLock::new(ahash::HashMap::default())),
        }
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn update_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }

    pub fn get(&self, key: &str) -> Option<Uuid> {
        let now = Instant::now();
        let mut guard = self.entries.write();
        if let Some(entry) = guard.get(key) {
            if now.duration_since(entry.stored_at) <= self.ttl {
                return Some(entry.member_id);
            }
            guard.remove(key);
        }
        None
    }

    pub fn put(&self, key: impl Into<String>, member_id: Uuid) {
        let mut guard = self.entries.write();
        guard.insert(
            key.into(),
            CacheEntry {
                member_id,
                stored_at: Instant::now(),
            },
        );
    }
}

#[async_trait]
pub trait VirtualMemberClient: Send + Sync {
    type Error: Send + Sync;

    async fn fetch(
        &self,
        member: &ResolvedVirtualMember,
        path: &StoragePath,
        method: Method,
    ) -> Result<Option<RepoResponse>, Self::Error>;
}

#[derive(Debug)]
pub struct ResolveHit {
    pub member: ResolvedVirtualMember,
    pub response: RepoResponse,
}

#[derive(Debug, Clone)]
pub struct VirtualResolver<C: VirtualMemberClient> {
    members: Vec<ResolvedVirtualMember>,
    cache: VirtualResolutionCache,
    client: C,
}

impl<C: VirtualMemberClient> VirtualResolver<C> {
    pub fn new(
        mut members: Vec<ResolvedVirtualMember>,
        order: VirtualResolutionOrder,
        cache: VirtualResolutionCache,
        client: C,
    ) -> Self {
        members.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.repository_name.cmp(&b.repository_name))
        });
        let _ = order;
        Self {
            members,
            cache,
            client,
        }
    }

    pub fn members(&self) -> &[ResolvedVirtualMember] {
        &self.members
    }

    pub async fn resolve(
        &self,
        cache_key: &str,
        path: &StoragePath,
        method: Method,
    ) -> Result<Option<ResolveHit>, C::Error> {
        if let Some(member_id) = self.cache.get(cache_key) {
            if let Some(member) = self
                .members
                .iter()
                .find(|m| m.enabled && m.repository_id == member_id)
            {
                if let Some(response) = self.client.fetch(member, path, method.clone()).await? {
                    return Ok(Some(ResolveHit {
                        member: member.clone(),
                        response,
                    }));
                }
            }
        }

        let mut auth_failure: Option<ResolveHit> = None;
        for member in self.members.iter().filter(|m| m.enabled) {
            let Some(response) = self.client.fetch(member, path, method.clone()).await? else {
                continue;
            };

            if is_auth_failure(&response) {
                auth_failure = Some(ResolveHit {
                    member: member.clone(),
                    response,
                });
                continue;
            }

            if is_not_found(&response) {
                continue;
            }

            self.cache.put(cache_key.to_string(), member.repository_id);
            return Ok(Some(ResolveHit {
                member: member.clone(),
                response,
            }));
        }

        if let Some(failure) = auth_failure {
            return Ok(Some(failure));
        }

        Ok(None)
    }
}

fn is_auth_failure(response: &RepoResponse) -> bool {
    matches!(
        response,
        RepoResponse::Other(resp)
            if resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN
    )
}

fn is_not_found(response: &RepoResponse) -> bool {
    matches!(response, RepoResponse::Other(resp) if resp.status() == StatusCode::NOT_FOUND)
}
