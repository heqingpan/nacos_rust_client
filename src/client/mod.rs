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

