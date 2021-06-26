
use std::sync::Arc;
use std::{collections::HashMap,time::Duration};
use anyhow::anyhow;

use serde::__private::de::borrow_cow_bytes;
use tokio::sync::RwLock;

use super::HostInfo;
use super::get_md5;


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
                tmpList.push(configs[start..i].to_owned());
                start = i+1;
            }
            else if char == 1 {
                let mut endValue = String::new();
                if start+1 <=i {
                    endValue = configs[start..i].to_owned();
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
                tmpList.push(configs[start..i].to_owned());
                start = i+1;
            }
            else if char == 1 {
                start = i+1;
                if tmpList.len() == 2 {
                    let key = ConfigKey::new(&tmpList[0],&tmpList[1],"");
                    list.push(key);
                }
                else{
                    let key = ConfigKey::new(&tmpList[0],&tmpList[1],&tmpList[2]);
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
    subscribe_map:RwLock<HashMap<ConfigKey,Vec<Box<ConfigListener>>>>,
    subscribe_context:RwLock<HashMap<ConfigKey,String>>,
}

//type ConfigListenerFunc =dyn Fn(&ConfigKey,&str);

pub trait ConfigListener {
    fn enable(&self) -> bool;
    fn change(&self,key:&ConfigKey,value:&str) -> ();
}

impl ConfigClient {
    pub fn new(host:HostInfo,tenant:String) -> Self {
        Self{
            host,
            tenant,
            subscribe_map: Default::default(),
            subscribe_context: Default::default(),
        }
    }

    pub fn gene_config_key(&self,group:&str,data_id:&str) -> ConfigKey {
        ConfigKey{
            data_id:data_id.to_owned(),
            group:group.to_owned(),
            tenant:self.tenant.to_owned(),
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
        let res = reqwest::get(&url).await?;
        if res.status().as_u16() != 200u16 {
            return Err(anyhow!("get config error"));
        }
        let text = res.text().await?;
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
        let client = reqwest::Client::builder().connect_timeout(Duration::from_millis(1000))
            .timeout(Duration::from_secs(3000))
            .build().unwrap();
        let body = serde_urlencoded::to_string(&param).unwrap();
        println!("{},{}",&url,&body);
        let res = client.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body).send().await?;
        let status = res.status().as_u16();
        if status != 200u16 {
            println!("{:?}",&res);
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
        let client = reqwest::Client::builder().connect_timeout(Duration::from_millis(1000))
            .timeout(Duration::from_secs(3000))
            .build().unwrap();
        let body = serde_urlencoded::to_string(&param).unwrap();
        let res = client.delete(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body).send().await?;
        let status = res.status().as_u16();
        if status != 200u16 {
            println!("{:?}",&res);
            return Err(anyhow!("del config error"));
        }
        Ok(())
    }

    pub async fn subscribe<T:ConfigListener+'static>(&self,key:ConfigKey,listener:T) {
        let mut rt = self.subscribe_map.write().await;
        let listener = Box::new(listener);
        let list=rt.get_mut(&key);
        match list {
            Some(v) => {
                v.push(listener);
            },
            None => {
                rt.insert(key.clone(), vec![listener]);
                self.subscribe_context.write().await.insert(key, "".to_owned());
            },
        };
    }

    pub async fn run_listener_reactor(self:Arc<Self>) {
        let this = self.clone();
        //tokio::spawn(async move {
        //    this.do_listener();
        //});
    }

    async fn do_listener(self) {
        loop {
            self.do_once_listener().await;
        }
    }

    async fn do_once_listener(&self) -> anyhow::Result<()> {
        let content = self.get_listener_body().await;
        match content{
            Some(content) => {
                let mut param : HashMap<&str,&str> = HashMap::new();
                param.insert("Listening-Configs", &content);
                let url = format!("http://{}:{}/nacos/v1/cs/configs/listener",self.host.ip,self.host.port);
                let client = reqwest::Client::builder().connect_timeout(Duration::from_millis(1000))
                    .timeout(Duration::from_secs(30000+1000))
                    .build().unwrap();
                let body = serde_urlencoded::to_string(&param).unwrap();
                let res = client.delete(&url)
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .header("Long-Pulling-Timeout", "30000")
                    .body(body).send().await?;
                let status = res.status().as_u16();
                if status != 200u16 {
                    println!("{:?}",&res);
                    return Err(anyhow!("listener config error"));
                }
                let text = res.text().await?;
                let items =ListenerItem::decode_listener_change_keys(&text);
                for key in items {
                    self.do_change_config(&key).await;
                }
            }
            None => {

            }
        };
        Ok(())
    }

    async fn get_listener_body(&self) -> Option<String> {
        let rt = self.subscribe_context.read().await;
        let items=rt.iter().collect::<Vec<_>>();
        if items.len()==0 {
            return None;
        }
        let mut body=String::new();
        for (k,v) in items {
            //body+= &k.build_key() + "\x02"+ v;
            body+= &format!("{}\x02{}\x02{}\x02{}\x01",k.data_id,k.group,v,k.tenant);
        }
        Some(body)
    }

    async fn do_change_config(&self,key:&ConfigKey) {
        match self.get_config(key).await {
            Ok(v)  => {
                let md5 = get_md5(&v);
                self.notify_listener(&key,&v).await;
                self.update_key_md5(key.clone(), md5).await;
            },
            Err(e) => {
            }
        }
    }

    async fn update_key_md5(&self,key:ConfigKey,md5:String) {
        self.subscribe_context.write().await.insert(key,md5);
    }

    async fn notify_listener(&self,key:&ConfigKey,content:&str) {
        let rt = self.subscribe_map.read().await;
        match rt.get(key){
            Some(list) => {
                for item in list {
                    item.change(key, content);
                }
            },
            None => {},
        }
    }

}

pub struct ConfigClientInner{
    //subscribe_map:RwLock<HashMap<ConfigKey,Vec<Box<ConfigListener>>>>,
    subscribe_context:RwLock<HashMap<ConfigKey,String>>,
}