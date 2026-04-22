use std::{future::Future, net::SocketAddr, task::Poll};

use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use futures::{SinkExt, Stream};
use nr_core::{
    repository::{browse::BrowseFile, project::ProjectResolution},
    storage::StoragePath,
};
use nr_storage::{
    DirectoryListStream, DynDirectoryListStream, DynStorage, EmptyDirectoryListStream, FileType,
    Storage, StorageFileMeta,
};
use opentelemetry::trace::Status;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::EnumIs;
use tokio::select;
use tracing::{
    Instrument as _, Level, Span, debug, debug_span, event,
    field::{Empty, debug},
    info, instrument, warn,
};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::BrowseStreamPrimaryData;
use crate::{
    app::{
        Pkgly,
        authentication::ws::{WebSocketAuthentication, WebSocketAuthenticationMessage},
    },
    audit::{AuditActor, AuditMetadata, AuditOutcome, emit_named_audit_log},
    error::InternalError,
    repository::{
        DynRepository, Repository, RepositoryAuthConfig, docker::metadata::resolve_browse_path,
        utils::can_read_repository_with_auth,
    },
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebsocketIncomingMessage {
    ListDirectory(StoragePath),
    Authentication(WebSocketAuthenticationMessage),
}
#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum WebsocketOutgoingMessage {
    DirectoryItem(BrowseFile),
    OpenedDirectory(BrowseStreamPrimaryData),
    EndOfDirectory,
    Error(String),
    Unauthorized,
    Authorized,
}
impl From<WebsocketOutgoingMessage> for Message {
    fn from(message: WebsocketOutgoingMessage) -> Self {
        let payload = encode_outgoing_message(&message);
        Message::Text(Utf8Bytes::from(payload))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIs)]
pub enum WSPermissionsStatus {
    Unauthorized,
    Pending,
    Authorized,
}

fn encode_outgoing_message(message: &WebsocketOutgoingMessage) -> String {
    match serde_json::to_string(message) {
        Ok(json) => json,
        Err(err) => {
            warn!(?err, "Failed to serialize outgoing WebSocket message");
            json!({
                "type": "Error",
                "data": "Internal server error"
            })
            .to_string()
        }
    }
}

#[cfg(test)]
mod tests;

pub struct BrowseWSState {
    pub repository: DynRepository,
    pub site: Pkgly,
    pub authentication: Option<WebSocketAuthentication>,
    pub access_status: WSPermissionsStatus,
    pub active_path: StoragePathStream,
    pub auth_config: RepositoryAuthConfig,
}

fn ws_audit_actor(authentication: Option<&WebSocketAuthentication>) -> AuditActor {
    match authentication {
        Some(WebSocketAuthentication::AuthToken { user, .. })
        | Some(WebSocketAuthentication::Session { user, .. }) => AuditActor {
            username: user.username.as_ref().to_string(),
            user_id: Some(user.id),
        },
        None => AuditActor::default(),
    }
}

fn ws_audit_metadata(
    repository: &DynRepository,
    authentication: Option<&WebSocketAuthentication>,
    path: Option<&StoragePath>,
) -> AuditMetadata {
    let storage = repository.get_storage();
    let storage_config = storage.storage_config();
    AuditMetadata {
        actor: ws_audit_actor(authentication),
        resource_kind: Some("repository".to_string()),
        resource_id: Some(repository.id().to_string()),
        resource_name: Some(repository.name()),
        repository_id: Some(repository.id().to_string()),
        storage_id: Some(storage_config.storage_config.storage_id.to_string()),
        path: path.map(ToString::to_string),
        ..Default::default()
    }
}

impl BrowseWSState {
    pub fn new(repository: DynRepository, site: Pkgly, auth_config: RepositoryAuthConfig) -> Self {
        let active_path = StoragePathStream::new(repository.clone());
        BrowseWSState {
            repository,
            site,
            authentication: None,
            access_status: WSPermissionsStatus::Pending,
            active_path,
            auth_config,
        }
    }

    async fn handle_message(
        &mut self,
        message: Result<Message, axum::Error>,
        socket: &mut WebSocket,
    ) -> Result<bool, InternalError> {
        let span = debug_span!(
            "Handle message",
            message = debug(&message),
            "message.type" = Empty,
        );
        let span_for_instrument = span.clone();
        async move {
            let message = match message {
                Ok(message) => message,
                Err(e) => {
                    span.set_status(Status::error(e.to_string()));
                    let message = WebsocketOutgoingMessage::Error(e.to_string());
                    socket.send(message.into()).await?;
                    return Ok(true);
                }
            };
            let incoming_message = match message {
                Message::Close(_) => {
                    span.record("message.type", "Close");
                    return Ok(true);
                }
                Message::Ping(_) | Message::Pong(_) => {
                    span.record("message.type", "Ping/Pong");
                    return Ok(false);
                }
                Message::Binary(bytes) => {
                    span.record("message.type", "Binary");
                    let message: WebsocketIncomingMessage = match serde_json::from_slice(&bytes) {
                        Ok(message) => message,
                        Err(e) => {
                            span.set_status(Status::error(e.to_string()));
                            event!(Level::ERROR, ?e, "Failed to parse message");
                            let message = WebsocketOutgoingMessage::Error(e.to_string());
                            socket.send(message.into()).await?;
                            return Ok(false);
                        }
                    };
                    message
                }
                Message::Text(content) => {
                    span.record("message.type", "Text");
                    let message: WebsocketIncomingMessage = match serde_json::from_str(&content) {
                        Ok(message) => message,
                        Err(e) => {
                            span.set_status(Status::error(e.to_string()));
                            event!(Level::ERROR, ?e, "Failed to parse message");
                            let message = WebsocketOutgoingMessage::Error(e.to_string());
                            socket.send(message.into()).await?;
                            return Ok(false);
                        }
                    };
                    message
                }
            };

            debug!(?incoming_message, "Received message");
            match incoming_message {
                WebsocketIncomingMessage::ListDirectory(path) => {
                    let audit_path = path.clone();
                    if self.access_status != WSPermissionsStatus::Authorized {
                        if !can_read_repository_with_auth(
                            &self.authentication,
                            self.repository.visibility(),
                            self.repository.id(),
                            self.site.as_ref(),
                            &self.auth_config,
                        )
                        .await?
                        {
                            info!(?self.authentication, "Access denied. Closing connection");
                            emit_named_audit_log(
                                &span,
                                "repository.browse_ws.list_directory",
                                AuditOutcome::Denied,
                                &ws_audit_metadata(
                                    &self.repository,
                                    self.authentication.as_ref(),
                                    Some(&audit_path),
                                ),
                            );
                            self.access_status = WSPermissionsStatus::Unauthorized;
                            let message = WebsocketOutgoingMessage::Unauthorized;
                            socket.send(message.into()).await?;
                            return Ok(true);
                        } else {
                            debug!("Access granted");
                            self.access_status = WSPermissionsStatus::Authorized;
                        }
                    }
                    match self.active_path.change_directory(path).await {
                        Ok(ok) => {
                            event!(Level::DEBUG, ?ok, "Opened directory");
                            emit_named_audit_log(
                                &span,
                                "repository.browse_ws.list_directory",
                                AuditOutcome::Success,
                                &ws_audit_metadata(
                                    &self.repository,
                                    self.authentication.as_ref(),
                                    Some(&audit_path),
                                ),
                            );
                            let message = WebsocketOutgoingMessage::OpenedDirectory(ok);
                            socket.send(message.into()).await?;
                        }
                        Err(err) => {
                            span.set_status(Status::error(err.to_string()));
                            let message = WebsocketOutgoingMessage::Error(err.to_string());
                            event!(Level::ERROR, ?err, "Failed to open directory");
                            socket.send(message.into()).await?;
                        }
                    }
                    Ok(false)
                }
                WebsocketIncomingMessage::Authentication(auth) => {
                    let auth = auth.attempt_login(&self.site).await;

                    match auth {
                        Ok(auth) => {
                            event!(Level::DEBUG, ?auth, "Authenticated");
                            self.authentication = Some(auth);
                            if !can_read_repository_with_auth(
                                &self.authentication,
                                self.repository.visibility(),
                                self.repository.id(),
                                self.site.as_ref(),
                                &self.auth_config,
                            )
                            .await?
                            {
                                info!(?self.authentication, "Access denied. Closing connection");
                                emit_named_audit_log(
                                    &span,
                                    "repository.browse_ws.authenticate",
                                    AuditOutcome::Denied,
                                    &ws_audit_metadata(
                                        &self.repository,
                                        self.authentication.as_ref(),
                                        None,
                                    ),
                                );

                                self.access_status = WSPermissionsStatus::Unauthorized;
                                let message = WebsocketOutgoingMessage::Unauthorized;
                                socket.send(message.into()).await?;
                                return Ok(true);
                            } else {
                                self.access_status = WSPermissionsStatus::Authorized;
                            }
                            emit_named_audit_log(
                                &span,
                                "repository.browse_ws.authenticate",
                                AuditOutcome::Success,
                                &ws_audit_metadata(
                                    &self.repository,
                                    self.authentication.as_ref(),
                                    None,
                                ),
                            );
                            let message = WebsocketOutgoingMessage::Authorized;
                            socket.send(message.into()).await?;
                            Ok(false)
                        }
                        Err(err) => {
                            span.set_status(Status::error(err.to_string()));
                            event!(Level::ERROR, ?err, "Failed to authenticate");
                            emit_named_audit_log(
                                &span,
                                "repository.browse_ws.authenticate",
                                AuditOutcome::Denied,
                                &ws_audit_metadata(&self.repository, None, None),
                            );
                            let message = WebsocketOutgoingMessage::Error(err.to_string());
                            socket.send(message.into()).await?;
                            Ok(true)
                        }
                    }
                }
            }
        }
        .instrument(span_for_instrument)
        .await
    }
    async fn handle_next_item(
        &mut self,
        socket: &mut WebSocket,
        next_item: Result<Option<StorageFileMeta<FileType>>, InternalError>,
    ) -> Result<bool, InternalError> {
        let span = debug_span!("Handle Next Item", next_item = debug(&next_item),);
        let span_for_instrument = span.clone();
        async move {
            match next_item {
                Ok(Some(file)) => {
                    let message = WebsocketOutgoingMessage::DirectoryItem(file.into());
                    debug!(?message, "Sending message");
                    socket.send(message.into()).await?;
                    span.set_status(Status::Ok);
                }
                Ok(None) => {
                    let message = WebsocketOutgoingMessage::EndOfDirectory;
                    socket.send(message.into()).await?;
                    span.set_status(Status::Ok);
                }
                Err(e) => {
                    event!(Level::ERROR, ?e, "Failed to get next item");
                    let message = WebsocketOutgoingMessage::Error(e.to_string());
                    socket.send(message.into()).await?;
                    span.set_status(Status::error(e.to_string()));
                    return Ok(true);
                }
            }
            Ok(false)
        }
        .instrument(span_for_instrument)
        .await
    }
}

pub(super) async fn handle_socket(
    mut socket: WebSocket,
    who: SocketAddr,
    repository: DynRepository,
    site: Pkgly,
    span: Span,
) {
    let span_for_connect = span.clone();
    async move {
        info!(?who, "New websocket connection");
        emit_named_audit_log(
            &span_for_connect,
            "repository.browse_ws.connect",
            AuditOutcome::Success,
            &ws_audit_metadata(&repository, None, None),
        );

        if let Err(socket) = socket.send(Message::Ping(Default::default())).await {
            event!(Level::ERROR, ?socket, "Failed to send ping");
            return;
        }
        let auth_config = match site.get_repository_auth_config(repository.id()).await {
            Ok(config) => config,
            Err(err) => {
                event!(
                    Level::ERROR,
                    ?err,
                    "Failed to load repository auth config for browse websocket"
                );
                RepositoryAuthConfig::default()
            }
        };

        let mut state = BrowseWSState::new(repository, site, auth_config);
        loop {
            select! {
                 message = socket.recv() => {
                    let Some(message) = message else{
                        event!(Level::DEBUG, "End of stream");
                        break;
                    };

                    match state.handle_message(message,  &mut socket).await  {
                        Ok(ok) if ok => {
                            break;
                        },
                        Ok(_) => {},
                        Err(err) => {
                            event!(Level::ERROR, ?err, "Failed to handle message");
                            break;
                        },
                    }
                 }
                 next_item = state.active_path.next_item() => {
                    debug!(?next_item, "Next item");
                    match state.handle_next_item(&mut socket, next_item).await {
                        Ok(ok) if ok => {
                            break;
                        },
                        Ok(_) => {},
                        Err(err) => {
                            event!(Level::ERROR, ?err, "Failed to handle next item");
                            break;
                        },
                    }
                 }
            }
        }
        if let Err(err) = socket.close().await {
            event!(Level::ERROR, ?err, "Failed to close websocket connection");
        }
        info!("Closing websocket connection");
    }
    .instrument(span)
    .await
}
#[derive(Debug)]
pub struct StoragePathStream {
    repository: DynRepository,
    path: StoragePath,
    storage: DynStorage,
    current: DynDirectoryListStream,
    sent_end_of_directory: bool,
}

impl StoragePathStream {
    pub fn new(repository: DynRepository) -> Self {
        let storage = repository.get_storage();
        StoragePathStream {
            path: StoragePath::from("/"),
            repository,
            storage,
            sent_end_of_directory: true,
            current: DynDirectoryListStream::new(EmptyDirectoryListStream),
        }
    }

    #[instrument(skip(self), fields(project, number_of_files))]
    pub async fn change_directory(
        &mut self,
        path: StoragePath,
    ) -> Result<BrowseStreamPrimaryData, InternalError> {
        let span = Span::current();
        self.path = path;
        let path_display = self.path.to_string();
        let path_display = if path_display.is_empty() {
            "/".to_string()
        } else {
            path_display
        };
        info!(path = %path_display, "Changing directory");
        self.sent_end_of_directory = false;
        let target_path = match &self.repository {
            DynRepository::Docker(_) => {
                resolve_browse_path(&self.storage, self.repository.id(), &self.path)
                    .await
                    .map_err(InternalError::from)?
            }
            _ => self.path.clone(),
        };

        self.current = self
            .storage
            .stream_directory(self.repository.id(), &target_path)
            .await?
            .unwrap_or_else(|| {
                warn!("Empty directory stream");
                DynDirectoryListStream::new(EmptyDirectoryListStream)
            });

        let project_resolution = {
            event!(Level::DEBUG, "Checking for project and version");
            match self
                .repository
                .resolve_project_and_version_for_path(&self.path)
                .await
            {
                Ok(ok) => ok,
                Err(err) => {
                    event!(
                        Level::ERROR,
                        ?err,
                        path = ?self.path,
                        "Failed to resolve project and version for path"
                    );
                    ProjectResolution::default()
                }
            }
        };
        span.record("project", debug(&project_resolution));
        span.record("number_of_files", self.current.number_of_files());
        let data = BrowseStreamPrimaryData {
            project_resolution: Some(project_resolution),
            number_of_files: self.current.number_of_files() as usize,
        };
        debug!("Opened directory");

        Ok(data)
    }
    pub fn next_item(&mut self) -> NextItem<'_> {
        NextItem {
            stream: &mut self.current,
            sent_end_of_dir: &mut self.sent_end_of_directory,
        }
    }
}
#[pin_project]
pub struct NextItem<'a> {
    #[pin]
    stream: &'a mut DynDirectoryListStream,

    sent_end_of_dir: &'a mut bool,
}
impl Future for NextItem<'_> {
    type Output = Result<Option<StorageFileMeta<FileType>>, InternalError>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        match this.stream.poll_next(cx) {
            std::task::Poll::Ready(Some(file)) => {
                std::task::Poll::Ready(file.map_err(InternalError::from))
            }
            std::task::Poll::Ready(None) => {
                // Only Send an end of dictory message once. Otherwise we will keep sending Pending
                if **this.sent_end_of_dir {
                    Poll::Pending
                } else {
                    **this.sent_end_of_dir = true;
                    std::task::Poll::Ready(Ok(None))
                }
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
