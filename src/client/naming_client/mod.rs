use std::{sync::Arc, thread::Thread};
use std::time::Duration;
use actix::prelude::*;
use hyper::{Body, Client, Request, Response, body::Buf, client::HttpConnector,Method};
use std::collections::HashMap;
use inner_mem_cache::TimeoutSet;
use super::now_millis;
use crate::client::HostInfo;
use super::utils::{self,Utils};

mod request_model;

use request_model::{BeatInfo,BeatRequest,InstanceWebParams};

static REGISTER_PERIOD :u64 = 5000u64;

#[derive(Debug,Clone)]
struct InnerNamingRequestClient{
    host:HostInfo,
    client: Client<HttpConnector>,
    headers:HashMap<String,String>,
}

impl InnerNamingRequestClient{

    fn new(host:HostInfo) -> Self{
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

    async fn register(&self,instance:&RegisterInstance) -> anyhow::Result<bool>{
        let params = instance.to_web_params();
        let body = serde_urlencoded::to_string(&params)?;
        let url = format!("http://{}:{}/nacos/v1/ns/instance",&self.host.ip,&self.host.port);
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        println!("register:{}",resp.get_lossy_string_body());
        Ok("ok"==resp.get_string_body())
    }

    async fn remove(&self,instance:&RegisterInstance) -> anyhow::Result<bool>{
        let params = instance.to_web_params();
        let body = serde_urlencoded::to_string(&params)?;
        let url = format!("http://{}:{}/nacos/v1/ns/instance",&self.host.ip,&self.host.port);
        let resp=Utils::request(&self.client, "DELETE", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        println!("remove:{}",resp.get_lossy_string_body());
        Ok("ok"==resp.get_string_body())
    }

    async fn heartbeat(&self,beat_string:Arc<String>) -> anyhow::Result<bool>{
        let url = format!("http://{}:{}/nacos/v1/ns/instance/beat",&self.host.ip,&self.host.port);
        let resp=Utils::request(&self.client, "PUT", &url, beat_string.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        println!("heartbeat:{}",resp.get_lossy_string_body());
        return Ok( "ok"==resp.get_string_body());
    }
}

#[derive(Debug,Clone,Default)]
pub struct RegisterInstance {
    //pub id:String,
    pub ip:String,
    pub port:u32,
    pub weight:f32,
    pub enabled:bool,
    pub healthy:bool,
    pub ephemeral: bool,
    pub cluster_name:String,
    pub service_name:String,
    pub group_name:String,
    pub metadata:Option<HashMap<String,String>>,
    pub namespace_id:String,
    //pub app_name:String,
    pub beat_string:Option<Arc<String>>,
}

impl RegisterInstance {

    pub fn new(ip:&str,port:u32,service_name:&str,group_name:&str,cluster_name:&str,namespace_id:&str,metadata:Option<HashMap<String,String>>) -> Self{
        let cluster_name = if cluster_name.len()==0 {"DEFAULT".to_owned()} else{cluster_name.to_owned()};
        let group_name = if group_name.len()==0 {"DEFAULT_GROUP".to_owned()} else{group_name.to_owned()};
        let namespace_id = if namespace_id.len()==0 {"public".to_owned()} else{namespace_id.to_owned()};
        Self{
            ip:ip.to_owned(),
            port,
            weight:1.0f32,
            enabled:true,
            healthy:true,
            ephemeral:true,
            cluster_name,
            service_name:service_name.to_owned(),
            group_name,
            metadata,
            namespace_id,
            beat_string: None,
        }
    }

    pub fn generate_key(&self) -> String {
        format!("{}#{}#{}#{}#{}#{}",&self.ip,&self.port,&self.cluster_name,&self.service_name,&self.group_name,&self.namespace_id)
    }

    pub fn get_service_named(&self) -> String {
        format!("{}@@{}",self.group_name,self.service_name)
    }

    fn generate_beat_info(&self) -> BeatInfo {
        let mut beat = BeatInfo::default();
        beat.cluster = self.cluster_name.to_owned();
        beat.ip = self.ip.to_owned();
        beat.port = self.port;
        if let Some(metadata) = &self.metadata {
            beat.metadata = metadata.clone();
        }
        beat.period = REGISTER_PERIOD as i64;
        beat.scheduled=false;
        beat.serviceName = self.get_service_named();
        beat.stopped=false;
        beat.weight = self.weight;
        beat
        //serde_json::to_string(&beat).unwrap()
    }

    fn generate_beat_request(&self) -> BeatRequest {
        let mut req = BeatRequest::default();
        let beat = self.generate_beat_info();
        req.beat = serde_json::to_string(&beat).unwrap();
        req.namespaceId = self.namespace_id.to_owned();
        req.serviceName = beat.serviceName;
        req.clusterName = beat.cluster;
        req.groupName = self.group_name.to_owned();
        req
    }

    fn generate_beat_request_urlencode(&self) -> String {
        let req = self.generate_beat_request();
        serde_urlencoded::to_string(&req).unwrap()
    }

    pub fn init_beat_string(&mut self) {
        self.beat_string = Some(Arc::new(self.generate_beat_request_urlencode()));
    }

    pub fn to_web_params(&self) -> InstanceWebParams {
        let mut params = InstanceWebParams::default();
        params.ip = self.ip.to_owned();
        params.port = self.port;
        params.namespaceId = self.namespace_id.to_owned();
        params.weight = self.weight;
        params.enabled=true;
        params.healthy=true;
        params.ephemeral=true;
        if let Some(metadata) = &self.metadata {
            params.metadata = serde_json::to_string(metadata).unwrap();
        }
        params.clusterName = self.cluster_name.to_owned();
        params.serviceName = self.get_service_named();
        params.groupName = self.group_name.to_owned();
        params
    }
}


//#[derive()]
pub struct InnerNamingRegister{
    instances:HashMap<String,RegisterInstance>,
    timeout_set:TimeoutSet<String>,
    request_client:InnerNamingRequestClient,
    period: u64,
    stop_remove_all:bool
}

impl InnerNamingRegister {

    pub fn new(host:HostInfo) -> Self{
        Self{
            instances:Default::default(),
            timeout_set:Default::default(),
            request_client: InnerNamingRequestClient::new(host),
            period: REGISTER_PERIOD,
            stop_remove_all:false,
        }
    }

    pub fn hb(&self,ctx:&mut actix::Context<Self>) {
        ctx.run_later(Duration::new(1,0), |act,ctx|{
            let current_time = now_millis();
            let addr = ctx.address();
            for key in act.timeout_set.timeout(current_time){
                addr.do_send(NamingRegisterCmd::Heartbeat(key,current_time));
            }
            act.hb(ctx);
        });
    }

    fn remove_instance(&self,instance:RegisterInstance,ctx:&mut actix::Context<Self>){
        let client = self.request_client.clone();
        async move {
            client.remove(&instance).await.unwrap();
            instance
        }.into_actor(self)
        .map(|_,_,ctx|{}).wait(ctx);
    }

    fn remove_all_instance(&mut self,ctx:&mut actix::Context<Self>) {
        let client = self.request_client.clone();
        let instances = self.instances.clone();
        self.instances = HashMap::new();
        async move {
            for (_,instance) in instances.iter() {
                client.remove(&instance).await.unwrap();
            }
            ()
        }.into_actor(self)
        .map(|_,act,ctx|{
            act.stop_remove_all=true;
            ctx.stop();
        }).wait(ctx);
    }
}

impl Actor for InnerNamingRegister {
    type Context = Context<Self>;

    fn started(&mut self,ctx: &mut Self::Context) {
        println!(" InnerNamingRegister started");
        self.hb(ctx);
        /*
        let addr = ctx.address();
        async {
            tokio::spawn(async{task_02(addr).await});
        }.into_actor(self)
        .map(|_,_,_|{})
        .wait(ctx);
        */
    }

    fn stopping(&mut self,ctx: &mut Self::Context) -> Running {
        println!(" InnerNamingRegister stopping ");
        if self.stop_remove_all {
            return Running::Stop;
        }
        self.remove_all_instance(ctx);
        Running::Continue
    }
}

#[derive(Debug,Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub enum NamingRegisterCmd {
    Register(RegisterInstance),
    Remove(RegisterInstance),
    Heartbeat(String,u64),
    Close(),
}

impl Handler<NamingRegisterCmd> for InnerNamingRegister {
    type Result = Result<(),std::io::Error>;

    fn handle(&mut self,msg:NamingRegisterCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg{
            NamingRegisterCmd::Register(mut instance) => {
                instance.init_beat_string();
                let key = instance.generate_key();
                if self.instances.contains_key(&key) {
                    return Ok(());
                }
                // request register
                let time = now_millis();
                self.timeout_set.add(time+self.period,key.clone());
                let client = self.request_client.clone();
                async move {
                    client.register(&instance).await.unwrap();
                    instance
                }.into_actor(self)
                .map(|instance,act,_|{
                    act.instances.insert(key, instance);
                })
                .wait(ctx);
            },
            NamingRegisterCmd::Remove(instance) => {
                let key = instance.generate_key();
                if let Some(instatnce)=self.instances.remove(&key) {
                    // request unregister
                    self.remove_instance(instance, ctx);

                }
            },
            NamingRegisterCmd::Heartbeat(key,time) => {
                if let Some(instance)=self.instances.get(&key) {
                    // request heartbeat
                    let client = self.request_client.clone();
                    if let Some(beat_string) = &instance.beat_string {
                        let beat_string = beat_string.clone();
                        async move {
                            client.heartbeat(beat_string).await.unwrap();
                        }.into_actor(self)
                        .map(|_,_,_|{}).wait(ctx);
                    }
                    self.timeout_set.add(time+self.period, key);
                }
            },
            NamingRegisterCmd::Close() => {
                ctx.stop();
            }
        }
        Ok(())
    }
}


pub struct NamingClient{
    host:HostInfo,
    namespace_id:String,
    register:Addr<InnerNamingRegister>,
}

impl Drop for NamingClient {

    fn drop(&mut self) { 
        println!("NamingClient droping");
        self.register.do_send(NamingRegisterCmd::Close());
        std::thread::sleep(utils::ms(200));
    }
}

impl NamingClient {
    pub fn new(host:HostInfo,namespace_id:String) -> Self {
        let register=Self::init_register(host.clone());
        Self{
            host,
            namespace_id,
            register,
        }
    }

    fn init_register(host:HostInfo) -> Addr<InnerNamingRegister> {
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let rt = System::new();
            let addr = rt.block_on(async {InnerNamingRegister::new(host).start()});
            tx.send(addr);
            rt.run();
        });
        let addr = rx.recv().unwrap();
        addr
    }

    pub fn register(&self,instance:RegisterInstance) {
        self.register.do_send(NamingRegisterCmd::Register(instance));
    }

    pub fn unregister(&self,instance:RegisterInstance) {
        self.register.do_send(NamingRegisterCmd::Remove(instance));
    }
}