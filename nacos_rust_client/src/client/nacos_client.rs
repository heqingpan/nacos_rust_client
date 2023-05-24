
use crate::client::ConfigClient;
use crate::client::NamingClient;
use crate::conn_manage::manage::ConnManage;
use std::sync::Arc;
use std::sync::Mutex;

use crate::client::naming_client::{InnerNamingListener,InnerNamingRegister};
use crate::client::naming_client::UdpWorker;
use crate::client::auth::AuthActor;
use super::utils;
use super::{HostInfo, config_client::ConfigInnerActor};
use actix::{prelude::*, Context};

#[derive(Debug,Clone)]
pub struct NacosConfig {
    pub config_host:Option<HostInfo>,
    pub naming_host:Option<HostInfo>,
}

impl NacosConfig {
    pub fn new(addr:&str) -> Self {
        let host = HostInfo::parse(addr);
        Self{
            config_host:Some(host.clone()),
            naming_host:Some(host.clone()),
        }
    }
}

pub struct NacosClient {
    //config:NacosConfig,
}

impl NacosClient {
    pub fn new(_:&str) -> Self{
        //let config = NacosConfig::new(addr);
        Self {
            //config
        }
    }
}


pub struct ActixSystemActor {
    last_config_client: Option<Arc<ConfigClient>>,
    last_naming_client: Option<Arc<NamingClient>>,
}

impl ActixSystemActor {
    pub fn new() -> Self {
        Self{
            last_config_client:None,
            last_naming_client:None,
        }
    }
}

impl Actor for ActixSystemActor {
    type Context = Context<Self>;

    fn started(&mut self,_:&mut Self::Context){
        log::info!("ActixSystemActor started");
    }
}

type ActixSystemResultSender = std::sync::mpsc::SyncSender<ActixSystemResult>;

#[derive(Message)]
#[rtype(result="Result<ActixSystemResult,std::io::Error>")]
pub enum ActixSystemCmd
{
    AuthActor(AuthActor,ActixSystemResultSender),
    ConfigInnerActor(ConfigInnerActor,ActixSystemResultSender),
    UdpWorker(UdpWorker,ActixSystemResultSender),
    InnerNamingListener(InnerNamingListener,ActixSystemResultSender),
    InnerNamingRegister(InnerNamingRegister,ActixSystemResultSender),
    ConnManage(ConnManage,ActixSystemResultSender),
    Close,
}

pub enum ActixSystemResult {
    None,
    AuthActorAddr(Addr<AuthActor>),
    ConfigInnerActor(Addr<ConfigInnerActor>),
    UdpWorker(Addr<UdpWorker>),
    InnerNamingListener(Addr<InnerNamingListener>),
    InnerNamingRegister(Addr<InnerNamingRegister>),
    ConnManage(Addr<ConnManage>),
}

impl Handler<ActixSystemCmd> for ActixSystemActor 
{
    type Result = Result<ActixSystemResult,std::io::Error>;
    fn handle(&mut self,msg:ActixSystemCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg {
            ActixSystemCmd::AuthActor(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::AuthActorAddr(addr)).unwrap_or_default();
            },
            ActixSystemCmd::ConfigInnerActor(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::ConfigInnerActor(addr)).unwrap_or_default();
            },
            ActixSystemCmd::UdpWorker(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::UdpWorker(addr)).unwrap_or_default();

            },
            ActixSystemCmd::InnerNamingListener(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::InnerNamingListener(addr)).unwrap_or_default();
            },
            ActixSystemCmd::InnerNamingRegister(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::InnerNamingRegister(addr)).unwrap_or_default();
            },
            ActixSystemCmd::ConnManage(actor,tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::ConnManage(addr)).unwrap_or_default();
            },
            ActixSystemCmd::Close => {
                if let Some(naming_client) = &self.last_naming_client {
                    naming_client.droping();
                    std::thread::sleep(utils::ms(100));
                }
                self.last_config_client=None;
                self.last_naming_client=None;
                ctx.stop();
                System::current().stop();
            },
        }
        Ok(ActixSystemResult::None)
    }
}

pub trait ActorCreate {
    fn create(&self) -> ();
}

pub struct ActorCreateWrap <T: actix::Actor,P> {
    pub content:Arc<std::sync::Mutex<Option<Addr<T>>>>,
    pub params: P,
}

impl <T: actix::Actor,P> ActorCreateWrap<T, P> {
    pub fn get_value(&self) -> Option<Addr<T>> {
        match self.content.lock().unwrap().as_ref() {
            Some(c) => Some(c.clone()),
            _ => None
        }
    }

    pub fn set_value(&self,addr:Addr<T>)  {
        let mut r = self.content.lock().unwrap();
        *r = Some(addr);
    }
}

type ActixSystemCreateResultSender = std::sync::mpsc::SyncSender<Box<dyn ActorCreate+Send>>;

#[derive(Message)]
#[rtype(result="Result<(),std::io::Error>")]
pub enum ActixSystemCreateCmd{
    ActorInit(Box<dyn ActorCreate+Send>,ActixSystemCreateResultSender)
}

impl Handler<ActixSystemCreateCmd> for ActixSystemActor 
{
    type Result = Result<(),std::io::Error>;
    fn handle(&mut self,msg:ActixSystemCreateCmd,_:&mut Context<Self>) -> Self::Result {
        match msg {
            ActixSystemCreateCmd::ActorInit(t,tx) => {
                t.create();
                tx.send(t).unwrap();
            },
        };
        Ok(())
    }
}

#[derive(Message)]
#[rtype(result="Result<Box<dyn ActorCreate+Send>,std::io::Error>")]
pub enum ActixSystemCreateAsyncCmd{
    ActorInit(Box<dyn ActorCreate+Send>)
}

impl Handler<ActixSystemCreateAsyncCmd> for ActixSystemActor 
{
    type Result = Result<Box<dyn ActorCreate+Send>,std::io::Error>;
    fn handle(&mut self,msg:ActixSystemCreateAsyncCmd,_:&mut Context<Self>) -> Self::Result {
        let v = match msg {
            ActixSystemCreateAsyncCmd::ActorInit(v) => {
                v.create();
                v
            },
        };
        Ok(v)
    }
}

#[derive(Message)]
#[rtype(result="Result<(),std::io::Error>")]
pub enum ActixSystemActorSetCmd{
    LastConfigClient(Arc<ConfigClient>),
    LastNamingClient(Arc<NamingClient>),
}

impl Handler<ActixSystemActorSetCmd> for ActixSystemActor 
{
    type Result = Result<(),std::io::Error>;
    fn handle(&mut self,msg:ActixSystemActorSetCmd,_:&mut Context<Self>) -> Self::Result {
        match msg {
            ActixSystemActorSetCmd::LastConfigClient(config_client) => {
                self.last_config_client = Some(config_client);
            },
            ActixSystemActorSetCmd::LastNamingClient(naming_client) => {
                self.last_naming_client = Some(naming_client);
            },
        }
        Ok(())
    }
}

type ActixSystemActorQueryResultSender = std::sync::mpsc::SyncSender<Box<ActixSystemActorQueryResult>>;


#[derive(Message)]
#[rtype(result="Result<ActixSystemActorQueryResult,std::io::Error>")]
pub enum ActixSystemActorQueryCmd{
    QueryLastConfigClient,
    QueryLastNamingClient,
    SyncQueryLastConfigClient(ActixSystemActorQueryResultSender),
    SyncQueryLastNamingClient(ActixSystemActorQueryResultSender),
}

pub enum ActixSystemActorQueryResult{
    None,
    LastConfigClient(Arc<ConfigClient>),
    LastNamingClient(Arc<NamingClient>),
}

impl Handler<ActixSystemActorQueryCmd> for ActixSystemActor 
{
    type Result = Result<ActixSystemActorQueryResult,std::io::Error>;
    fn handle(&mut self,msg:ActixSystemActorQueryCmd,_:&mut Context<Self>) -> Self::Result {
        match msg {
            ActixSystemActorQueryCmd::QueryLastConfigClient => {
                if let Some(client) = &self.last_config_client{
                    return Ok(ActixSystemActorQueryResult::LastConfigClient(client.clone()));
                }
            },
            ActixSystemActorQueryCmd::QueryLastNamingClient => {
                if let Some(client) = &self.last_naming_client{
                    return Ok(ActixSystemActorQueryResult::LastNamingClient(client.clone()));
                }
            },
            ActixSystemActorQueryCmd::SyncQueryLastConfigClient(sender) => {
                if let Some(client) = &self.last_config_client{
                    sender.send(Box::new(ActixSystemActorQueryResult::LastConfigClient(client.clone()))).unwrap();
                }
                else{
                    sender.send(Box::new(ActixSystemActorQueryResult::None)).unwrap();
                }
            },
            ActixSystemActorQueryCmd::SyncQueryLastNamingClient(sender) => {
                if let Some(client) = &self.last_naming_client{
                    sender.send(Box::new(ActixSystemActorQueryResult::LastNamingClient(client.clone()))).unwrap();
                }
                else{
                    sender.send(Box::new(ActixSystemActorQueryResult::None)).unwrap();
                }
            },
        }
        Ok(ActixSystemActorQueryResult::None)
    }
}





lazy_static::lazy_static! {
    static ref ACTIX_SYSTEM: Mutex<Option<Addr<ActixSystemActor>>> =  Mutex::new(None);
}

pub fn get_global_system_actor() -> Option<Addr<ActixSystemActor>> {
    let r = ACTIX_SYSTEM.lock().unwrap();
    r.clone()
}

pub fn set_global_system_actor(addr:Addr<ActixSystemActor>) {
    let mut r = ACTIX_SYSTEM.lock().unwrap();
    *r = Some(addr);
}

pub fn init_global_system_actor() -> Addr<ActixSystemActor> {
    if let Some(r)= get_global_system_actor() {
        return r;
    }
    else{
        let addr = init_register();
        set_global_system_actor(addr.clone());
        return addr;
    }
}

pub fn close_global_system_actor()  {
    let mut r = ACTIX_SYSTEM.lock().unwrap();
    if let Some(addr) = &*r {
        addr.do_send(ActixSystemCmd::Close);
        *r = None;
    }
}

pub fn get_last_config_client() -> Option<Arc<ConfigClient>> {
    if let Some(actor) = get_global_system_actor() {
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        actor.do_send(ActixSystemActorQueryCmd::SyncQueryLastConfigClient(tx));
        match *rx.recv().unwrap() {
            ActixSystemActorQueryResult::LastConfigClient(client) => {
                return Some(client);
            },
            _ => {},
        }
    }
    None
}

pub fn get_last_naming_client() -> Option<Arc<NamingClient>> {
    if let Some(actor) = get_global_system_actor() {
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        actor.do_send(ActixSystemActorQueryCmd::SyncQueryLastNamingClient(tx));
        match *rx.recv().unwrap() {
            ActixSystemActorQueryResult::LastNamingClient(client) => {
                return Some(client);
            },
            _ => {
            },
        }
    }
    None
}

fn init_register() -> Addr<ActixSystemActor> {
    let (tx,rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let rt = System::new();
        let addrs = rt.block_on(async {
            ActixSystemActor::new().start()
        });
        tx.send(addrs).unwrap();
        rt.run().unwrap();
    });
    let addrs = rx.recv().unwrap();
    addrs
}

