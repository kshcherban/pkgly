#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::resolver::{
    ResolvedVirtualMember, VirtualMemberClient, VirtualResolutionCache, VirtualResolver,
};
use crate::repository::r#virtual::config::VirtualResolutionOrder;
use async_trait::async_trait;
use http::Method;
use nr_core::storage::StoragePath;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Default)]
struct RecordingClient {
    responses: Arc<Mutex<HashMap<Uuid, VecDeque<Option<crate::repository::RepoResponse>>>>>,
    calls: Arc<Mutex<Vec<Uuid>>>,
}

impl RecordingClient {
    fn with_response(
        self,
        member: Uuid,
        response: Option<crate::repository::RepoResponse>,
    ) -> Self {
        {
            let mut guard = futures::executor::block_on(self.responses.lock());
            guard.entry(member).or_default().push_back(response);
        }
        self
    }

    async fn record_call(&self, member: Uuid) {
        let mut calls = self.calls.lock().await;
        calls.push(member);
    }

    async fn calls(&self) -> Vec<Uuid> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl VirtualMemberClient for RecordingClient {
    type Error = crate::repository::RepositoryHandlerError;

    async fn fetch(
        &self,
        member: &ResolvedVirtualMember,
        _path: &StoragePath,
        _method: Method,
    ) -> Result<Option<crate::repository::RepoResponse>, Self::Error> {
        self.record_call(member.repository_id).await;
        let mut guard = self.responses.lock().await;
        let response = guard
            .get_mut(&member.repository_id)
            .and_then(|queue| queue.pop_front())
            .flatten();
        Ok(response)
    }
}

fn member(priority: u32) -> ResolvedVirtualMember {
    ResolvedVirtualMember {
        repository_id: Uuid::new_v4(),
        repository_name: format!("repo-{priority}"),
        priority,
        enabled: true,
    }
}

#[tokio::test]
async fn resolver_honors_priority_and_skips_disabled() {
    let mut members = vec![member(50), member(10), member(5)];
    members.push(ResolvedVirtualMember {
        enabled: false,
        ..member(1)
    });

    let priority50 = members[0].repository_id;
    let priority10 = members[1].repository_id;
    let priority5 = members[2].repository_id;

    let client = RecordingClient::default()
        .with_response(
            priority10,
            Some(crate::repository::RepoResponse::basic_text_response(
                http::StatusCode::OK,
                "from-10",
            )),
        )
        .with_response(
            priority50,
            Some(crate::repository::RepoResponse::basic_text_response(
                http::StatusCode::OK,
                "from-50",
            )),
        );

    let resolver = VirtualResolver::new(
        members,
        VirtualResolutionOrder::Priority,
        VirtualResolutionCache::new(Duration::from_secs(30)),
        client.clone(),
    );

    let path = StoragePath::from("left-pad");
    let resolved = resolver
        .resolve("left-pad", &path, Method::GET)
        .await
        .expect("resolution succeeds")
        .expect("some result");

    assert_eq!(resolved.member.repository_id, priority10);
    assert_eq!(client.calls().await, vec![priority5, priority10],);

    let response = resolved.response.into_response_default();
    assert_eq!(response.status(), http::StatusCode::OK);
}

#[tokio::test]
async fn cache_prevents_redundant_lookups_until_ttl_expires() {
    let first = member(0);
    let second = member(1);

    let client = RecordingClient::default()
        .with_response(
            first.repository_id,
            Some(crate::repository::RepoResponse::basic_text_response(
                http::StatusCode::OK,
                "cached",
            )),
        )
        .with_response(
            first.repository_id,
            Some(crate::repository::RepoResponse::basic_text_response(
                http::StatusCode::OK,
                "cached",
            )),
        )
        .with_response(first.repository_id, None)
        .with_response(
            second.repository_id,
            Some(crate::repository::RepoResponse::basic_text_response(
                http::StatusCode::OK,
                "late",
            )),
        );

    let resolver = VirtualResolver::new(
        vec![first.clone(), second.clone()],
        VirtualResolutionOrder::Priority,
        VirtualResolutionCache::new(Duration::from_millis(25)),
        client.clone(),
    );

    let path = StoragePath::from("lodash");
    let first_res = resolver
        .resolve("lodash", &path, Method::GET)
        .await
        .expect("ok")
        .expect("result");
    assert_eq!(first_res.member.repository_id, first.repository_id);

    // Second resolution should hit cache and avoid new fetches
    let second_res = resolver
        .resolve("lodash", &path, Method::GET)
        .await
        .expect("ok")
        .expect("result");
    assert_eq!(second_res.member.repository_id, first.repository_id);
    assert_eq!(client.calls().await.len(), 2);

    // After TTL, resolver should query again and move to next member because first will return None
    tokio::time::sleep(Duration::from_millis(30)).await;
    let third_res = resolver
        .resolve("lodash", &path, Method::GET)
        .await
        .expect("ok")
        .expect("result");
    assert_eq!(third_res.member.repository_id, second.repository_id);
    assert!(client.calls().await.len() >= 4);
}
