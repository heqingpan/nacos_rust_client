use std::collections::HashMap;

pub mod nacos_client;
pub mod config_client;
pub mod naming_client;

pub mod utils;

use crypto::digest::Digest;

pub use self::nacos_client::NacosClient;
pub use self::config_client::ConfigClient;

#[derive(Debug,Clone)]
pub struct HostInfo {
    pub ip:String,
    pub port:u32,
}

pub fn now_millis() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub fn get_md5(content:&str) -> String {
    let mut m = crypto::md5::Md5::new();
    m.input_str(content);
    m.result_str()
}

impl HostInfo {
    pub fn new(ip:&str,port:u32) -> Self {
        Self {
            ip:ip.to_owned(),
            port,
        }
    }

    pub fn parse(addr:&str) -> Self {
        let strs=addr.split(':').collect::<Vec<_>>();
        let mut port = 8848u32;
        let ip = strs.get(0).unwrap_or(&"127.0.0.1");
        if let Some(p) = strs.get(1) {
            let pstr = (*p).to_owned();
            port=pstr.parse::<u32>().unwrap_or(8848u32);

        }
        Self{
            ip:(*ip).to_owned(),
            port,
        }
    }
}

#[derive(Debug,Clone)]
pub struct AuthInfo {
    pub username:String,
    pub password:String,
}

#[derive(Debug,Clone)]
pub struct ServerEndpointInfo {
    pub hosts:Vec<HostInfo>,
    pub auth:Option<AuthInfo>,
}

impl ServerEndpointInfo {
    pub fn new(addrs:&str) -> Self {
        let addrs=addrs.split(',').collect::<Vec<_>>();
        let mut hosts = Vec::new();
        for addr in addrs {
            hosts.push(HostInfo::parse(&addr));
        }
        if hosts.len() == 0 {
            hosts.push(HostInfo::parse("127.0.0.1:8848"));
        }
        Self {
            hosts:hosts,
            auth:None,
        }
    }

    pub fn new_with_auth(addrs:&str,auth:AuthInfo) -> Self {
        let addrs=addrs.split(',').collect::<Vec<_>>();
        let mut hosts = Vec::new();
        for addr in addrs {
            hosts.push(HostInfo::parse(&addr));
        }
        Self {
            hosts:hosts,
            auth:Some(auth),
        }
    }

    pub fn select_host(&self) -> &HostInfo{
        let index=naming_client::NamingUtils::select_by_weight_fn(&self.hosts, |e| 1);
        self.hosts.get(index).unwrap()
    }

}

pub struct Client {
    server_addr: String,
    tenant: Option<String>,
}

impl Client {
    pub fn new (addr: &str) -> Client {
        Client{
            server_addr: addr.to_string(),
            tenant: None,
        }
    }
}

