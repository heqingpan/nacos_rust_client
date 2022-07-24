use nacos_rust_client::{ActorCreate, ActixSystemCreateCmd, init_global_system_actor};
use tonic::transport::Endpoint;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tonic::transport::Channel;
use tower::discover::Change;
use nacos_rust_client::client::naming_client::{ServiceInstanceKey, InstanceDefaultListener};
use nacos_rust_client::client::naming_client::{NamingClient, Instance};

use actix::{prelude::*, Context};

type DiscoverChangeSender=tokio::sync::mpsc::Sender<Change<String,Endpoint>>;
pub struct DiscoverEntity {
    channel: Channel,
    sender: DiscoverChangeSender,
    key:ServiceInstanceKey,
    //listener:InstanceDefaultListener,
}

impl DiscoverEntity {
    pub fn new (key:ServiceInstanceKey,channel: Channel, sender: DiscoverChangeSender) -> Self {
        Self { channel,sender,key}
    }
}


#[derive(Clone)]
pub struct TonicDiscoverFactory {
    tonic_discover_addr:Addr<InnerTonicDiscover>,
    naming_client:Arc<NamingClient>,
}

impl TonicDiscoverFactory {
    pub fn new(naming_client:Arc<NamingClient>) -> Self {
        let system_addr = init_global_system_actor();
        let creator = InnerTonicDiscoverCreate::new(naming_client.clone());
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg=ActixSystemCreateCmd::ActorInit(Box::new(creator.clone()),tx);
        system_addr.do_send(msg);
        rx.recv().unwrap();
        let tonic_discover_addr = creator.get_value().unwrap();
        Self {
            tonic_discover_addr,
            naming_client,
        }
    }

    /**
     * 如果已存在Channel，则直接返回；否则创建后再返回Channel
     */
    pub async fn build_service_channel(&self,key:ServiceInstanceKey) -> anyhow::Result<Channel> {
        let key_str = key.get_key();
        if let Ok(v) = self.get_channel(&key_str).await {
            return Ok(v);
        }
        self.insert_channel(key).await?;
        self.get_channel(&key_str).await
    }

    async fn get_channel(&self,key_str:&String) -> anyhow::Result<Channel> {
        let msg = DiscoverCmd::Get(key_str.to_owned());
        match self.tonic_discover_addr.send(msg).await?? {
            DiscoverResult::None => Err(anyhow::anyhow!("not found channel")),
            DiscoverResult::Channel(v) => Ok(v),
        }
    }


    async fn insert_channel(&self,key:ServiceInstanceKey) -> anyhow::Result<()> {
        let (channel, rx) = Channel::balance_channel(10);
        let addr = self.tonic_discover_addr.clone();
        let new_key = key.clone();

        let listener = InstanceDefaultListener::new(key.clone(),Some(Arc::new(
            move |instances,add_list,remove_list| {
            if add_list.len()> 0 || remove_list.len() > 0 {
                let msg = DiscoverCmd::Change(new_key.clone(), add_list, remove_list);
                addr.do_send(msg);
            }
        })));
        let entity = DiscoverEntity::new(key.clone(),channel,rx);
        let msg = DiscoverCmd::Insert(entity);
        self.tonic_discover_addr.send(msg).await??;
        self.naming_client.subscribe(Box::new(listener)).await?;
        Ok(())
    }
}


pub struct InnerTonicDiscover {
    service_map:HashMap<String,DiscoverEntity>,
    naming_client:Arc<NamingClient>,
}

impl InnerTonicDiscover {
    pub fn new(naming_client:Arc<NamingClient>) -> Self {
        Self {
            naming_client,
            service_map: Default::default(),
        }
    }

    fn change(&mut self,ctx:&mut Context<Self>,key:&ServiceInstanceKey,add_list:Vec<Arc<Instance>>,remove_list:Vec<Arc<Instance>>){
        let key_str = key.get_key();
        if let Some(entity) = self.service_map.get(&key_str){
            let sender = entity.sender.clone();
            async move {
                for item in &add_list {
                    let host=format!("{}:{}",&item.ip,item.port);
                    match Endpoint::from_str(&format!("http://{}",host)) {
                        Ok(endpoint) => {
                            let change = Change::Insert(host,endpoint);
                            sender.send(change).await;
                        }
                        Err(_) => todo!(),
                    };
                }
                for item in &remove_list{
                    let host=format!("{}:{}",&item.ip,item.port);
                    let change = Change::Remove(host);
                    sender.send(change).await;
                }
            }.into_actor(self).map(|r,act,ctx|{
            }).wait(ctx);
        }
    }
}

impl Actor for InnerTonicDiscover {
    type Context = Context<Self>;

    fn started(&mut self,ctx:&mut Self::Context){
        log::info!("AuthActor started");
        //ctx.run_later(Duration::from_nanos(1), |act,ctx|{
        //    act.hb(ctx);
        //});
    }
}

#[derive(Message)]
#[rtype(result="Result<DiscoverResult,std::io::Error>")]
pub enum DiscoverCmd {
    Change(ServiceInstanceKey,Vec<Arc<Instance>>,Vec<Arc<Instance>>),
    Insert(DiscoverEntity),
    Get(String),
}

pub enum DiscoverResult {
    None,
    Channel(Channel),
}


impl Handler<DiscoverCmd> for InnerTonicDiscover {
    type Result = Result<DiscoverResult,std::io::Error>;
    fn handle(&mut self,msg:DiscoverCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg {
            DiscoverCmd::Change(key,add_list, remove_list) => {
                self.change(ctx, &key, add_list, remove_list);
            },
            DiscoverCmd::Insert(entity) => {
                let key = entity.key.get_key();
                self.service_map.insert(key, entity);
            },
            DiscoverCmd::Get(key) => {
                if let Some(e) = self.service_map.get(&key) {
                    return Ok( DiscoverResult::Channel(e.channel.clone()));
                }
            },
            
        };
        Ok(DiscoverResult::None)
    }
}


#[derive(Clone)]
pub struct InnerTonicDiscoverCreate {
    pub content:Arc<std::sync::RwLock<Option<Addr<InnerTonicDiscover>>>>,
    pub params: Arc<NamingClient>,
}

impl InnerTonicDiscoverCreate {

    pub fn new(params: Arc<NamingClient>) -> Self {
        Self{ content:Default::default(),params}
    }

    pub fn get_value(&self) -> Option<Addr<InnerTonicDiscover>> {
        match self.content.read().unwrap().as_ref() {
            Some(c) => Some(c.clone()),
            _ => None
        }
    }

    fn set_value(content:Arc<std::sync::RwLock<Option<Addr<InnerTonicDiscover>>>>,value: Addr<InnerTonicDiscover>) {
        let mut r = content.write().unwrap();
        *r = Some(value);
    }
}

impl ActorCreate for InnerTonicDiscoverCreate {
    fn create(&self) {
        let actor = InnerTonicDiscover::new(self.params.clone());
        let addr  = actor.start();
        Self::set_value(self.content.clone(), addr);
    }
}