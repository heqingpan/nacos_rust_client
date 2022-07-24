
use super::HostInfo;

#[derive(Debug,Clone)]
pub struct NacosConfig {
    pub config_host:Option<HostInfo>,
    pub naming_host:Option<HostInfo>,
}

impl NacosConfig {
    pub fn new(addr:&str) -> Self {
        let host = HostInfo::parse(addr);
        Self{
            config_host:Some(host.clone()),
            naming_host:Some(host.clone()),
        }
    }
}

pub struct NacosClient {
    config:NacosConfig,
}

impl NacosClient {
    pub fn new(addr:&str) -> Self{
        let config = NacosConfig::new(addr);
        Self {
            config
        }
    }
}

