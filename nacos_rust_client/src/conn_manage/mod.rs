use actix::WeakAddr;

use crate::client::{config_client::inner::ConfigInnerActor, naming_client::InnerNamingListener};

pub(crate) mod breaker;
pub mod conn_msg;
pub mod endpoint;
pub(crate) mod inner_conn;
pub mod manage;

#[derive(Default, Clone)]
pub struct NotifyCallbackAddr {
    pub(crate) config_inner_addr: Option<WeakAddr<ConfigInnerActor>>,
    pub(crate) naming_listener_addr: Option<WeakAddr<InnerNamingListener>>,
}
