use axum::extract::{FromRef, FromRequestParts};
use chrono::Utc;
use http::request::Parts;
use nr_core::{
    database::entities::user::{
        UserSafeData, UserType, auth_token::AuthToken, permissions::UserRepositoryPermissions,
    },
    user::permissions::{
        HasPermissions, RepositoryActions, UserPermissions,
        does_user_and_token_have_repository_action,
    },
};
use sqlx::PgPool;
use strum::EnumIs;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::app::{
    Pkgly,
    authentication::{AuthenticationError, AuthenticationRaw, session::Session, verify_login},
};

#[derive(Clone, Debug, PartialEq, EnumIs)]
pub enum RepositoryAuthentication {
    /// An Auth Token was passed under the Authorization Header
    AuthToken(AuthToken, UserSafeData),
    /// Uses a Session Cookie or Session Header
    Session(Session, UserSafeData),
    /// Uses Basic Authorization Header
    Basic(Option<AuthToken>, UserSafeData),
    /// Internal wrapper used by virtual repositories: for `Read` action checks, use the virtual
    /// repository id instead of the member repository id.
    VirtualRepository {
        virtual_repository_id: Uuid,
        inner: Box<RepositoryAuthentication>,
    },
    /// An authorization header was passed but it does not match any known types
    Other(String, String),
    /// No Identification was passed
    NoIdentification,
}
impl RepositoryAuthentication {
    #[instrument]
    pub async fn can_access_repository(
        &self,
        action: RepositoryActions,
        repository_id: Uuid,
        database: &PgPool,
    ) -> Result<bool, AuthenticationError> {
        let (auth, effective_repository_id) =
            unwrap_virtual_for_action(self, action, repository_id);
        match auth {
            RepositoryAuthentication::AuthToken(token, user)
            | RepositoryAuthentication::Basic(Some(token), user) => {
                debug!("Request has an Auth Token. Checking if it has access to the repository");
                does_user_and_token_have_repository_action(
                    user,
                    token,
                    action,
                    repository_id,
                    database,
                )
                .await
                .map_err(AuthenticationError::from)
            }
            RepositoryAuthentication::Session(_, user)
            | RepositoryAuthentication::Basic(None, user) => Ok(user
                .has_action(action, effective_repository_id, database)
                .await?),
            _ => Ok(false),
        }
    }
    #[instrument]
    pub async fn get_user_if_has_action(
        &self,
        action: RepositoryActions,
        repository_id: Uuid,
        database: &PgPool,
    ) -> Result<Option<&UserSafeData>, AuthenticationError> {
        let (auth, effective_repository_id) =
            unwrap_virtual_for_action(self, action, repository_id);
        match auth {
            RepositoryAuthentication::AuthToken(token, user)
            | RepositoryAuthentication::Basic(Some(token), user) => {
                debug!("Request has an Auth Token. Checking if it has access to the repository");
                if does_user_and_token_have_repository_action(
                    user,
                    token,
                    action,
                    effective_repository_id,
                    database,
                )
                .await?
                {
                    Ok(Some(user))
                } else {
                    Ok(None)
                }
            }
            RepositoryAuthentication::Session(_, user)
            | RepositoryAuthentication::Basic(None, user) => {
                if user
                    .has_action(action, effective_repository_id, database)
                    .await?
                {
                    Ok(Some(user))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}
impl HasPermissions for RepositoryAuthentication {
    fn get_permissions(&self) -> Option<UserPermissions> {
        match self {
            RepositoryAuthentication::AuthToken(_, user) => user.get_permissions(),
            RepositoryAuthentication::Session(_, user) => user.get_permissions(),
            RepositoryAuthentication::Basic(_, user) => user.get_permissions(),
            RepositoryAuthentication::VirtualRepository { inner, .. } => inner.get_permissions(),
            _ => None,
        }
    }

    fn user_id(&self) -> Option<i32> {
        match self {
            RepositoryAuthentication::AuthToken(_, user) => Some(user.id),
            RepositoryAuthentication::Session(_, user) => Some(user.id),
            RepositoryAuthentication::Basic(_, user) => Some(user.id),
            RepositoryAuthentication::VirtualRepository { inner, .. } => inner.user_id(),
            _ => None,
        }
    }

    async fn has_action(
        &self,
        action: RepositoryActions,
        repository: Uuid,
        db: &PgPool,
    ) -> Result<bool, sqlx::Error> {
        let (auth, effective_repository_id) = unwrap_virtual_for_action(self, action, repository);
        if auth.is_admin_or_system_manager() {
            return Ok(true);
        }
        let Some(user_id) = auth.user_id() else {
            return Ok(false);
        };
        UserRepositoryPermissions::has_repository_action(
            user_id,
            effective_repository_id,
            action,
            db,
        )
        .await
    }
}
impl RepositoryAuthentication {
    pub fn get_user_id(&self) -> Option<i32> {
        match self {
            RepositoryAuthentication::AuthToken(_, user) => Some(user.id),
            RepositoryAuthentication::Session(_, user) => Some(user.id),
            RepositoryAuthentication::Basic(_, user) => Some(user.id),
            RepositoryAuthentication::VirtualRepository { inner, .. } => inner.get_user_id(),
            _ => None,
        }
    }
    pub fn get_user(&self) -> Option<&UserSafeData> {
        match self {
            RepositoryAuthentication::AuthToken(_, user) => Some(user),
            RepositoryAuthentication::Session(_, user) => Some(user),
            RepositoryAuthentication::Basic(_, user) => Some(user),
            RepositoryAuthentication::VirtualRepository { inner, .. } => inner.get_user(),
            _ => None,
        }
    }
    pub fn has_auth_token(&self) -> bool {
        matches!(
            self,
            RepositoryAuthentication::AuthToken(..) | RepositoryAuthentication::Basic(Some(_), _)
        )
    }

    pub fn wrap_for_virtual_reads(self, virtual_repository_id: Uuid) -> Self {
        RepositoryAuthentication::VirtualRepository {
            virtual_repository_id,
            inner: Box::new(self),
        }
    }

    pub fn as_raw(&self) -> AuthenticationRaw {
        match self {
            RepositoryAuthentication::AuthToken(token, _) => {
                AuthenticationRaw::AuthToken(token.token.clone())
            }
            RepositoryAuthentication::Session(session, _) => {
                AuthenticationRaw::Session(session.clone())
            }
            RepositoryAuthentication::Basic(Some(token), _) => {
                AuthenticationRaw::AuthToken(token.token.clone())
            }
            RepositoryAuthentication::Basic(None, user) => AuthenticationRaw::Basic {
                username: user.username.as_ref().to_string(),
                password: String::new(),
            },
            RepositoryAuthentication::VirtualRepository { inner, .. } => inner.as_raw(),
            RepositoryAuthentication::Other(scheme, value) => {
                AuthenticationRaw::AuthorizationHeaderUnknown(scheme.clone(), value.clone())
            }
            RepositoryAuthentication::NoIdentification => AuthenticationRaw::NoIdentification,
        }
    }
    #[instrument(skip(site))]
    pub async fn from_raw(
        raw_auth: AuthenticationRaw,
        site: &Pkgly,
    ) -> Result<Self, AuthenticationError> {
        match raw_auth {
            AuthenticationRaw::AuthToken(token) => {
                let (token, user) = get_by_auth_token_cached(&token, site).await?;
                Ok(RepositoryAuthentication::AuthToken(token, user))
            }
            AuthenticationRaw::Session(session) => {
                let user = UserSafeData::get_by_id(session.user_id, &site.database)
                    .await?
                    .ok_or(AuthenticationError::Unauthorized)?;
                Ok(RepositoryAuthentication::Session(session, user))
            }
            AuthenticationRaw::Basic { username, password } => {
                match verify_login(username, &password, &site.database).await {
                    Ok(user) => Ok(RepositoryAuthentication::Basic(None, user)),
                    Err(AuthenticationError::Unauthorized) => {
                        let (token, user) = get_by_auth_token_cached(&password, site).await?;
                        Ok(RepositoryAuthentication::Basic(Some(token), user))
                    }
                    Err(err) => Err(err),
                }
            }
            AuthenticationRaw::NoIdentification => Ok(RepositoryAuthentication::NoIdentification),
            AuthenticationRaw::AuthorizationHeaderUnknown(scheme, value) => {
                debug!("Unknown Authorization Header: {} {}", scheme, value);
                Ok(RepositoryAuthentication::Other(scheme, value))
            }
        }
    }
}

fn unwrap_virtual_for_action<'a>(
    mut auth: &'a RepositoryAuthentication,
    action: RepositoryActions,
    repository_id: Uuid,
) -> (&'a RepositoryAuthentication, Uuid) {
    let mut effective_repository_id = repository_id;
    while let RepositoryAuthentication::VirtualRepository {
        virtual_repository_id,
        inner,
    } = auth
    {
        if matches!(action, RepositoryActions::Read) {
            effective_repository_id = *virtual_repository_id;
        }
        auth = inner;
    }
    (auth, effective_repository_id)
}
impl<S> FromRequestParts<S> for RepositoryAuthentication
where
    Pkgly: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthenticationError;
    #[instrument(
        name = "repository_auth_from_request",
        skip(parts, state),
        fields(project_module = "Authentication")
    )]
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let raw_extension = parts.extensions.get::<AuthenticationRaw>().cloned();
        let repo = Pkgly::from_ref(state);
        let Some(raw_auth) = raw_extension else {
            return Err(AuthenticationError::Unauthorized);
        };
        RepositoryAuthentication::from_raw(raw_auth, &repo).await
    }
}
async fn get_by_auth_token(
    token: &str,
    database: &PgPool,
) -> Result<(AuthToken, UserSafeData), AuthenticationError> {
    let token = AuthToken::get_by_token(token, database)
        .await?
        .ok_or(AuthenticationError::Unauthorized)?;
    if let Some(expires) = token.expires_at {
        if expires <= Utc::now().fixed_offset() {
            return Err(AuthenticationError::Unauthorized);
        }
    }
    let user = UserSafeData::get_by_id(token.user_id, database)
        .await?
        .ok_or(AuthenticationError::Unauthorized)?;
    Ok((token, user))
}

async fn get_by_auth_token_cached(
    token: &str,
    site: &Pkgly,
) -> Result<(AuthToken, UserSafeData), AuthenticationError> {
    // Try cache first
    if let Some(cached) = site.auth_token_cache.get(token).await {
        // Verify token hasn't expired
        if let Some(expires) = cached.0.expires_at {
            if expires <= Utc::now().fixed_offset() {
                // Token expired, remove from cache
                site.auth_token_cache.invalidate(token).await;
                return Err(AuthenticationError::Unauthorized);
            }
        }
        return Ok(cached);
    }

    // Cache miss, query database
    let result = get_by_auth_token(token, &site.database).await?;

    // Store in cache
    site.auth_token_cache
        .insert(token.to_string(), result.clone())
        .await;

    Ok(result)
}
