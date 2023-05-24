use std::sync::Arc;

use actix::prelude::*;

use crate::client::{config_client::ConfigKey, naming_client::{Instance, QueryInstanceListParams, ServiceInstanceKey}};

#[derive(Debug)]
pub enum ConfigRequest{
    GetConfig(ConfigKey),
    SetConfig(ConfigKey,String),
    DeleteConfig(ConfigKey),
    V1Listen(String),  // 兼容v1版本协议
    Listen(Vec<(ConfigKey,String)>,bool),  //(key,md5)
}

#[derive(Debug)]
pub enum ConfigResponse{
    ConfigValue(String,String),
    ChangeKeys(Vec<ConfigKey>),
}

#[derive(Debug)]
pub enum NamingRequest {
    Register(Vec<Instance>),
    Unregister(Vec<Instance>),
    Subscribe(Vec<ServiceInstanceKey>),
    Unsubscribe(Vec<ServiceInstanceKey>),
    QueryInstance(Box<QueryInstanceListParams>),
    V1Heartbeat(Arc<String>),
}

#[derive(Debug)]
pub enum NamingResponse{
    Instances(Vec<Arc<Instance>>),
}


#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<ConnMsgResult>")]
pub enum ConnCmd {
    ConfigCmd(ConfigRequest),
    NamingCmd(NamingRequest),
}

pub enum ConnMsgResult {
    ConfigResult(ConfigResponse),
    NamingRequest(ConfigResponse),
    None,
}


#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum ConnCallbackMsg {
    ConfigChange(ConfigKey,String,String),
    InstanceChange(ServiceInstanceKey,Vec<Instance>),
}