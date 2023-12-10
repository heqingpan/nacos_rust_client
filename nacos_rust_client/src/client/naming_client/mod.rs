use std::sync::Arc;
use std::time::Duration;
//use actix::prelude::*;
use inner_mem_cache::TimeoutSet;
use std::collections::HashMap;

mod api_model;
mod client;
mod listerner;
mod register;
mod request_client;
mod udp_actor;

pub use request_client::InnerNamingRequestClient;

pub use api_model::{
    BeatInfo, BeatRequest, InstanceVO, InstanceWebParams, InstanceWebQueryListParams, NamingUtils,
    QueryListResult,
};
pub use client::NamingClient;
pub use listerner::{
    InnerNamingListener, InstanceDefaultListener, InstanceListener, NamingListenerCmd,
    NamingQueryCmd, NamingQueryResult,
};
pub use register::{InnerNamingRegister, NamingRegisterCmd};
pub use udp_actor::{UdpDataCmd, UdpWorker};

pub(crate) static REGISTER_PERIOD: u64 = 5000u64;

#[derive(Debug, Clone, Default)]
pub struct Instance {
    //pub id:String,
    pub ip: String,
    pub port: u32,
    pub weight: f32,
    pub enabled: bool,
    pub healthy: bool,
    pub ephemeral: bool,
    pub cluster_name: String,
    pub service_name: String,
    pub group_name: String,
    pub metadata: Option<HashMap<String, String>>,
    pub namespace_id: String,
    //pub app_name:String,
    pub beat_string: Option<Arc<String>>,
}

impl Instance {
    pub fn new_simple(ip: &str, port: u32, service_name: &str, group_name: &str) -> Self {
        Self::new(ip, port, service_name, group_name, "", "", None)
    }

    pub fn new(
        ip: &str,
        port: u32,
        service_name: &str,
        group_name: &str,
        cluster_name: &str,
        namespace_id: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Self {
        let cluster_name = if cluster_name.len() == 0 {
            "DEFAULT".to_owned()
        } else {
            cluster_name.to_owned()
        };
        let group_name = if group_name.len() == 0 {
            "DEFAULT_GROUP".to_owned()
        } else {
            group_name.to_owned()
        };
        let namespace_id = if namespace_id.len() == 0 {
            "public".to_owned()
        } else {
            namespace_id.to_owned()
        };
        Self {
            ip: ip.to_owned(),
            port,
            weight: 1.0f32,
            enabled: true,
            healthy: true,
            ephemeral: true,
            cluster_name,
            service_name: service_name.to_owned(),
            group_name,
            metadata,
            namespace_id,
            beat_string: None,
        }
    }

    pub fn generate_key(&self) -> String {
        format!(
            "{}#{}#{}#{}#{}#{}",
            &self.ip,
            &self.port,
            &self.cluster_name,
            &self.service_name,
            &self.group_name,
            &self.namespace_id
        )
    }

    pub fn get_service_named(&self) -> String {
        format!("{}@@{}", self.group_name, self.service_name)
    }

    fn generate_beat_info(&self) -> BeatInfo {
        let mut beat = BeatInfo::default();
        beat.cluster = self.cluster_name.to_owned();
        beat.ip = self.ip.to_owned();
        beat.port = self.port;
        if let Some(metadata) = &self.metadata {
            beat.metadata = metadata.clone();
        }
        beat.period = REGISTER_PERIOD as i64;
        beat.scheduled = false;
        beat.service_name = self.get_service_named();
        beat.stopped = false;
        beat.weight = self.weight;
        beat
        //serde_json::to_string(&beat).unwrap()
    }

    fn generate_beat_request(&self) -> BeatRequest {
        let mut req = BeatRequest::default();
        let beat = self.generate_beat_info();
        req.beat = serde_json::to_string(&beat).unwrap();
        req.namespace_id = self.namespace_id.to_owned();
        req.service_name = beat.service_name;
        req.cluster_name = beat.cluster;
        req.group_name = self.group_name.to_owned();
        req
    }

    fn generate_beat_request_urlencode(&self) -> String {
        let req = self.generate_beat_request();
        serde_urlencoded::to_string(&req).unwrap()
    }

    pub fn init_beat_string(&mut self) {
        self.beat_string = Some(Arc::new(self.generate_beat_request_urlencode()));
    }

    pub fn to_web_params(&self) -> InstanceWebParams {
        let mut params = InstanceWebParams::default();
        params.ip = self.ip.to_owned();
        params.port = self.port;
        params.namespace_id = self.namespace_id.to_owned();
        params.weight = self.weight;
        params.enabled = true;
        params.healthy = true;
        params.ephemeral = true;
        if let Some(metadata) = &self.metadata {
            params.metadata = serde_json::to_string(metadata).unwrap();
        }
        params.cluster_name = self.cluster_name.to_owned();
        params.service_name = self.get_service_named();
        params.group_name = self.group_name.to_owned();
        params
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServiceInstanceKey {
    pub namespace_id: Option<String>,
    pub group_name: String,
    pub service_name: String,
}

impl ServiceInstanceKey {
    pub fn new(service_name: &str, group_name: &str) -> Self {
        Self {
            group_name: group_name.to_owned(),
            service_name: service_name.to_owned(),
            ..Default::default()
        }
    }

    pub fn new_with_namespace(&mut self, namespace_id: &str) {
        self.namespace_id = Some(namespace_id.to_owned());
    }

    pub fn get_key(&self) -> String {
        NamingUtils::get_group_and_service_name(&self.service_name, &self.group_name)
    }

    pub fn from_str(key_str: &str) -> Self {
        let mut s = Self::new("", "");
        if let Some((group, service)) = NamingUtils::split_group_and_serivce_name(&key_str) {
            s.group_name = group;
            s.service_name = service;
        }
        s
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueryInstanceListParams {
    pub namespace_id: String,
    pub group_name: String,
    pub service_name: String,
    pub clusters: Option<Vec<String>>,
    pub healthy_only: bool,
    client_ip: Option<String>,
    udp_port: Option<u16>,
}

impl QueryInstanceListParams {
    pub fn new(
        namespace_id: &str,
        group_name: &str,
        service_name: &str,
        clusters: Option<Vec<String>>,
        healthy_only: bool,
    ) -> Self {
        Self {
            namespace_id: namespace_id.to_owned(),
            group_name: group_name.to_owned(),
            service_name: service_name.to_owned(),
            clusters: clusters,
            healthy_only,
            client_ip: None,
            udp_port: None,
        }
    }

    pub fn new_simple(service_name: &str, group_name: &str) -> Self {
        Self::new("", group_name, service_name, None, true)
    }

    pub fn new_by_serivce_key(key: &ServiceInstanceKey) -> Self {
        Self::new_simple(&key.service_name, &key.group_name)
    }

    pub fn get_key(&self) -> String {
        NamingUtils::get_group_and_service_name(&self.service_name, &self.group_name)
    }

    pub fn build_key(&self) -> ServiceInstanceKey {
        ServiceInstanceKey {
            namespace_id: Some(self.namespace_id.clone()),
            group_name: self.group_name.clone(),
            service_name: self.service_name.clone(),
        }
    }

    fn to_web_params(&self) -> InstanceWebQueryListParams {
        let mut params = InstanceWebQueryListParams::default();
        params.namespace_id = self.namespace_id.to_owned();
        params.group_name = self.group_name.to_owned();
        params.service_name =
            NamingUtils::get_group_and_service_name(&self.service_name, &self.group_name);
        if let Some(clusters) = &self.clusters {
            params.clusters = clusters.join(",")
        }
        params.healthy_only = self.healthy_only;
        params.client_ip = self.client_ip.clone();
        params.udp_port = self.udp_port;
        params
    }
}
