use std::collections::HashMap;
use std::sync::Arc;

pub mod builder;
pub mod config_client;
pub mod nacos_client;
pub mod naming_client;

pub mod utils;

pub mod auth;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

pub use self::builder::ClientBuilder;
pub use self::config_client::ConfigClient;
pub use self::nacos_client::NacosClient;
pub use self::naming_client::NamingClient;

#[derive(Debug, Clone, Default)]
pub struct HostInfo {
    pub ip: String,
    pub port: u32,
    pub grpc_port: u32,
}

pub fn now_millis() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub fn get_md5(content: &str) -> String {
    let mut m = Md5::new();
    m.update(content.as_bytes());

    hex::encode(&m.finalize()[..])
}

impl HostInfo {
    pub fn new(ip: &str, port: u32) -> Self {
        Self {
            ip: ip.to_owned(),
            port,
            grpc_port: port + 1000,
        }
    }

    pub fn new_with_grpc(ip: &str, port: u32, grpc_port: u32) -> Self {
        Self {
            ip: ip.to_owned(),
            port,
            grpc_port,
        }
    }

    pub fn parse(addr: &str) -> Self {
        let strs = addr.split(':').collect::<Vec<_>>();
        let mut port = 8848u32;
        let mut grpc_port = port + 1000;
        let ip = strs.first().unwrap_or(&"127.0.0.1");
        if let Some(p) = strs.get(1) {
            let ports = p.split('#').collect::<Vec<_>>();
            if let Some(p) = ports.first() {
                let pstr = (*p).to_owned();
                port = pstr.parse::<u32>().unwrap_or(8848u32);
                grpc_port = port + 1000;
            }
            if let Some(p) = ports.get(1) {
                let pstr = (*p).to_owned();
                grpc_port = pstr.parse::<u32>().unwrap_or(port + 1000);
            }
        }
        Self {
            ip: (*ip).to_owned(),
            port,
            grpc_port,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub username: String,
    pub password: String,
}

impl AuthInfo {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_owned(),
            password: password.to_owned(),
        }
    }
    pub fn is_valid(&self) -> bool {
        !self.username.is_empty() && !self.password.is_empty()
    }
}

/// 客户端信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// 客户端IP
    pub client_ip: String,
    /// 应用名
    pub app_name: String,
    /// 链接扩展信息
    pub headers: HashMap<String, String>,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientInfo {
    pub fn new() -> Self {
        let client_ip = match std::env::var("NACOS_CLIENT_IP") {
            Ok(v) => v,
            Err(_) => local_ipaddress::get().unwrap_or("127.0.0.1".to_owned()),
        };
        let app_name = match std::env::var("NACOS_CLIENT_APP_NAME") {
            Ok(v) => v,
            Err(_) => "UNKNOWN".to_owned(),
        };
        Self {
            client_ip,
            app_name,
            headers: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub access_token: String,
    pub token_ttl: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ServerEndpointInfo {
    pub hosts: Vec<HostInfo>,
}

impl ServerEndpointInfo {
    pub fn new(addrs: &str) -> Self {
        let addrs = addrs.split(',').collect::<Vec<_>>();
        let mut hosts = Vec::new();
        for addr in addrs {
            hosts.push(HostInfo::parse(addr));
        }
        if hosts.is_empty() {
            hosts.push(HostInfo::parse("127.0.0.1:8848"));
        }
        Self { hosts }
    }

    pub fn select_host(&self) -> &HostInfo {
        let index = naming_client::NamingUtils::select_by_weight_fn(&self.hosts, |_| 1);
        self.hosts.get(index).unwrap()
    }
}

pub struct Client {
    //server_addr: String,
    //tenant: Option<String>,
}

impl Client {
    pub fn new(_: &str) -> Client {
        Client{
            //server_addr: addr.to_string(),
            //tenant: None,
        }
    }

    pub async fn login(
        client: &reqwest::Client,
        endpoints: Arc<ServerEndpointInfo>,
        auth_info: &AuthInfo,
    ) -> anyhow::Result<TokenInfo> {
        let mut param: HashMap<&str, &str> = HashMap::new();
        param.insert("username", &auth_info.username);
        param.insert("password", &auth_info.password);
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        );

        let host = endpoints.select_host();
        let url = format!("http://{}:{}/nacos/v1/auth/login", host.ip, host.port);
        let resp = utils::Utils::request(
            client,
            "POST",
            &url,
            serde_urlencoded::to_string(&param)
                .unwrap()
                .as_bytes()
                .to_vec(),
            Some(&headers),
            Some(3000),
        )
        .await?;
        if !resp.status_is_200() {
            return Err(anyhow::anyhow!("get config error"));
        }
        let text = resp.get_string_body();
        let token = serde_json::from_str(&text)?;
        Ok(token)
    }

    pub(crate) fn build_http_headers() -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        );
        headers.insert("User-Agent".to_owned(), "nacos-rust-client 0.3".to_owned());
        headers
    }
}

pub enum ProtocolMode {
    Http,
    Grpc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md5() {
        assert_eq!(
            get_md5("hello"),
            String::from("5d41402abc4b2a76b9719d911017c592")
        );
    }
}
