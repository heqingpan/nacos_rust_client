use tonic::transport::Endpoint;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tonic::transport::Channel;
use tower::discover::Change;
use nacos_rust_client::client::naming_client::{ServiceInstanceKey, InstanceDefaultListener};
use nacos_rust_client::client::naming_client::{NamingClient, Instance,QueryInstanceListParams};

pub struct DiscoverEntity {
    channel: Channel,
    key:ServiceInstanceKey,
    listener:InstanceDefaultListener,
}


pub struct TonicDiscoverFactory {
    content:Arc<std::sync::RwLock<HashMap<String,DiscoverEntity>>>,
    naming_client:Arc<NamingClient>,
}

impl TonicDiscoverFactory {
    pub fn new(naming_client:Arc<NamingClient>) -> Self {
        Self {
            content:Default::default(),
            naming_client,
        }
    }

    pub fn build_service_channel(&self,key:ServiceInstanceKey) -> Option<Channel> {
        let key_str = key.get_key();
        if let Some(v) = self.get_channel(&key_str) {
            return Some(v);
        }
        self.insert_channel(key);
        self.get_channel(&key_str)
    }

    fn get_channel(&self,key_str:&String) -> Option<Channel> {
        let map =self.content.read().unwrap();
        if let Some(v)=map.get(key_str) {
            Some(v.channel.clone())
        }
        else{
            None
        }
    }

    async fn insert_channel(&self,key:ServiceInstanceKey) {
        let map = self.content.write().unwrap();
        let (channel, rx) = Channel::balance_channel(10);
        let listener = InstanceDefaultListener::new(key.clone(),Some(Arc::new(
            move |instances,add_list,remove_list| {
            for item in &add_list {
                let host=format!("{}:{}",&item.ip,item.port);
                match Endpoint::from_str(&format!("http://{}",host)) {
                    Ok(endpoint) => {
                        let change = Change::Insert(host,endpoint);
                        rx.send(change);
                        //rx.send(change).await;
                    }
                    Err(_) => todo!(),
                };
            }
        })));
    }
}