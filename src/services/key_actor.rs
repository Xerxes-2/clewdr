use std::collections::{HashSet, VecDeque};

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::Serialize;
use snafu::{GenerateImplicitData, Location};
use tracing::{error, info};

use crate::{
    config::{CLEWDR_CONFIG, ClewdrConfig, KeyStatus},
    error::ClewdrError,
};

#[derive(Debug, Serialize, Clone)]
pub struct KeyStatusInfo {
    pub valid: Vec<KeyStatus>,
}

/// Messages that the KeyActor can handle
#[derive(Debug)]
enum KeyActorMessage {
    /// Return a Key
    Return(KeyStatus),
    /// Submit a new Key
    Submit(KeyStatus),
    /// Request to get a Key
    Request(RpcReplyPort<Result<KeyStatus, ClewdrError>>),
    /// Get all Key status information
    GetStatus(RpcReplyPort<KeyStatusInfo>),
    /// Delete a Key
    Delete(KeyStatus, RpcReplyPort<Result<(), ClewdrError>>),
}

/// KeyActor state - manages the collection of valid keys
type KeyActorState = VecDeque<KeyStatus>;

/// Key actor that handles key distribution and status tracking using Ractor
struct KeyActor;

impl KeyActor {
    /// Saves the current state of keys to the configuration
    fn save(state: &KeyActorState) {
        CLEWDR_CONFIG.rcu(|config| {
            let mut config = ClewdrConfig::clone(config);
            config.gemini_keys = state.iter().cloned().collect();
            config
        });

        tokio::spawn(async move {
            let result = CLEWDR_CONFIG.load().save().await;
            match result {
                Ok(_) => info!("Configuration saved successfully"),
                Err(e) => error!("Save task panicked: {}", e),
            }
        });
    }

    /// Dispatches a key for use
    fn dispatch(state: &mut KeyActorState) -> Result<KeyStatus, ClewdrError> {
        let key = state.pop_front().ok_or(ClewdrError::NoKeyAvailable)?;
        state.push_back(key.to_owned());
        Ok(key)
    }

    /// Collects (returns) a key back to the pool
    fn collect(state: &mut KeyActorState, key: KeyStatus) {
        let Some(pos) = state.iter().position(|k| *k == key) else {
            error!("Key not found in valid keys");
            return;
        };
        state[pos] = key;
    }

    /// Accepts a new key into the valid collection
    fn accept(state: &mut KeyActorState, key: KeyStatus) {
        if CLEWDR_CONFIG.load().gemini_keys.contains(&key) {
            info!("Key already exists");
            return;
        }
        state.push_back(key);
        Self::save(state);
    }

    /// Creates a report of all key statuses
    fn report(state: &KeyActorState) -> KeyStatusInfo {
        KeyStatusInfo {
            valid: state.iter().cloned().collect(),
        }
    }

    /// Deletes a key from the collection
    fn delete(state: &mut KeyActorState, key: KeyStatus) -> Result<(), ClewdrError> {
        let size_before = state.len();
        state.retain(|k| *k != key);

        if state.len() < size_before {
            Self::save(state);
            Ok(())
        } else {
            Err(ClewdrError::UnexpectedNone {
                msg: "Delete operation did not find the key",
            })
        }
    }
}

impl Actor for KeyActor {
    type Msg = KeyActorMessage;
    type State = KeyActorState;
    type Arguments = HashSet<KeyStatus>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state: Self::State = VecDeque::from_iter(args);
        Ok(state)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            KeyActorMessage::Return(key) => {
                Self::collect(state, key);
            }
            KeyActorMessage::Submit(key) => {
                #[cfg(feature = "db")]
                let k = key.clone();
                Self::accept(state, key);
                #[cfg(feature = "db")]
                if CLEWDR_CONFIG.load().is_db_mode() {
                    tokio::spawn(async move {
                        if let Err(e) = crate::persistence::persist_key_upsert(&k).await {
                            error!("Failed to upsert key: {}", e);
                        }
                    });
                }
            }
            KeyActorMessage::Request(reply_port) => {
                let result = Self::dispatch(state);
                reply_port.send(result)?;
            }
            KeyActorMessage::GetStatus(reply_port) => {
                let status_info = Self::report(state);
                reply_port.send(status_info)?;
            }
            KeyActorMessage::Delete(key, reply_port) => {
                let result = Self::delete(state, key.clone());
                let ok = result.is_ok();
                reply_port.send(result)?;
                if ok {
                    if crate::persistence::storage().is_enabled() {
                        tokio::spawn(async move {
                            if let Err(e) = crate::persistence::storage().delete_key_row(&key).await {
                                error!("Failed to delete key row: {}", e);
                            }
                        });
                    }
                }
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        KeyActor::save(state);
        Ok(())
    }
}

/// Handle for interacting with the KeyActor
#[derive(Clone)]
pub struct KeyActorHandle {
    actor_ref: ActorRef<KeyActorMessage>,
}

impl KeyActorHandle {
    /// Create a new KeyActor and return a handle to it
    pub async fn start() -> Result<Self, ractor::SpawnErr> {
        let (actor_ref, _join_handle) =
            Actor::spawn(None, KeyActor, CLEWDR_CONFIG.load().gemini_keys.clone()).await?;
        Ok(Self { actor_ref })
    }

    /// Request a key from the key actor
    pub async fn request(&self) -> Result<KeyStatus, ClewdrError> {
        ractor::call!(self.actor_ref, KeyActorMessage::Request).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with KeyActor for request operation: {e}"),
            }
        })?
    }

    /// Return a key to the key actor
    pub async fn return_key(&self, key: KeyStatus) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, KeyActorMessage::Return(key)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with KeyActor for return operation: {e}"),
            }
        })
    }

    /// Submit a new key to the key actor
    pub async fn submit(&self, key: KeyStatus) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, KeyActorMessage::Submit(key)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with KeyActor for submit operation: {e}"),
            }
        })
    }

    /// Get status information about all keys
    pub async fn get_status(&self) -> Result<KeyStatusInfo, ClewdrError> {
        ractor::call!(self.actor_ref, KeyActorMessage::GetStatus).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with KeyActor for get status operation: {e}"),
            }
        })
    }

    /// Delete a key from the key actor
    pub async fn delete_key(&self, key: KeyStatus) -> Result<(), ClewdrError> {
        ractor::call!(self.actor_ref, KeyActorMessage::Delete, key).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with KeyActor for delete operation: {e}"),
            }
        })?
    }
}
