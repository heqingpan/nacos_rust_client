use std::{collections::HashMap, sync::Arc};

use crate::client;
use crate::client::{
    auth::{AuthActor, AuthCmd, AuthHandleResult},
    utils::Utils,
    HostInfo, ServerEndpointInfo,
};
use actix::Addr;

use super::{listener::ListenerItem, ConfigKey};

#[derive(Clone)]
pub struct ConfigInnerRequestClient {
    pub(crate) endpoints: Arc<ServerEndpointInfo>,
    pub(crate) client: reqwest::Client,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) auth_addr: Option<Addr<AuthActor>>,
}

impl ConfigInnerRequestClient {
    pub fn new(host: HostInfo) -> Self {
        let client = reqwest::Client::builder().build().unwrap();
        let endpoints = ServerEndpointInfo { hosts: vec![host] };
        Self {
            endpoints: Arc::new(endpoints),
            client,
            headers: client::Client::build_http_headers(),
            auth_addr: None,
        }
    }

    pub fn new_with_endpoint(
        endpoints: Arc<ServerEndpointInfo>,
        auth_addr: Option<Addr<AuthActor>>,
    ) -> Self {
        let client = reqwest::Client::builder().build().unwrap();
        Self {
            endpoints,
            client,
            headers: client::Client::build_http_headers(),
            auth_addr,
        }
    }

    pub fn set_auth_addr(&mut self, addr: Addr<AuthActor>) {
        self.auth_addr = Some(addr);
    }

    pub async fn get_token_result(&self) -> anyhow::Result<String> {
        if let Some(auth_addr) = &self.auth_addr {
            match auth_addr.send(AuthCmd::QueryToken).await?? {
                AuthHandleResult::None => {}
                AuthHandleResult::Token(v) => {
                    if v.len() > 0 {
                        return Ok(format!("accessToken={}", &v));
                    }
                }
            };
        }
        Ok(String::new())
    }

    pub async fn get_token(&self) -> String {
        self.get_token_result().await.unwrap_or_default()
    }

    pub async fn get_config(&self, key: &ConfigKey) -> anyhow::Result<String> {
        let mut param: HashMap<&str, &str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0 {
            param.insert("tenant", &key.tenant);
        }
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!(
            "http://{}:{}/nacos/v1/cs/configs?{}&{}",
            host.ip,
            host.port,
            token_param,
            serde_urlencoded::to_string(&param).unwrap()
        );
        let resp = Utils::request(
            &self.client,
            "GET",
            &url,
            vec![],
            Some(&self.headers),
            Some(3000),
        )
        .await?;
        if !resp.status_is_200() {
            return Err(anyhow::anyhow!("get config error"));
        }
        let text = resp.get_string_body();
        log::debug!("get_config:{}", &text);
        Ok(text)
    }

    pub async fn set_config(&self, key: &ConfigKey, value: &str) -> anyhow::Result<()> {
        let mut param: HashMap<&str, &str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0 {
            param.insert("tenant", &key.tenant);
        }
        param.insert("content", value);
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!(
            "http://{}:{}/nacos/v1/cs/configs?{}",
            host.ip, host.port, token_param
        );

        let body = serde_urlencoded::to_string(&param).unwrap();
        let resp = Utils::request(
            &self.client,
            "POST",
            &url,
            body.as_bytes().to_vec(),
            Some(&self.headers),
            Some(3000),
        )
        .await?;
        if !resp.status_is_200() {
            log::error!("{}", resp.get_lossy_string_body());
            return Err(anyhow::anyhow!("set config error"));
        }
        Ok(())
    }

    pub async fn del_config(&self, key: &ConfigKey) -> anyhow::Result<()> {
        let mut param: HashMap<&str, &str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0 {
            param.insert("tenant", &key.tenant);
        }
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!(
            "http://{}:{}/nacos/v1/cs/configs?{}",
            host.ip, host.port, token_param
        );
        let body = serde_urlencoded::to_string(&param).unwrap();
        let resp = Utils::request(
            &self.client,
            "DELETE",
            &url,
            body.as_bytes().to_vec(),
            Some(&self.headers),
            Some(3000),
        )
        .await?;
        if !resp.status_is_200() {
            log::error!("{}", resp.get_lossy_string_body());
            return Err(anyhow::anyhow!("del config error"));
        }
        Ok(())
    }

    pub async fn listene(
        &self,
        content: &str,
        timeout: Option<u64>,
    ) -> anyhow::Result<Vec<ConfigKey>> {
        let mut param: HashMap<&str, &str> = HashMap::new();
        let timeout = timeout.unwrap_or(30000u64);
        let timeout_str = timeout.to_string();
        param.insert("Listening-Configs", content);
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!(
            "http://{}:{}/nacos/v1/cs/configs/listener?{}",
            host.ip, host.port, token_param
        );
        let body = serde_urlencoded::to_string(&param).unwrap();
        let mut headers = self.headers.clone();
        headers.insert("Long-Pulling-Timeout".to_owned(), timeout_str);
        let resp = Utils::request(
            &self.client,
            "POST",
            &url,
            body.as_bytes().to_vec(),
            Some(&headers),
            Some(timeout + 1000),
        )
        .await?;
        if !resp.status_is_200() {
            log::error!(
                "{},{},{},{}",
                &url,
                &body,
                resp.status,
                resp.get_lossy_string_body()
            );
            return Err(anyhow::anyhow!("listener config error"));
        }
        let text = resp.get_string_body();
        let t = format!("v={}", &text);
        let map: HashMap<&str, String> = serde_urlencoded::from_str(&t).unwrap();
        let text = map.get("v").unwrap_or(&text);
        let items = ListenerItem::decode_listener_change_keys(&text);
        Ok(items)
    }
}
