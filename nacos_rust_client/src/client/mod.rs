use std::collections::HashMap;
use std::sync::Arc;

pub mod nacos_client;
pub mod config_client;
pub mod naming_client;

pub mod utils;

pub mod auth;

use crypto::digest::Digest;
use serde::{Serialize, Deserialize};

pub use self::nacos_client::NacosClient;
pub use self::config_client::ConfigClient;
pub use self::naming_client::NamingClient;

#[derive(Debug, Clone)]
pub struct HostInfo {
    pub ip:String,
    pub port:u32,
    pub grpc_port:u32,
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
            grpc_port:port+1000,
        }
    }

    pub fn new_with_grpc(ip:&str,port:u32,grpc_port:u32) -> Self {
        Self {
            ip:ip.to_owned(),
            port,
            grpc_port,
        }
    }

    pub fn parse(addr:&str) -> Self {
        let strs=addr.split(':').collect::<Vec<_>>();
        let mut port = 8848u32;
        let mut grpc_port = port+1000;
        let ip = strs.get(0).unwrap_or(&"127.0.0.1");
        if let Some(p) = strs.get(1) {
            let ports=p.split('#').collect::<Vec<_>>();
            if let Some(p) = ports.get(0) {
                let pstr = (*p).to_owned();
                port=pstr.parse::<u32>().unwrap_or(8848u32);
            }
            if let Some(p) = ports.get(1) {
                let pstr = (*p).to_owned();
                grpc_port=pstr.parse::<u32>().unwrap_or(port+1000);
            }
        }
        Self{
            ip:(*ip).to_owned(),
            port,
            grpc_port,
        }
    }
}

#[derive(Debug,Clone)]
pub struct AuthInfo {
    pub username:String,
    pub password:String,
}

impl AuthInfo {
    pub fn new(username:&str,password:&str) -> Self {
        Self { 
            username: username.to_owned(), 
            password: password.to_owned(),
        }
    }
    pub fn is_valid(&self) -> bool {
        self.username.len()> 0 && self.password.len()>0
    }
}

#[derive(Debug,Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub access_token:String,
    pub token_ttl:u64,
}

#[derive(Debug,Clone)]
pub struct ServerEndpointInfo {
    pub hosts:Vec<HostInfo>,
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
        }
    }

    pub fn select_host(&self) -> &HostInfo{
        let index=naming_client::NamingUtils::select_by_weight_fn(&self.hosts, |_| 1);
        self.hosts.get(index).unwrap()
    }

}

pub struct Client {
    //server_addr: String,
    //tenant: Option<String>,
}

impl Client {
    pub fn new (_: &str) -> Client {
        Client{
            //server_addr: addr.to_string(),
            //tenant: None,
        }
    }

    pub async fn login(client:&reqwest::Client,endpoints:Arc<ServerEndpointInfo>,auth_info:&AuthInfo) -> anyhow::Result<TokenInfo> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        param.insert("username", &auth_info.username);
        param.insert("password", &auth_info.password);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned());

        let host = endpoints.select_host();
        let url = format!("http://{}:{}/nacos/v1/auth/login",host.ip,host.port);
        let resp=utils::Utils::request(client, "POST", &url, serde_urlencoded::to_string(&param).unwrap().as_bytes().to_vec(), Some(&headers), Some(3000)).await?;
        if !resp.status_is_200() {
            return Err(anyhow::anyhow!("get config error"));
        }
        let text = resp.get_string_body();
        let token = serde_json::from_str(&text)?;
        Ok(token)
    }
}


