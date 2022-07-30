
use std::sync::Arc;
use std::sync::Mutex;

use crate::client::naming_client::{InnerNamingListener,InnerNamingRegister};
use crate::client::naming_client::UdpWorker;
use crate::client::auth::AuthActor;
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
    config:NacosConfig,
}

impl NacosClient {
    pub fn new(addr:&str) -> Self{
        let config = NacosConfig::new(addr);
        Self {
            config
        }
    }
}


pub struct ActixSystemActor {}

impl ActixSystemActor {
    pub fn new() -> Self {
        Self{}
    }
}

impl Actor for ActixSystemActor {
    type Context = Context<Self>;

    fn started(&mut self,ctx:&mut Self::Context){
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
}

pub enum ActixSystemResult {
    None,
    AuthActorAddr(Addr<AuthActor>),
    ConfigInnerActor(Addr<ConfigInnerActor>),
    UdpWorker(Addr<UdpWorker>),
    InnerNamingListener(Addr<InnerNamingListener>),
    InnerNamingRegister(Addr<InnerNamingRegister>),
}

impl Handler<ActixSystemCmd> for ActixSystemActor 
{
    type Result = Result<ActixSystemResult,std::io::Error>;
    fn handle(&mut self,msg:ActixSystemCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg {
            ActixSystemCmd::AuthActor(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::AuthActorAddr(addr));
            },
            ActixSystemCmd::ConfigInnerActor(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::ConfigInnerActor(addr));
            },
            ActixSystemCmd::UdpWorker(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::UdpWorker(addr));

            },
            ActixSystemCmd::InnerNamingListener(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::InnerNamingListener(addr));
            },
            ActixSystemCmd::InnerNamingRegister(actor, tx) => {
                let addr = actor.start();
                tx.send(ActixSystemResult::InnerNamingRegister(addr));
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
    fn handle(&mut self,msg:ActixSystemCreateCmd,ctx:&mut Context<Self>) -> Self::Result {
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
    fn handle(&mut self,msg:ActixSystemCreateAsyncCmd,ctx:&mut Context<Self>) -> Self::Result {
        let v = match msg {
            ActixSystemCreateAsyncCmd::ActorInit(v) => {
                v.create();
                v
            },
        };
        Ok(v)
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

