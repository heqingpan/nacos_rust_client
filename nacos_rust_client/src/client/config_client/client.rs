use std::{sync::Arc, collections::HashMap};

use actix::Addr;

use crate::{client::{ServerEndpointInfo, auth::{AuthActor, AuthCmd, AuthHandleResult}, HostInfo, utils::Utils, nacos_client::{ActixSystemActorSetCmd, ActixSystemCmd, ActixSystemResult}, AuthInfo, get_md5}, init_global_system_actor};

use super::{ config_key::ConfigKey, listener::{ListenerItem, ConfigListener}, inner::{ConfigInnerActor, ConfigInnerCmd}};



pub struct ConfigClient{
    tenant:String,
    request_client:ConfigInnerRequestClient,
    config_inner_addr: Addr<ConfigInnerActor> ,
}

impl Drop for ConfigClient {
    fn drop(&mut self){
        self.config_inner_addr.do_send(ConfigInnerCmd::Close);
        //std::thread::sleep(utils::ms(500));
    }
}


impl ConfigClient {
    pub fn new(host:HostInfo,tenant:String) -> Arc<Self> {
        let request_client = ConfigInnerRequestClient::new(host.clone());
        let (config_inner_addr,_) = Self::init_register(request_client.clone(),None);
        //request_client.set_auth_addr(auth_addr);
        let r=Arc::new(Self{
            tenant,
            request_client,
            config_inner_addr
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastConfigClient(r.clone()));
        r
    }

    pub fn new_with_addrs(addrs:&str,tenant:String,auth_info:Option<AuthInfo>) -> Arc<Self> {
        let endpoint = Arc::new(ServerEndpointInfo::new(addrs));
        let mut request_client = ConfigInnerRequestClient::new_with_endpoint(endpoint);
        let (config_inner_addr,auth_addr) = Self::init_register(request_client.clone(),auth_info);
        request_client.set_auth_addr(auth_addr);
        let r=Arc::new(Self{
            tenant,
            request_client,
            config_inner_addr
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastConfigClient(r.clone()));
        r
    }

    fn init_register(mut request_client:ConfigInnerRequestClient,auth_info:Option<AuthInfo>) -> (Addr<ConfigInnerActor>,Addr<AuthActor>){
        let system_addr =  init_global_system_actor();
        let endpoint=request_client.endpoints.clone();
        let actor = AuthActor::new(endpoint,auth_info);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::AuthActor(actor,tx);
        system_addr.do_send(msg);
        let auth_addr= match rx.recv().unwrap() {
            ActixSystemResult::AuthActorAddr(auth_addr) => auth_addr,
            _ => panic!("init actor error"),
        };
        request_client.set_auth_addr(auth_addr.clone());
        let actor = ConfigInnerActor::new(request_client);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::ConfigInnerActor(actor,tx);
        system_addr.do_send(msg);
        let config_inner_addr= match rx.recv().unwrap() {
            ActixSystemResult::ConfigInnerActor(addr) => addr,
            _ => panic!("init actor error"),
        };
        (config_inner_addr,auth_addr)
    }

    pub fn gene_config_key(&self,data_id:&str,group:&str) -> ConfigKey {
        ConfigKey{
            data_id:data_id.to_owned(),
            group:group.to_owned(),
            tenant:self.tenant.to_owned(),
        }
    }

    pub async fn get_config(&self,key:&ConfigKey) -> anyhow::Result<String> {
        self.request_client.get_config(key).await
    }

    pub async fn set_config(&self,key:&ConfigKey,value:&str) -> anyhow::Result<()> {
        self.request_client.set_config(key,value).await
    }

    pub async fn del_config(&self,key:&ConfigKey) -> anyhow::Result<()> {
        self.request_client.del_config(key).await
    }

    pub async fn listene(&self,content:&str,timeout:Option<u64>) -> anyhow::Result<Vec<ConfigKey>> {
        self.request_client.listene(content, timeout).await
    }

    pub async fn subscribe<T:ConfigListener + Send + 'static>(&self,listener:Box<T>) -> anyhow::Result<()> {
        let key = listener.get_key();
        self.subscribe_with_key(key, listener).await
    }

    pub async fn subscribe_with_key<T:ConfigListener + Send + 'static>(&self,key:ConfigKey,listener:Box<T>) -> anyhow::Result<()> {
        let id=0u64;
        let md5=match self.get_config(&key).await{
            Ok(text) => {
                listener.change(&key,&text);
                get_md5(&text)
            }
            Err(_) => {
                "".to_owned()
            },
        };
        let msg=ConfigInnerCmd::SUBSCRIBE(key,id,md5,listener);
        self.config_inner_addr.do_send(msg);
        //let msg=ConfigInnerMsg::SUBSCRIBE(key,id,md5,listener);
        //self.subscribe_sender.send(msg).await;
        Ok(())
    }

    pub async fn unsubscribe(&self,key:ConfigKey ) -> anyhow::Result<()>{
        let id = 0u64;
        let msg = ConfigInnerCmd::REMOVE(key, id);
        self.config_inner_addr.do_send(msg);
        Ok(())
    }

}

#[derive(Clone)]
pub struct ConfigInnerRequestClient{
    endpoints: Arc<ServerEndpointInfo>,
    client: reqwest::Client,
    headers:HashMap<String,String>,
    auth_addr: Option<Addr<AuthActor>>,
}

impl ConfigInnerRequestClient {
    pub fn new(host:HostInfo) -> Self {
        /* 
        let client = Client::builder()
        .http1_title_case_headers(true)
        .http1_preserve_header_case(true)
        .build_http();
        */
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned());
        let client = reqwest::Client::builder()
            .build().unwrap();
        let endpoints = ServerEndpointInfo{
            hosts: vec![host],
        };
        Self{
            endpoints:Arc::new(endpoints),
            client,
            headers,
            auth_addr:None,
        }
    }

    pub fn new_with_endpoint(endpoints:Arc<ServerEndpointInfo>) -> Self {
        let client = reqwest::Client::builder()
            .build().unwrap();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned());
        Self {
            endpoints,
            client,
            headers,
            auth_addr:None
        }
    } 

    pub fn set_auth_addr(&mut self,addr:Addr<AuthActor>){
        self.auth_addr = Some(addr);
    }

    pub async fn get_token_result(&self) -> anyhow::Result<String>{
        if let Some(auth_addr) = &self.auth_addr {
            match auth_addr.send(AuthCmd::QueryToken).await?? {
                AuthHandleResult::None => {},
                AuthHandleResult::Token(v) => {
                    if v.len()> 0{
                        return Ok(format!("accessToken={}",&v));
                    }
                },
            };
        }
        Ok(String::new())
    }

    pub async fn get_token(&self) -> String {
        self.get_token_result().await.unwrap_or_default()
    }

    pub async fn get_config(&self,key:&ConfigKey) -> anyhow::Result<String> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0{
            param.insert("tenant",&key.tenant);
        }
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/cs/configs?{}&{}",host.ip,host.port,token_param,serde_urlencoded::to_string(&param).unwrap());
        let resp=Utils::request(&self.client, "GET", &url, vec![], Some(&self.headers), Some(3000)).await?;
        if !resp.status_is_200() {
            return Err(anyhow::anyhow!("get config error"));
        }
        let text = resp.get_string_body();
        log::debug!("get_config:{}",&text);
        Ok(text)
    }

    pub async fn set_config(&self,key:&ConfigKey,value:&str) -> anyhow::Result<()> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0{
            param.insert("tenant",&key.tenant);
        }
        param.insert("content",value);
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!("http://{}:{}/nacos/v1/cs/configs?{}",host.ip,host.port,token_param);

        let body = serde_urlencoded::to_string(&param).unwrap();
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        if !resp.status_is_200() {
            log::error!("{}",resp.get_lossy_string_body());
            return Err(anyhow::anyhow!("set config error"));
        }
        Ok(())
    }

    pub async fn del_config(&self,key:&ConfigKey) -> anyhow::Result<()> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0{
            param.insert("tenant",&key.tenant);
        }
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!("http://{}:{}/nacos/v1/cs/configs?{}",host.ip,host.port,token_param);
        let body = serde_urlencoded::to_string(&param).unwrap();
        let resp=Utils::request(&self.client, "DELETE", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        if !resp.status_is_200() {
            log::error!("{}",resp.get_lossy_string_body());
            return Err(anyhow::anyhow!("del config error"));
        }
        Ok(())
    }

    pub async fn listene(&self,content:&str,timeout:Option<u64>) -> anyhow::Result<Vec<ConfigKey>> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        let timeout = timeout.unwrap_or(30000u64);
        let timeout_str = timeout.to_string();
        param.insert("Listening-Configs", content);
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!("http://{}:{}/nacos/v1/cs/configs/listener?{}",host.ip,host.port,token_param);
        let body = serde_urlencoded::to_string(&param).unwrap();
        let mut headers = self.headers.clone();
        headers.insert("Long-Pulling-Timeout".to_owned(), timeout_str);
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&headers), Some(timeout+1000)).await?;
        if !resp.status_is_200() {
            log::error!("{},{},{},{}",&url,&body,resp.status,resp.get_lossy_string_body());
            return Err(anyhow::anyhow!("listener config error"));
        }
        let text = resp.get_string_body();
        let t = format!("v={}",&text);
        let map:HashMap<&str,String>=serde_urlencoded::from_str(&t).unwrap();
        let text = map.get("v").unwrap_or(&text);
        let items =ListenerItem::decode_listener_change_keys(&text);
        Ok(items)
    }
}
