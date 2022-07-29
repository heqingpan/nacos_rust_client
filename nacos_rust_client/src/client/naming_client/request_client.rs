use crate::client::naming_client::QueryListResult;
use crate::client::naming_client::QueryInstanceListParams;
use crate::client::utils::Utils;
use crate::client::naming_client::Instance;
use crate::client::{auth::{AuthActor, AuthCmd, AuthHandleResult}, HostInfo};
use actix::Addr;
use std::{collections::HashMap, sync::Arc};

use crate::client::ServerEndpointInfo;

#[derive(Clone)]
pub struct InnerNamingRequestClient{
    pub(crate) client: reqwest::Client,
    pub(crate) headers:HashMap<String,String>,
    pub(crate) endpoints: Arc<ServerEndpointInfo>,
    pub(crate) auth_addr: Option<Addr<AuthActor>>,
}

impl InnerNamingRequestClient{

    pub(crate) fn new(host:HostInfo) -> Self{
        /* 
        let client = Client::builder()
        .http1_title_case_headers(true)
        .http1_preserve_header_case(true)
        .build_http();
        */
        let client = reqwest::Client::builder()
            .build().unwrap();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned());
        let endpoints = Arc::new(ServerEndpointInfo{
            hosts: vec![host],
        });
        Self{
            client,
            headers,
            endpoints,
            auth_addr:None,
        }
    }

    pub(crate) fn new_with_endpoint(endpoints:Arc<ServerEndpointInfo>) -> Self {
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

    pub(crate) fn set_auth_addr(&mut self,addr:Addr<AuthActor>){
        self.auth_addr = Some(addr);
    }

    pub(crate) async fn get_token_result(&self) -> anyhow::Result<String>{
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

    pub(crate) async fn get_token(&self) -> String {
        self.get_token_result().await.unwrap_or_default()
    }

    pub(crate) async fn register(&self,instance:&Instance) -> anyhow::Result<bool>{
        let params = instance.to_web_params();
        let body = serde_urlencoded::to_string(&params)?;
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/ns/instance?{}",&host.ip,&host.port,token_param);
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        log::info!("register:{}",resp.get_lossy_string_body());
        Ok("ok"==resp.get_string_body())
    }

    pub(crate) async fn remove(&self,instance:&Instance) -> anyhow::Result<bool>{
        let params = instance.to_web_params();
        let body = serde_urlencoded::to_string(&params)?;
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/ns/instance?{}",&host.ip,&host.port,token_param);
        let resp=Utils::request(&self.client, "DELETE", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        log::info!("remove:{}",resp.get_lossy_string_body());
        Ok("ok"==resp.get_string_body())
    }

    pub(crate) async fn heartbeat(&self,beat_string:Arc<String>) -> anyhow::Result<bool>{
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/ns/instance/beat?{}",&host.ip,&host.port,token_param);
        let resp=Utils::request(&self.client, "PUT", &url, beat_string.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        log::debug!("heartbeat:{}",resp.get_lossy_string_body());
        return Ok( "ok"==resp.get_string_body());
    }

    pub(crate) async fn get_instance_list(&self,query_param:&QueryInstanceListParams) -> anyhow::Result<QueryListResult> {
        let params = query_param.to_web_params();
        let token_param = self.get_token().await;
        let host = self.endpoints.select_host();
        let url = format!("http://{}:{}/nacos/v1/ns/instance/list?{}&{}",&host.ip,&host.port
                ,token_param,&serde_urlencoded::to_string(&params)?);
        let resp=Utils::request(&self.client, "GET", &url, vec![], Some(&self.headers), Some(3000)).await?;
        
        let result:Result<QueryListResult,_>=serde_json::from_slice(&resp.body);
        match result {
            Ok(r) => {
                log::debug!("get_instance_list instance:{}",&r.hosts.is_some());
                return Ok( r)
            },
            Err(e) => {
                log::error!("get_instance_list error:\n\turl:{}\n\t{}",&url,resp.get_string_body());
                return Err(anyhow::format_err!(e))
            }
        }
    }
}
