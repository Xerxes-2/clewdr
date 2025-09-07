use std::collections::{HashSet, VecDeque};

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::Serialize;
// no snafu imports needed in this module
use tracing::{error, info, warn};

use crate::{
    config::{ClewdrConfig, CliTokenStatus, CLEWDR_CONFIG},
    error::ClewdrError,
};

#[derive(Debug, Serialize, Clone)]
pub struct CliTokenStatusInfo {
    pub valid: Vec<CliTokenStatus>,
}

#[derive(Debug)]
enum CliTokenMsg {
    Return(CliTokenStatus),
    Submit(CliTokenStatus),
    Request(RpcReplyPort<Result<CliTokenStatus, ClewdrError>>),
    GetStatus(RpcReplyPort<CliTokenStatusInfo>),
    Delete(CliTokenStatus, RpcReplyPort<Result<(), ClewdrError>>),
}

type CliTokenState = VecDeque<CliTokenStatus>;

struct CliTokenActor;

impl CliTokenActor {
    fn save(state: &CliTokenState) {
        CLEWDR_CONFIG.rcu(|config| {
            let mut config = ClewdrConfig::clone(config);
            config.cli_tokens = state.iter().cloned().collect();
            config
        });
        tokio::spawn(async move {
            let result = CLEWDR_CONFIG.load().save().await;
            match result {
                Ok(_) => info!("Configuration saved successfully (cli tokens)"),
                Err(e) => error!("Save task failed: {}", e),
            }
        });
    }

    fn dispatch(state: &mut CliTokenState) -> Result<CliTokenStatus, ClewdrError> {
        let tok = state.pop_front().ok_or(ClewdrError::NoKeyAvailable)?;
        state.push_back(tok.to_owned());
        Ok(tok)
    }

    fn collect(state: &mut CliTokenState, tok: CliTokenStatus) {
        if let Some(pos) = state.iter().position(|t| *t == tok) {
            state[pos] = tok;
        } else {
            warn!("Token not found in pool when returning");
        }
        Self::save(state);
    }

    fn accept(state: &mut CliTokenState, tok: CliTokenStatus) {
        if CLEWDR_CONFIG.load().cli_tokens.contains(&tok) {
            info!("Token already exists");
            return;
        }
        state.push_back(tok);
        Self::save(state);
    }

    fn report(state: &CliTokenState) -> CliTokenStatusInfo {
        CliTokenStatusInfo { valid: state.iter().cloned().collect() }
    }

    fn delete(state: &mut CliTokenState, tok: CliTokenStatus) -> Result<(), ClewdrError> {
        let size = state.len();
        state.retain(|t| *t != tok);
        if state.len() < size { Self::save(state); Ok(()) } else { Err(ClewdrError::UnexpectedNone { msg: "Token not found" }) }
    }
}

impl Actor for CliTokenActor {
    type Msg = CliTokenMsg;
    type State = CliTokenState;
    type Arguments = HashSet<CliTokenStatus>;

    async fn pre_start(&self, _me: ActorRef<Self::Msg>, args: Self::Arguments) -> Result<Self::State, ActorProcessingErr> {
        Ok(VecDeque::from_iter(args))
    }

    async fn handle(&self, _myself: ActorRef<Self::Msg>, msg: Self::Msg, state: &mut Self::State) -> Result<(), ActorProcessingErr> {
        match msg {
            CliTokenMsg::Return(tok) => Self::collect(state, tok),
            CliTokenMsg::Submit(tok) => Self::accept(state, tok),
            CliTokenMsg::Request(port) => { let res = Self::dispatch(state); port.send(res)?; }
            CliTokenMsg::GetStatus(port) => { port.send(Self::report(state))?; }
            CliTokenMsg::Delete(tok, port) => { port.send(Self::delete(state, tok))?; }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct CliTokenActorHandle(ActorRef<CliTokenMsg>);

impl CliTokenActorHandle {
    pub async fn start() -> Result<Self, ClewdrError> {
        let (actor, _) = ractor::Actor::spawn(None, CliTokenActor, CLEWDR_CONFIG.load().cli_tokens.clone())
            .await
            .map_err(|e| ClewdrError::Whatever { message: "Start CliTokenActor".into(), source: Some(Box::new(e)) })?;
        Ok(Self(actor))
    }
    pub async fn request(&self) -> Result<CliTokenStatus, ClewdrError> {
        ractor::call!(self.0, CliTokenMsg::Request).map_err(|e| ClewdrError::Whatever { message: "request cli token".into(), source: Some(Box::new(e)) })?
    }
    pub async fn submit(&self, tok: CliTokenStatus) -> Result<(), ClewdrError> {
        ractor::cast!(self.0, CliTokenMsg::Submit(tok)).map_err(|e| ClewdrError::Whatever { message: "submit cli token".into(), source: Some(Box::new(e)) })
    }
    pub async fn return_token(&self, tok: CliTokenStatus) -> Result<(), ClewdrError> {
        ractor::cast!(self.0, CliTokenMsg::Return(tok)).map_err(|e| ClewdrError::Whatever { message: "return cli token".into(), source: Some(Box::new(e)) })
    }
    pub async fn get_status(&self) -> Result<CliTokenStatusInfo, ClewdrError> {
        ractor::call!(self.0, CliTokenMsg::GetStatus).map_err(|e| ClewdrError::Whatever { message: "get cli tokens".into(), source: Some(Box::new(e)) })
    }
    pub async fn delete(&self, tok: CliTokenStatus) -> Result<(), ClewdrError> {
        ractor::call!(self.0, CliTokenMsg::Delete, tok).map_err(|e| ClewdrError::Whatever { message: "delete cli token".into(), source: Some(Box::new(e)) })?
    }
}
