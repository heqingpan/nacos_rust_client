use std::collections::HashMap;
use serde::{Serialize,Deserialize};


#[derive(Debug,Serialize,Deserialize,Default)]
pub struct BeatInfo {
    pub cluster:String,
    pub ip:String,
    pub port:u32,
    pub metadata:HashMap<String,String>,
    pub period:i64,
    pub scheduled:bool,
    pub serviceName:String,
    pub stopped:bool,
    pub weight:f32,
}

#[derive(Debug,Serialize,Deserialize,Default)]
pub struct BeatRequest{
    pub namespaceId:String,
    pub serviceName:String,
    pub clusterName:String,
    pub groupName:String,
    pub ephemeral:Option<String>,
    pub beat:String,
}

#[derive(Debug,Serialize,Deserialize,Default)]
pub struct InstanceWebParams {
    pub ip:String,
    pub port:u32,
    pub namespaceId:String,
    pub weight: f32,
    pub enabled:bool,
    pub healthy:bool,
    pub ephemeral:bool,
    pub metadata:String,
    pub clusterName:String,
    pub serviceName:String,
    pub groupName:String,
}