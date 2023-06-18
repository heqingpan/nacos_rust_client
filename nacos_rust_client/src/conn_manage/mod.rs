use actix::WeakAddr;

use crate::client::{config_client::inner::ConfigInnerActor, naming_client::InnerNamingListener};


pub mod manage;
pub mod endpoint;
pub(crate) mod inner_conn;
pub(crate) mod breaker;
pub mod conn_msg;


#[derive(Default,Clone)]
pub struct NotifyCallbackAddr {
    pub(crate) config_inner_addr :Option<WeakAddr<ConfigInnerActor>>,
    pub(crate) naming_listener_addr :Option<WeakAddr<InnerNamingListener>>,
}