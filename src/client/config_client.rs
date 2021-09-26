
use crate::client::utils::Utils;
use std::sync::Arc;
use std::{collections::HashMap,time::Duration};
use anyhow::anyhow;

use hyper::Client;
use hyper::client::HttpConnector;
use tokio::sync::RwLock;

use super::HostInfo;
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
    host:HostInfo,
    tenant:String,
    request_client:ConfigInnerRequestClient,
    subscribe_sender:ConfigInnerMsgSenderType,
    subscribe_map:RwLock<HashMap<ConfigKey,Vec<Box<ConfigListenerFunc>>>>,
    subscribe_context:RwLock<HashMap<ConfigKey,String>>,
}

#[derive(Debug,Clone)]
pub struct ConfigInnerRequestClient{
    host:HostInfo,
    client: Client<HttpConnector>,
    headers:HashMap<String,String>,
}

impl ConfigInnerRequestClient {
    pub fn new(host:HostInfo) -> Self {
        let client = Client::builder()
        .http1_title_case_headers(true)
        .http1_preserve_header_case(true)
        .build_http();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned());
        Self{
            host,
            client,
            headers,
        }
    }

    pub async fn get_config(&self,key:&ConfigKey) -> anyhow::Result<String> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        param.insert("group", &key.group);
        param.insert("dataId", &key.data_id);
        if key.tenant.len() > 0{
            param.insert("tenant",&key.tenant);
        }
        let url = format!("http://{}:{}/nacos/v1/cs/configs?{}",self.host.ip,self.host.port,serde_urlencoded::to_string(&param).unwrap());
        let resp=Utils::request(&self.client, "GET", &url, vec![], Some(&self.headers), Some(3000)).await?;
        if !resp.status_is_200() {
            return Err(anyhow!("get config error"));
        }
        let text = resp.get_string_body();
        //println!("get_config:{}",&text);
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
        let url = format!("http://{}:{}/nacos/v1/cs/configs",self.host.ip,self.host.port);

        let body = serde_urlencoded::to_string(&param).unwrap();
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        if !resp.status_is_200() {
            println!("{}",resp.get_lossy_string_body());
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
        let url = format!("http://{}:{}/nacos/v1/cs/configs",self.host.ip,self.host.port);
        let body = serde_urlencoded::to_string(&param).unwrap();
        let resp=Utils::request(&self.client, "DELETE", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        if !resp.status_is_200() {
            println!("{}",resp.get_lossy_string_body());
            return Err(anyhow!("del config error"));
        }
        Ok(())
    }

    pub async fn listene(&self,content:&str,timeout:Option<u64>) -> anyhow::Result<Vec<ConfigKey>> {
        let mut param : HashMap<&str,&str> = HashMap::new();
        let timeout = timeout.unwrap_or(30000u64);
        let timeout_str = timeout.to_string();
        param.insert("Listening-Configs", content);
        let url = format!("http://{}:{}/nacos/v1/cs/configs/listener",self.host.ip,self.host.port);
        let body = serde_urlencoded::to_string(&param).unwrap();
        let mut headers = self.headers.clone();
        headers.insert("Long-Pulling-Timeout".to_owned(), timeout_str);
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&headers), Some(timeout+1000)).await?;
        if !resp.status_is_200() {
            println!("{},{},{},{}",&url,&body,resp.status,resp.get_lossy_string_body());
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

type ConfigListenerFunc =Fn(&ConfigKey,&str) + Send + 'static;

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
        println!("ConfigDefaultListener change:{:?},{}",key,value);
        let content = self.content.clone();
        let  convert = self.convert.as_ref();
        if let Some(value) = convert(value){
            Self::set_value(content, value);
        }
        ()
    }
}

impl ConfigClient {
    pub fn new(host:HostInfo,tenant:String) -> Self {
        let request_client = ConfigInnerRequestClient::new(host.clone());
        let (tx,rx) = tokio::sync::mpsc::channel::<ConfigInnerMsg>(100);
        ConfigInnerSubscribeClient::new(request_client.clone(),rx).spawn();
        Self{
            host,
            tenant,
            request_client,
            subscribe_sender:tx,
            subscribe_map: Default::default(),
            subscribe_context: Default::default(),
        }
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

    pub async fn subscribe<T:ConfigListener + Send + 'static>(&mut self,listener:Box<T>) -> anyhow::Result<()> {
        let key = listener.get_key();
        self.subscribe_with_key(key, listener).await
    }

    pub async fn subscribe_with_key<T:ConfigListener + Send + 'static>(&mut self,key:ConfigKey,listener:Box<T>) -> anyhow::Result<()> {
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
        let msg=ConfigInnerMsg::SUBSCRIBE(key,id,md5,listener);
        self.subscribe_sender.send(msg).await;
        Ok(())
    }
}
enum ConfigInnerMsg {
    SUBSCRIBE(ConfigKey,u64,String,Box<ConfigListener + Send + 'static>),
    REMOVE(ConfigKey,u64),
}

type ConfigInnerMsgSenderType = tokio::sync::mpsc::Sender<ConfigInnerMsg>;
type ConfigInnerMsgReceiverType = tokio::sync::mpsc::Receiver<ConfigInnerMsg>;

//unsafe impl Send for ConfigInnerMsg {}

struct ListeneValue {
    md5:String,
    listeners:Vec<(u64,Box<ConfigListener + Send>)>,
}

impl ListeneValue {
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


pub struct ConfigInnerSubscribeClient{
    request_client:ConfigInnerRequestClient,
    rt:ConfigInnerMsgReceiverType,
}

impl ConfigInnerSubscribeClient {

    fn new(request_client:ConfigInnerRequestClient,rt:ConfigInnerMsgReceiverType) -> Self {
        Self {
            request_client,
            rt,
        }
    }

    fn spawn(self) {
        tokio::spawn(async move {
            self.recv().await;
        });
    }

    async fn recv(self) {
        let mut rt = self.rt;
        //let request_client = self.request_client.clone();
        let mut subscribe_map:HashMap<ConfigKey,ListeneValue>=Default::default();
        loop {
            let mut has_msg=false;
            tokio::select! {
                Some(msg) = rt.recv() => {
                    subscribe_map=Self::take_msg(msg, subscribe_map);
                    has_msg=true;
                },
                _ = tokio::time::sleep(ms(1)) => {},
            }
            if has_msg {
                continue;
            }
            subscribe_map = Self::do_once_listener(&self.request_client,subscribe_map).await;
            tokio::time::sleep(ms(5)).await;
        }
    }

    fn take_msg(msg:ConfigInnerMsg,mut subscribe_map:HashMap<ConfigKey,ListeneValue>) -> HashMap<ConfigKey,ListeneValue> {
        match msg {
            ConfigInnerMsg::SUBSCRIBE(key,id,md5, func) => {
                let list=subscribe_map.get_mut(&key);
                match list {
                    Some(v) => {
                        v.push(id,func);
                        if md5.len()> 0 {
                            v.md5 = md5;
                        }
                    },
                    None => {
                        let v = ListeneValue::new(vec![(id,func)],md5);
                        subscribe_map.insert(key, v);
                    },
                };
            },
            ConfigInnerMsg::REMOVE(key, id) => {
                let list=subscribe_map.get_mut(&key);
                match list {
                    Some(v) => {
                        let size=v.remove(id);
                        if size == 0 {
                            subscribe_map.remove(&key);
                        }
                    },
                    None => {},
                };

            },
        };
        subscribe_map
    }

    async fn do_once_listener(request_client:&ConfigInnerRequestClient,subscribe_map:HashMap<ConfigKey,ListeneValue>) -> HashMap<ConfigKey,ListeneValue> {
        let (content,mut subscribe_map) = Self::get_listener_body(subscribe_map);
        //println!("do_once_listener,{:?}",content);
        match content{
            Some(content) => {
                match request_client.listene(&content, None).await {
                    Ok(items) => {
                        for key in items {
                            subscribe_map=Self::do_change_config(request_client,subscribe_map,&key).await;
                            //let md5_value = "".to_owned();
                            //Self::update_key_md5(&subscribe_map,&key,md5_value);
                        }
                    },
                    Err(_) => {},
                };
            },
            None => { }
        };
        subscribe_map
    }

    fn get_listener_body(subscribe_map:HashMap<ConfigKey,ListeneValue>) -> (Option<String>,HashMap<ConfigKey,ListeneValue>) {
        let items=subscribe_map.iter().collect::<Vec<_>>();
        if items.len()==0 {
            return (None,subscribe_map);
        }
        let mut body=String::new();
        for (k,v) in items {
            body+= &format!("{}\x02{}\x02{}\x02{}\x01",k.data_id,k.group,v.md5,k.tenant);
        }
        (Some(body),subscribe_map)
    }


    async fn do_change_config(request_client:&ConfigInnerRequestClient,mut subscribe_map:HashMap<ConfigKey,ListeneValue>,key:&ConfigKey) -> HashMap<ConfigKey,ListeneValue> {
        match request_client.get_config(key).await {
            Ok(content)  => {
                let md5 = get_md5(&content);
                match subscribe_map.get_mut(key) {
                    Some(v) => {
                        v.md5 = md5;
                        v.notify(key, &content);
                    },
                    None => {}
                }
            },
            Err(e) => {
            }
        };
        subscribe_map
    }
}