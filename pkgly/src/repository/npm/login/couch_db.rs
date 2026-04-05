use std::fmt::Debug;

use nr_core::{
    database::entities::user::auth_token::NewRepositoryToken, user::permissions::RepositoryActions,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, instrument};

use crate::{
    app::authentication::verify_login,
    repository::{
        RepoResponse, RepositoryRequest,
        npm::{NPMRegistryError, login::LoginResponse, utils::NpmRegistryExt},
    },
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CouchDBLoginRequest {
    pub name: String,
    pub password: String,
    pub email: Option<String>,
    #[serde(rename = "type")]
    pub login_type: String,
    #[serde(default)]
    pub roles: Vec<Value>,
}
impl Debug for CouchDBLoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CouchDBLogin")
            .field("name", &self.name)
            .field("password", &"********")
            .field("email", &self.email)
            .field("login_type", &self.login_type)
            .field("roles", &self.roles)
            .finish()
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CouchDBLoginResponse {
    pub ok: bool,
    pub id: String,
    pub name: String,
    pub token: String,
}

impl CouchDBLoginResponse {
    pub fn new(token: String, username: &str) -> Self {
        let id = format!("org.couchdb.user:{username}");
        Self {
            ok: true,
            id,
            name: username.to_string(),
            token,
        }
    }
}
/// Handles the login request for CouchDB
/// Required route is `/-/user/org.couchdb.user:<username>`
#[instrument(name = "npm_couch_db_login")]
pub async fn perform_login(
    repository: &impl NpmRegistryExt,
    request: RepositoryRequest,
) -> Result<RepoResponse, NPMRegistryError> {
    let path_as_string = request.path.to_string();
    let Some(source) = request
        .user_agent_as_string()?
        .map(|header| format!("NPM CLI ({})", header))
    else {
        return Ok(RepoResponse::forbidden());
    };
    let user_name = path_as_string.replace("-/user/org.couchdb.user:", "");
    let body = request.body.body_as_string().await?;
    debug!(user_name = %user_name, body.len = body.len(), "Handling PUT request");
    let login: CouchDBLoginRequest = serde_json::from_str(&body)?;
    debug!(user_name = %user_name, login.name = %login.name, "Handling PUT request");
    let user = match verify_login(login.name, login.password, repository.site().as_ref()).await {
        Ok(ok) => ok,
        Err(_err) => {
            return Ok(RepoResponse::forbidden());
        }
    };

    let (_, token) =
        NewRepositoryToken::new(user.id, source, repository.id(), RepositoryActions::all())
            .insert(repository.site().as_ref())
            .await?;
    let response = CouchDBLoginResponse::new(token, &user_name);
    return Ok(LoginResponse::ValidCouchDBLogin(response).into());
}

#[cfg(test)]
mod tests {
    use super::CouchDBLoginResponse;
    use serde_json::json;

    #[test]
    fn couch_login_response_matches_npm_cli_shape() {
        let resp = CouchDBLoginResponse::new("tok123".into(), "alice");
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(
            json,
            json!({
                "ok": true,
                "id": "org.couchdb.user:alice",
                "name": "alice",
                "token": "tok123"
            })
        );
    }
}
