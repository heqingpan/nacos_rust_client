use std::{sync::Arc};

use actix::Addr;

use crate::{client::{HostInfo, nacos_client::{ActixSystemActorSetCmd, ActixSystemCmd, ActixSystemResult}, AuthInfo, ServerEndpointInfo, auth::AuthActor, get_md5}, init_global_system_actor};

use super::{ config_key::ConfigKey, listener::{ConfigListener}, inner::{ConfigInnerActor, ConfigInnerCmd}, inner_client::ConfigInnerRequestClient};



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

