use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct AccessLogContext(Arc<Mutex<AccessLogData>>);

#[derive(Debug, Default)]
struct AccessLogData {
    repository_id: Option<Uuid>,
    user: Option<String>,
    user_id: Option<i32>,
    audit_action: Option<String>,
    resource_kind: Option<String>,
    resource_id: Option<String>,
    resource_name: Option<String>,
    storage_id: Option<Uuid>,
    target_user_id: Option<i32>,
    token_id: Option<i32>,
    audit_path: Option<String>,
    audit_query: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccessLogSnapshot {
    pub repository_id: Option<Uuid>,
    pub user: Option<String>,
    pub user_id: Option<i32>,
    pub audit_action: Option<String>,
    pub resource_kind: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    pub storage_id: Option<Uuid>,
    pub target_user_id: Option<i32>,
    pub token_id: Option<i32>,
    pub audit_path: Option<String>,
    pub audit_query: Option<String>,
}

impl AccessLogContext {
    pub fn set_repository_id(&self, repository_id: Uuid) {
        self.0.lock().repository_id = Some(repository_id);
    }

    pub fn set_user(&self, user: impl Into<String>) {
        self.0.lock().user = Some(user.into());
    }

    pub fn set_user_id(&self, user_id: i32) {
        self.0.lock().user_id = Some(user_id);
    }

    pub fn set_audit_action(&self, action: impl Into<String>) {
        self.0.lock().audit_action = Some(action.into());
    }

    pub fn set_resource_kind(&self, resource_kind: impl Into<String>) {
        self.0.lock().resource_kind = Some(resource_kind.into());
    }

    pub fn set_resource_id(&self, resource_id: impl Into<String>) {
        self.0.lock().resource_id = Some(resource_id.into());
    }

    pub fn set_resource_name(&self, resource_name: impl Into<String>) {
        self.0.lock().resource_name = Some(resource_name.into());
    }

    pub fn set_storage_id(&self, storage_id: Uuid) {
        self.0.lock().storage_id = Some(storage_id);
    }

    pub fn set_target_user_id(&self, user_id: i32) {
        self.0.lock().target_user_id = Some(user_id);
    }

    pub fn set_token_id(&self, token_id: i32) {
        self.0.lock().token_id = Some(token_id);
    }

    pub fn set_audit_path(&self, path: impl Into<String>) {
        self.0.lock().audit_path = Some(path.into());
    }

    pub fn set_audit_query(&self, query: impl Into<String>) {
        self.0.lock().audit_query = Some(query.into());
    }

    pub fn snapshot(&self) -> AccessLogSnapshot {
        let locked = self.0.lock();
        AccessLogSnapshot {
            repository_id: locked.repository_id,
            user: locked.user.clone(),
            user_id: locked.user_id,
            audit_action: locked.audit_action.clone(),
            resource_kind: locked.resource_kind.clone(),
            resource_id: locked.resource_id.clone(),
            resource_name: locked.resource_name.clone(),
            storage_id: locked.storage_id,
            target_user_id: locked.target_user_id,
            token_id: locked.token_id,
            audit_path: locked.audit_path.clone(),
            audit_query: locked.audit_query.clone(),
        }
    }
}
