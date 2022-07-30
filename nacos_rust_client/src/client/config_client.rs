
use crate::client::nacos_client::ActixSystemCmd;
use crate::init_global_system_actor;
use crate::client::utils::Utils;
use std::sync::Arc;
use std::{collections::HashMap,time::Duration};
use anyhow::anyhow;
use actix::prelude::*;
use super::auth::{AuthActor, AuthCmd};
use super::nacos_client::{ActixSystemActor, ActixSystemActorSetCmd};
use super::{now_millis, AuthInfo, utils};

use super::{HostInfo,ServerEndpointInfo, TokenInfo};
use super::get_md5;

fn ms(millis:u64) -> Duration {
    Duration::from_millis(millis)
}

#[derive(Debug,Hash,Eq,Clone)]
pub struct ConfigKey {
    pub data_id:String,
    pub group:String,
    pub tenant:String,
}

impl ConfigKey {
    pub fn new (data_id:&str,group:&str,tenant:&str) -> Self {
        Self {
            data_id:data_id.to_owned(),
            group:group.to_owned(),
            tenant: tenant.to_owned(),
        }
    }

    pub fn build_key(&self) -> String {
        if self.tenant.len()==0 {
            return format!("{}\x02{}",self.data_id,self.group)
        }
        format!("{}\x02{}\x02{}",self.data_id,self.group,self.tenant)
    }
}

impl PartialEq for ConfigKey {
    fn eq(&self,o:&Self) -> bool {
        self.data_id==o.data_id && self.group==o.group && self.tenant==self.tenant
    }
}

pub struct ListenerItem {
    pub key:ConfigKey,
    pub md5:String,
}

impl ListenerItem {
    pub fn new(key:ConfigKey,md5:String) -> Self {
        Self {
            key,
            md5,
        }
    }

    pub fn decode_listener_items(configs:&str) -> Vec::<Self> {
        let mut list = vec![];
        let mut start = 0;
        let bytes = configs.as_bytes();
        let mut tmpList = vec![];
        for i in 0..bytes.len(){
            let char = bytes[i];
            if char == 2 {
                if tmpList.len() > 2{
                    continue;
                }
                //tmpList.push(configs[start..i].to_owned());
                tmpList.push(String::from_utf8(configs[start..i].into()).unwrap());
                start = i+1;
            }
            else if char == 1 {
                let mut endValue = String::new();
                if start+1 <=i {
                    //endValue = configs[start..i].to_owned();
                    endValue = String::from_utf8(configs[start..i].into()).unwrap();
                }
                start = i+1;
                if tmpList.len() == 2 {
                    let key = ConfigKey::new(&tmpList[0],&tmpList[1],"");
                    list.push(ListenerItem::new(key,endValue));
                }
                else{
                    let key = ConfigKey::new(&tmpList[0],&tmpList[1],&endValue);
                    list.push(ListenerItem::new(key,tmpList[2].to_owned()));
                }
                tmpList.clear();
            }
        }
        list
    }

    pub fn decode_listener_change_keys(configs:&str) -> Vec::<ConfigKey> {
        let mut list = vec![];
        let mut start = 0;
        let bytes = configs.as_bytes();
        let mut tmpList = vec![];
        for i in 0..bytes.len(){
            let char = bytes[i];
            if char == 2 {
                if tmpList.len() > 2{
                    continue;
                }
                //tmpList.push(configs[start..i].to_owned());
                tmpList.push(String::from_utf8(configs[start..i].into()).unwrap());
                start = i+1;
            }
            else if char == 1 {
                let mut endValue = String::new();
                if start+1 <=i {
                    //endValue = configs[start..i].to_owned();
                    endValue = String::from_utf8(configs[start..i].into()).unwrap();
                }
                start = i+1;
                if tmpList.len() == 1 {
                    let key = ConfigKey::new(&tmpList[0],&endValue,"");
                    list.push(key);
                }
                else{
                    let key = ConfigKey::new(&tmpList[0],&tmpList[1],&endValue);
                    list.push(key);
                }
                tmpList.clear();
            }
        }
        list
    }
}

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
                super::auth::AuthHandleResult::None => {},
                super::auth::AuthHandleResult::Token(v) => {
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
            return Err(anyhow!("get config error"));
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
            return Err(anyhow!("set config error"));
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
            return Err(anyhow!("del config error"));
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
            return Err(anyhow!("listener config error"));
        }
        let text = resp.get_string_body();
        let t = format!("v={}",&text);
        let map:HashMap<&str,String>=serde_urlencoded::from_str(&t).unwrap();
        let text = map.get("v").unwrap_or(&text);
        let items =ListenerItem::decode_listener_change_keys(&text);
        Ok(items)
    }
}


pub trait ConfigListener {
    fn get_key(&self) -> ConfigKey;
    fn change(&self,key:&ConfigKey,value:&str) -> ();
}

#[derive(Clone)]
pub struct ConfigDefaultListener<T>
{
    key:ConfigKey,
    pub content:Arc<std::sync::RwLock<Option<Arc<T>>>>,
    pub convert:Arc<Fn(&str)-> Option<T>+Send+Sync>,
}

impl <T> ConfigDefaultListener <T>
 {
    pub fn new( key:ConfigKey,convert:Arc<Fn(&str)-> Option<T>+Send+Sync>) -> Self {
        Self {
            key,
            content: Default::default(),
            convert,
        }
    }

    pub fn get_value(&self) -> Option<Arc<T>> {
        match self.content.read().unwrap().as_ref() {
            Some(c) => Some(c.clone()),
            _ => None
        }
    }

    fn set_value(content:Arc<std::sync::RwLock<Option<Arc<T>>>>,value:T) {
        let mut r = content.write().unwrap();
        *r = Some(Arc::new(value));
    }
}

impl <T> ConfigListener for ConfigDefaultListener<T> {
    fn get_key(&self) -> ConfigKey {
        self.key.clone()
    }

    fn change(&self,key:&ConfigKey,value:&str) -> () {
        log::debug!("ConfigDefaultListener change:{:?},{}",key,value);
        let content = self.content.clone();
        let  convert = self.convert.as_ref();
        if let Some(value) = convert(value){
            Self::set_value(content, value);
        }
        ()
    }
}

impl ConfigClient {
    pub fn new(host:HostInfo,tenant:String) -> Arc<Self> {
        let mut request_client = ConfigInnerRequestClient::new(host.clone());
        let (config_inner_addr,_) = Self::init_register2(request_client.clone(),None);
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
        let (config_inner_addr,auth_addr) = Self::init_register2(request_client.clone(),auth_info);
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

    fn init_register(mut request_client:ConfigInnerRequestClient,auth_info:Option<AuthInfo>) -> (Addr<ConfigInnerActor>,Addr<AuthActor>) {
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let endpoint=request_client.endpoints.clone();
        std::thread::spawn(move || {
            let rt = System::new();
            let addrs = rt.block_on(async {
                let auth_addr = AuthActor::new(endpoint,auth_info).start();
                request_client.set_auth_addr(auth_addr.clone());
                let addr=ConfigInnerActor::new(request_client).start();
                (addr,auth_addr)
            });
            tx.send(addrs);
            log::info!("config actor init_register");
            println!("config actor init_register");
            rt.run();
        });
        let addrs = rx.recv().unwrap();
        addrs
    }

    fn init_register2(mut request_client:ConfigInnerRequestClient,auth_info:Option<AuthInfo>) -> (Addr<ConfigInnerActor>,Addr<AuthActor>){
        let system_addr =  init_global_system_actor();
        let endpoint=request_client.endpoints.clone();
        let actor = AuthActor::new(endpoint,auth_info);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::AuthActor(actor,tx);
        system_addr.do_send(msg);
        let auth_addr= match rx.recv().unwrap() {
            super::nacos_client::ActixSystemResult::AuthActorAddr(auth_addr) => auth_addr,
            _ => panic!("init actor error"),
        };
        let actor = ConfigInnerActor::new(request_client);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::ConfigInnerActor(actor,tx);
        system_addr.do_send(msg);
        let config_inner_addr= match rx.recv().unwrap() {
            super::nacos_client::ActixSystemResult::ConfigInnerActor(addr) => addr,
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

struct ListenerValue {
    md5:String,
    listeners:Vec<(u64,Box<ConfigListener + Send>)>,
}

impl ListenerValue {
    fn new(listeners:Vec<(u64,Box<ConfigListener + Send>)>,md5:String) -> Self {
        Self{
            md5,
            listeners,
        }
    }

    fn push(&mut self,id:u64,func:Box<ConfigListener + Send>) {
        self.listeners.push((id,func));
    }

    fn notify(&self,key:&ConfigKey,content:&str) {
        for (_,func) in self.listeners.iter() {
            func.change(key,content);
        }
    }

    fn remove(&mut self,id:u64) -> usize{
        let mut indexs = Vec::new();
        for i in 0..self.listeners.len() {
            match self.listeners.get(i){
                Some((item_id,_)) => {
                    if id==*item_id {
                        indexs.push(i);
                    }
                },
                None => {}
            };
        }
        for index in indexs.iter().rev() {
            let index = *index;
            self.listeners.remove(index);
        }
        self.listeners.len()
    }
}

pub struct ConfigInnerActor{
    pub request_client : ConfigInnerRequestClient,
    subscribe_map:HashMap<ConfigKey,ListenerValue>,
}

type ConfigInnerHandleResultSender = tokio::sync::oneshot::Sender<ConfigInnerHandleResult>;

#[derive(Message)]
#[rtype(result="Result<ConfigInnerHandleResult,std::io::Error>")]
pub enum ConfigInnerCmd {
    SUBSCRIBE(ConfigKey,u64,String,Box<ConfigListener + Send + 'static>),
    REMOVE(ConfigKey,u64),
    Close,
}

pub enum ConfigInnerHandleResult {
    None,
    Value(String),
}

impl ConfigInnerActor{
    fn new(request_client:ConfigInnerRequestClient) -> Self{
        Self { 
            request_client,
            subscribe_map: Default::default(),
        }
    }

    fn do_change_config(&mut self,key:&ConfigKey,content:String){
        let md5 = get_md5(&content);
        match self.subscribe_map.get_mut(key) {
            Some(v) => {
                v.md5 = md5;
                v.notify(key, &content);
            },
            None => {}
        }
    }

    fn listener(&mut self,ctx:&mut actix::Context<Self>) {
        if let Some(content) = self.get_listener_body(){
            let request_client = self.request_client.clone();
            let endpoints = self.request_client.endpoints.clone();
            async move{
                let mut list =vec![];
                match request_client.listene(&content, None).await{
                    Ok(items) => {
                        for key in items {
                            if let Ok(value)=request_client.get_config(&key).await {
                                list.push((key,value));
                            }
                        }
                    },
                    Err(_) => {},
                }
                list
            }
            .into_actor(self).map(|r,this,ctx|{
                for (key,context) in r {
                    this.do_change_config(&key,context)
                }
                if this.subscribe_map.len() > 0 {
                    ctx.run_later(Duration::from_millis(5), |act,ctx|{
                        act.listener(ctx);
                    });
                }
            }).spawn(ctx);
        }
    }

    fn get_listener_body(&self) -> Option<String> {
        let items=self.subscribe_map.iter().collect::<Vec<_>>();
        if items.len()==0 {
            return None;
        }
        let mut body=String::new();
        for (k,v) in items {
            body+= &format!("{}\x02{}\x02{}\x02{}\x01",k.data_id,k.group,v.md5,k.tenant);
        }
        Some(body)
    }

}

impl Actor for ConfigInnerActor {
    type Context = Context<Self>;

    fn started(&mut self,ctx:&mut Self::Context){
        log::info!("ConfigInnerActor started");
        ctx.run_later(Duration::from_millis(5), |act,ctx|{
            act.listener(ctx);
        });
    }
}

impl Handler<ConfigInnerCmd> for ConfigInnerActor {
    type Result = Result<ConfigInnerHandleResult,std::io::Error>;
    fn handle(&mut self,msg:ConfigInnerCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg {
            ConfigInnerCmd::SUBSCRIBE(key,id,md5, func) => {
                let first=self.subscribe_map.len()==0;
                let list=self.subscribe_map.get_mut(&key);
                match list {
                    Some(v) => {
                        v.push(id,func);
                        if md5.len()> 0 {
                            v.md5 = md5;
                        }
                    },
                    None => {
                        let v = ListenerValue::new(vec![(id,func)],md5);
                        self.subscribe_map.insert(key, v);
                    },
                };
                if first {
                    ctx.run_later(Duration::from_millis(5), |act,ctx|{
                        act.listener(ctx);
                    });
                }
                Ok(ConfigInnerHandleResult::None)
            },
            ConfigInnerCmd::REMOVE(key, id) => {
                let list=self.subscribe_map.get_mut(&key);
                match list {
                    Some(v) => {
                        let size=v.remove(id);
                        if size == 0 {
                            self.subscribe_map.remove(&key);
                        }
                    },
                    None => {},
                };
                Ok(ConfigInnerHandleResult::None)
            },
            ConfigInnerCmd::Close => {
                ctx.stop();
                Ok(ConfigInnerHandleResult::None)
            },
        }
    }
}
