use std::sync::Arc;

use actix::prelude::*;

use crate::client::{config_client::ConfigKey, naming_client::{Instance, QueryInstanceListParams, ServiceInstanceKey}};

#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<ConfigResponse>")]
pub enum ConfigRequest{
    GetConfig(ConfigKey),
    SetConfig(ConfigKey,String),
    DeleteConfig(ConfigKey),
    V1Listen(String),  // 兼容v1版本协议
    Listen(Vec<(ConfigKey,String)>,bool),  //(key,md5)
}

#[derive(Debug)]
pub enum ConfigResponse{
    ConfigValue(String,String), // (content,md5)
    ChangeKeys(Vec<ConfigKey>),
    None,
}

#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<NamingResponse>")]
pub enum NamingRequest {
    Register(Vec<Instance>),
    Unregister(Vec<Instance>),
    Subscribe(Vec<ServiceInstanceKey>),
    Unsubscribe(Vec<ServiceInstanceKey>),
    QueryInstance(Box<QueryInstanceListParams>),
    V1Heartbeat(Arc<String>),
}

#[derive(Debug,Default,Clone)]
pub struct ServiceResult{
    pub hosts:Vec<Arc<Instance>>,
    pub cache_millis:Option<i64>,
}

#[derive(Debug)]
pub enum NamingResponse{
    ServiceResult(ServiceResult),
    None,
}

#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum ConnCallbackMsg {
    ConfigChange(ConfigKey,String,String),
    InstanceChange(ServiceInstanceKey,Vec<Instance>),
}