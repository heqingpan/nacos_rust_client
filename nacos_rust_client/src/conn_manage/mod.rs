use actix::WeakAddr;

use crate::client::config_client::inner::ConfigInnerActor;


pub(crate) mod manage;
pub(crate) mod endpoint;
pub(crate) mod inner_conn;
pub(crate) mod breaker;
pub(crate) mod conn_msg;


#[derive(Default,Clone)]
pub struct NotifyCallbackAddr {
    pub(crate) config_inner_addr :Option<WeakAddr<ConfigInnerActor>>,
}