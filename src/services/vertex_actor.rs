use std::collections::VecDeque;

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::Serialize;
use snafu::{GenerateImplicitData, Location};
use tracing::{error, info};
use uuid::Uuid;
use yup_oauth2::ServiceAccountKey;

use crate::config::{CLEWDR_CONFIG, ClewdrConfig, VertexCredentialEntry};
use crate::error::ClewdrError;
use crate::persistence::StorageLayer;

#[derive(Debug, Clone)]
pub struct VertexCredential {
    pub id: Uuid,
    pub credential: ServiceAccountKey,
    pub count_403: u32,
}

impl From<VertexCredentialEntry> for VertexCredential {
    fn from(value: VertexCredentialEntry) -> Self {
        Self {
            id: value.id,
            credential: value.credential,
            count_403: value.count_403,
        }
    }
}

impl From<&VertexCredential> for VertexCredentialEntry {
    fn from(value: &VertexCredential) -> Self {
        VertexCredentialEntry {
            id: value.id,
            credential: value.credential.clone(),
            count_403: value.count_403,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct VertexCredentialStatus {
    pub id: String,
    pub client_email: Option<String>,
    pub project_id: Option<String>,
    #[serde(default)]
    pub count_403: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct VertexCredentialInfo {
    pub credentials: Vec<VertexCredentialStatus>,
}

#[derive(Debug)]
enum VertexActorMessage {
    Request(RpcReplyPort<Result<VertexCredential, ClewdrError>>),
    Submit(ServiceAccountKey),
    Return(VertexCredential),
    Delete(Uuid, RpcReplyPort<Result<(), ClewdrError>>),
    GetStatus(RpcReplyPort<VertexCredentialInfo>),
    Import(VertexCredentialEntry),
    Prune(Uuid),
}

struct VertexActor {
    storage: &'static dyn StorageLayer,
}

type VertexActorState = VecDeque<VertexCredential>;

impl VertexActor {
    fn save(state: &VertexActorState) {
        CLEWDR_CONFIG.rcu(|config| {
            let mut config = ClewdrConfig::clone(config);
            config.vertex.credentials = state.iter().map(VertexCredentialEntry::from).collect();
            config
        });

        tokio::spawn(async {
            if let Err(e) = CLEWDR_CONFIG.load().save().await {
                error!("Failed to persist vertex credentials: {}", e);
            } else {
                info!("Vertex credentials saved successfully");
            }
        });
    }

    fn insert_new(
        state: &mut VertexActorState,
        credential: ServiceAccountKey,
    ) -> Option<VertexCredential> {
        if state
            .iter()
            .any(|entry| entry.credential.client_email == credential.client_email)
        {
            info!("Vertex credential already exists");
            return None;
        }
        let entry = VertexCredential {
            id: Uuid::new_v4(),
            credential,
            count_403: 0,
        };
        state.push_back(entry.clone());
        Some(entry)
    }

    fn upsert_entry(state: &mut VertexActorState, entry: VertexCredentialEntry) {
        if let Some(pos) = state.iter().position(|c| c.id == entry.id) {
            state[pos] = VertexCredential::from(entry);
        } else {
            state.push_back(VertexCredential::from(entry));
        }
    }

    fn dispatch(state: &mut VertexActorState) -> Result<VertexCredential, ClewdrError> {
        let credential = state
            .pop_front()
            .ok_or(ClewdrError::NoVertexCredentialAvailable)?;
        let cloned = credential.clone();
        state.push_back(credential);
        Ok(cloned)
    }

    fn update_entry(
        state: &mut VertexActorState,
        credential: VertexCredential,
    ) -> Option<VertexCredentialEntry> {
        if let Some(pos) = state.iter_mut().position(|c| c.id == credential.id) {
            state[pos] = credential.clone();
            return Some(VertexCredentialEntry::from(&credential));
        }
        None
    }

    fn remove_entry(state: &mut VertexActorState, id: Uuid) -> Option<VertexCredentialEntry> {
        if let Some(pos) = state.iter().position(|c| c.id == id) {
            let removed = state.remove(pos).expect("remove by index");
            return Some(VertexCredentialEntry::from(&removed));
        }
        None
    }

    fn report(state: &VertexActorState) -> VertexCredentialInfo {
        let credentials = state
            .iter()
            .map(|entry| VertexCredentialStatus {
                id: entry.id.to_string(),
                client_email: Some(entry.credential.client_email.clone()),
                project_id: entry.credential.project_id.clone(),
                count_403: entry.count_403,
            })
            .collect();
        VertexCredentialInfo { credentials }
    }
}

impl Actor for VertexActor {
    type Msg = VertexActorMessage;
    type State = VertexActorState;
    type Arguments = Vec<VertexCredentialEntry>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(VecDeque::from_iter(
            args.into_iter().map(VertexCredential::from),
        ))
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            VertexActorMessage::Request(reply) => {
                let result = Self::dispatch(state);
                reply.send(result)?;
            }
            VertexActorMessage::Submit(credential) => {
                if let Some(entry) = Self::insert_new(state, credential) {
                    Self::save(state);
                    if self.storage.is_enabled() {
                        let storage = self.storage;
                        let entry_clone: VertexCredentialEntry = (&entry).into();
                        tokio::spawn(async move {
                            if let Err(e) = storage.persist_vertex_upsert(&entry_clone).await {
                                error!("Failed to upsert vertex credential: {}", e);
                            }
                        });
                    }
                }
            }
            VertexActorMessage::Return(credential) => {
                if let Some(entry) = Self::update_entry(state, credential) {
                    Self::save(state);
                    if self.storage.is_enabled() {
                        let storage = self.storage;
                        tokio::spawn(async move {
                            if let Err(e) = storage.persist_vertex_upsert(&entry).await {
                                error!("Failed to update vertex credential: {}", e);
                            }
                        });
                    }
                }
            }
            VertexActorMessage::Delete(id, reply) => {
                let result = Self::remove_entry(state, id);
                if let Some(entry) = result.clone() {
                    Self::save(state);
                    if self.storage.is_enabled() {
                        let storage = self.storage;
                        tokio::spawn(async move {
                            if let Err(e) = storage.delete_vertex_row(entry.id).await {
                                error!("Failed to delete vertex credential row: {}", e);
                            }
                        });
                    }
                    reply.send(Ok(()))?;
                } else {
                    reply.send(Err(ClewdrError::UnexpectedNone {
                        msg: "Vertex credential not found",
                    }))?;
                }
            }
            VertexActorMessage::GetStatus(reply) => {
                reply.send(Self::report(state))?;
            }
            VertexActorMessage::Import(entry) => {
                Self::upsert_entry(state, entry);
                Self::save(state);
            }
            VertexActorMessage::Prune(id) => {
                let _ = Self::remove_entry(state, id);
                Self::save(state);
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Self::save(state);
        Ok(())
    }
}

#[derive(Clone)]
pub struct VertexActorHandle {
    actor_ref: ActorRef<VertexActorMessage>,
}

impl VertexActorHandle {
    pub async fn start() -> Result<Self, ractor::SpawnErr> {
        let storage = crate::persistence::storage();
        let mut initial = CLEWDR_CONFIG
            .load()
            .vertex
            .credentials
            .to_vec();
        if storage.is_enabled()
            && let Ok(from_db) = storage.load_vertex_credentials().await
        {
            initial = from_db;
        }
        Self::start_with(initial, storage).await
    }

    pub async fn start_with(
        credentials: Vec<VertexCredentialEntry>,
        storage: &'static dyn StorageLayer,
    ) -> Result<Self, ractor::SpawnErr> {
        let (actor_ref, _join_handle) =
            Actor::spawn(None, VertexActor { storage }, credentials).await?;
        Ok(Self { actor_ref })
    }

    pub async fn request(&self) -> Result<VertexCredential, ClewdrError> {
        ractor::call!(self.actor_ref, VertexActorMessage::Request).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to request Vertex credential: {e}"),
            }
        })?
    }

    pub async fn submit(&self, credential: ServiceAccountKey) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, VertexActorMessage::Submit(credential)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to submit Vertex credential: {e}"),
            }
        })
    }

    pub async fn return_credential(&self, credential: VertexCredential) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, VertexActorMessage::Return(credential)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to return Vertex credential: {e}"),
            }
        })
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), ClewdrError> {
        ractor::call!(self.actor_ref, VertexActorMessage::Delete, id).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to delete Vertex credential: {e}"),
            }
        })?
    }

    pub async fn get_status(&self) -> Result<VertexCredentialInfo, ClewdrError> {
        ractor::call!(self.actor_ref, VertexActorMessage::GetStatus).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to fetch Vertex credential status: {e}"),
            }
        })
    }

    pub async fn import(&self, entry: VertexCredentialEntry) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, VertexActorMessage::Import(entry)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to import Vertex credential: {e}"),
            }
        })
    }

    pub async fn prune(&self, id: Uuid) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, VertexActorMessage::Prune(id)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to prune Vertex credential: {e}"),
            }
        })
    }
}
