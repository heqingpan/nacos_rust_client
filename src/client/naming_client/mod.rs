use crate::client::ServerEndpointInfo;
use crate::client::auth::AuthActor;
use crate::client::AuthInfo;
use std::collections::HashSet;
use std::{sync::Arc, thread::Thread};
use std::time::Duration;
use std::env;
use actix::prelude::*;
use std::collections::HashMap;
use inner_mem_cache::TimeoutSet;
use super::auth::AuthCmd;
use super::now_millis;
use crate::client::HostInfo;
use super::utils::{self,Utils};

mod api_model;
mod udp_actor;

pub use api_model::{BeatInfo,BeatRequest,InstanceWebParams,InstanceWebQueryListParams,QueryListResult,NamingUtils,InstanceVO};
pub use udp_actor::{UdpWorker,UdpDataCmd};

static REGISTER_PERIOD :u64 = 5000u64;

#[derive(Clone)]
pub struct InnerNamingRequestClient{
    client: reqwest::Client,
    headers:HashMap<String,String>,
    endpoints: Arc<ServerEndpointInfo>,
    auth_addr: Option<Addr<AuthActor>>,
}

impl InnerNamingRequestClient{

    fn new(host:HostInfo) -> Self{
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

    async fn register(&self,instance:&Instance) -> anyhow::Result<bool>{
        let params = instance.to_web_params();
        let body = serde_urlencoded::to_string(&params)?;
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/ns/instance?{}",&host.ip,&host.port,token_param);
        let resp=Utils::request(&self.client, "POST", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        log::info!("register:{}",resp.get_lossy_string_body());
        Ok("ok"==resp.get_string_body())
    }

    async fn remove(&self,instance:&Instance) -> anyhow::Result<bool>{
        let params = instance.to_web_params();
        let body = serde_urlencoded::to_string(&params)?;
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/ns/instance?{}",&host.ip,&host.port,token_param);
        let resp=Utils::request(&self.client, "DELETE", &url, body.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        log::info!("remove:{}",resp.get_lossy_string_body());
        Ok("ok"==resp.get_string_body())
    }

    async fn heartbeat(&self,beat_string:Arc<String>) -> anyhow::Result<bool>{
        let host = self.endpoints.select_host();
        let token_param = self.get_token().await;
        let url = format!("http://{}:{}/nacos/v1/ns/instance/beat?{}",&host.ip,&host.port,token_param);
        let resp=Utils::request(&self.client, "PUT", &url, beat_string.as_bytes().to_vec(), Some(&self.headers), Some(3000)).await?;
        log::debug!("heartbeat:{}",resp.get_lossy_string_body());
        return Ok( "ok"==resp.get_string_body());
    }

    async fn get_instance_list(&self,query_param:&QueryInstanceListParams) -> anyhow::Result<QueryListResult> {
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

#[derive(Debug,Clone,Default)]
pub struct Instance {
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

impl Instance {

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
    instances:HashMap<String,Instance>,
    timeout_set:TimeoutSet<String>,
    request_client:InnerNamingRequestClient,
    period: u64,
    stop_remove_all:bool
}

impl InnerNamingRegister {

    pub fn new(request_client:InnerNamingRequestClient) -> Self{
        Self{
            instances:Default::default(),
            timeout_set:Default::default(),
            request_client,
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

    fn remove_instance(&self,instance:Instance,ctx:&mut actix::Context<Self>){
        let client = self.request_client.clone();
        async move {
            client.remove(&instance).await;
            instance
        }.into_actor(self)
        .map(|_,_,ctx|{}).spawn(ctx);
    }

    fn remove_all_instance(&mut self,ctx:&mut actix::Context<Self>) {
        let client = self.request_client.clone();
        let instances = self.instances.clone();
        for (_,instance) in instances {
            self.remove_instance(instance,ctx);
        }
        self.instances = HashMap::new();
        /*
        async move {
            for (_,instance) in instances.iter() {
                client.remove(&instance).await;
            }
            ()
        }.into_actor(self)
        .map(|_,act,ctx|{
            act.stop_remove_all=true;
            ctx.stop();
        }).spawn(ctx);
        */
    }
}

impl Actor for InnerNamingRegister {
    type Context = Context<Self>;

    fn started(&mut self,ctx: &mut Self::Context) {
        log::info!(" InnerNamingRegister started");
        self.hb(ctx);
    }

    fn stopping(&mut self,ctx: &mut Self::Context) -> Running {
        log::info!(" InnerNamingRegister stopping ");
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
    Register(Instance),
    Remove(Instance),
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
                    client.register(&instance).await;
                    instance
                }.into_actor(self)
                .map(|instance,act,_|{
                    act.instances.insert(key, instance);
                })
                .spawn(ctx);
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
                            client.heartbeat(beat_string).await;
                        }.into_actor(self)
                        .map(|_,_,_|{}).spawn(ctx);
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

#[derive(Debug,Clone,Default)]
pub struct ServiceInstanceKey {
    //pub namespace_id:String,
    pub group_name:String,
    pub service_name:String,
}

impl ServiceInstanceKey{
    pub fn new(service_name:&str,group_name:&str) -> Self{
        Self{
            group_name:group_name.to_owned(),
            service_name:service_name.to_owned(),
        }
    }
    pub fn get_key(&self) -> String {
        NamingUtils::get_group_and_service_name(&self.service_name, &self.group_name)
    }

    pub fn from_str(key_str:&str) -> Self {
        let mut s = Self::new("","");
        if let Some((group,service))=NamingUtils::split_group_and_serivce_name(&key_str) {
            s.group_name=group;
            s.service_name = service;
        }
        s
    }
}


#[derive(Debug,Clone,Default)]
pub struct QueryInstanceListParams{
    pub namespace_id:String,
    pub group_name:String,
    pub service_name:String,
    pub clusters:Option<Vec<String>>,
    pub healthy_only:bool,
    client_ip:Option<String>,
    udp_port:Option<u16>
}

impl QueryInstanceListParams{
    pub fn new(namespace_id:&str,group_name:&str,service_name:&str,clusters:Option<Vec<String>>,healthy_only:bool) -> Self {
        Self{
            namespace_id:namespace_id.to_owned(),
            group_name:group_name.to_owned(),
            service_name:service_name.to_owned(),
            clusters:clusters,
            healthy_only,
            client_ip:None,
            udp_port:None,
        }
    }

    pub fn get_key(&self) -> String {
        NamingUtils::get_group_and_service_name(&self.service_name, &self.group_name)
    }

    fn to_web_params(&self) -> InstanceWebQueryListParams {
        let mut params = InstanceWebQueryListParams::default();
        params.namespaceId = self.namespace_id.to_owned();
        params.groupName = self.group_name.to_owned();
        params.serviceName = NamingUtils::get_group_and_service_name(&self.service_name, &self.group_name);
        if let Some(clusters) = &self.clusters {
            params.clusters = clusters.join(",")
        }
        params.healthyOnly=self.healthy_only;
        params.clientIP=self.client_ip.clone();
        params.udpPort=self.udp_port;
        params
    }
}

type InstanceListenerValue= Vec<Arc<Instance>>;
pub trait InstanceListener {
    fn get_key(&self) -> ServiceInstanceKey;
    fn change(&self,key:&ServiceInstanceKey,value:&InstanceListenerValue,add_list:&InstanceListenerValue,remove_list:&InstanceListenerValue) -> ();
}

#[derive(Clone)]
pub struct InstanceDefaultListener {
    key:ServiceInstanceKey,
    pub content:Arc<std::sync::RwLock<Option<Arc<Vec<Arc<Instance>>>>>>,
    pub callback:Option<Arc<Fn(Arc<InstanceListenerValue>,InstanceListenerValue,InstanceListenerValue)-> () +Send+Sync>>,
}

impl InstanceDefaultListener{
    pub fn new( key:ServiceInstanceKey,callback:Option<Arc<Fn(Arc<InstanceListenerValue>,InstanceListenerValue,InstanceListenerValue)-> () +Send+Sync>>) -> Self {
        Self{
            key,
            content: Default::default(),
            callback,
        }
    }

    pub fn get_content(&self) -> Arc<Vec<Arc<Instance>>> {
        match self.content.read().unwrap().as_ref() {
            Some(c) => c.clone(),
            _ => Default::default()
        }
    }

    fn set_value(content:Arc<std::sync::RwLock<Option<Arc<Vec<Arc<Instance>>>>>>,value:Vec<Arc<Instance>>){
        let mut r = content.write().unwrap();
        *r = Some(Arc::new(value));
    }

}

impl InstanceListener for InstanceDefaultListener {
    
    fn get_key(&self) -> ServiceInstanceKey { 
        self.key.clone()
    }

    fn change(&self,key:&ServiceInstanceKey,value:&Vec<Arc<Instance>>,add:&Vec<Arc<Instance>>,remove:&Vec<Arc<Instance>>) -> () {
        log::debug!("InstanceDefaultListener change,key{:?},valid count:{},add count:{},remove count:{}",key,value.len(),add.len(),remove.len());
        let content = self.content.clone();
        if value.len() > 0 {
            Self::set_value(content, value.clone());
            if let Some(callback) = &self.callback {
                callback(self.get_content(),add.clone(),remove.clone());
            }
        }
    }
}


struct ListenerValue{
    pub listener_key:ServiceInstanceKey,
    pub listener: Box<dyn InstanceListener+Send>,
    pub id:u64,
}

impl ListenerValue{
    fn new(listener_key:ServiceInstanceKey,listener:Box<dyn InstanceListener+Send>,id:u64) -> Self{
        Self{
            listener_key,
            listener,
            id,
        }
    }
}

#[derive(Debug,Default,Clone)]
struct InstancesWrap{
    instances: Vec<Arc<Instance>>,
    params:QueryInstanceListParams,
    last_sign:String,
    next_time:u64,
}


pub struct InnerNamingListener {
    namespace_id:String,
    //group@@servicename
    listeners:HashMap<String,Vec<ListenerValue>>,
    instances:HashMap<String,InstancesWrap>,
    timeout_set:TimeoutSet<String>,
    request_client:InnerNamingRequestClient,
    period: u64,
    client_ip:String,
    udp_port:u16,
    udp_addr:Addr<UdpWorker>,
}

impl InnerNamingListener {
    pub fn new(namespace_id:&str,client_ip:&str,udp_port:u16,request_client:InnerNamingRequestClient,udp_addr:Addr<UdpWorker>) -> Self{
        Self{
            namespace_id:namespace_id.to_owned(),
            listeners: Default::default(),
            instances: Default::default(),
            timeout_set: Default::default(),
            request_client,
            period:3000,
            client_ip:client_ip.to_owned(),
            udp_port:udp_port,
            udp_addr,
        }
    }

    pub fn query_instance(&self,key:String,ctx:&mut actix::Context<Self>) {
        let client = self.request_client.clone();
        if let Some(instance_warp) = self.instances.get(&key) {
            let params= instance_warp.params.clone();
            async move{
                (key,client.get_instance_list(&params).await)
            }.into_actor(self)
            .map(|(key,res),act,ctx|{
                match res {
                    Ok(result) => {
                    act.update_instances_and_notify(key, result);
                    },
                    Err(e) =>{
                        log::error!("get_instance_list error:{}",e);
                    },
                };
            })
            .spawn(ctx);
        }
    }

    fn update_instances_and_notify(&mut self,key:String,result:QueryListResult) -> anyhow::Result<()> {
        if let Some(cache_millis) = result.cacheMillis {
            self.period = cache_millis;
        }
        let mut is_notify=false;
        let mut old_instance_map = HashMap::new();
        if let Some(instance_warp) = self.instances.get_mut(&key) {
            let checksum = result.checksum.unwrap_or("".to_owned());
            if instance_warp.last_sign != checksum || instance_warp.last_sign.len()==0 {
                instance_warp.last_sign = checksum;
                if let Some(hosts) = result.hosts {
                    for e in &instance_warp.instances {
                        old_instance_map.insert(format!("{}:{}",e.ip,e.port), e.clone());
                    }
                    instance_warp.instances = hosts.into_iter()
                        .map(|e| Arc::new(e.to_instance()))
                        .filter(|e|e.weight>0.001f32)
                        .collect();
                    is_notify=true;
                }
            }
            let current_time = now_millis();
            instance_warp.next_time = current_time+self.period;
        }
        if is_notify {
            if let Some(instance_warp) = self.instances.get(&key) {
                let mut add_list = vec![];
                for item in &instance_warp.instances {
                    let key = format!("{}:{}",item.ip,item.port);
                    if old_instance_map.remove(&key).is_none() {
                        add_list.push(item.clone());
                    }
                }
                let remove_list:Vec<Arc<Instance>> = old_instance_map.into_iter().map(|(k,v)| {v}).collect();
                self.notify_listener(key, &instance_warp.instances,add_list,remove_list);
            }
        }
        Ok(())
    }

    fn notify_listener(&self,key_str:String,instances:&Vec<Arc<Instance>>,add_list:Vec<Arc<Instance>>,remove_list:Vec<Arc<Instance>>) {
        if add_list.len()==0 && remove_list.len()==0 {
            return;
        }
        let key =ServiceInstanceKey::from_str(&key_str); 
        if let Some(list) = self.listeners.get(&key_str) {
            for item in list {
                item.listener.change(&key, instances,&add_list,&remove_list);
            }
        }
    }

    fn filter_instances(&mut self,params:&QueryInstanceListParams,ctx:&mut actix::Context<Self>) -> Option<Vec<Arc<Instance>>>{
        let key = params.get_key();
        if let Some(instance_warp) = self.instances.get(&key) {
            let mut list = vec![];
            for item in &instance_warp.instances {
                if params.healthy_only && !item.healthy {
                    continue;
                }
                if let Some(clusters) = &params.clusters {
                    let name = &item.cluster_name;
                    if !clusters.contains(name) {
                        continue;
                    }
                }
                list.push(item.clone());
            }
            return Some(list);
            //if list.len()> 0 {
            //    return Some(list);
            //}
        }
        else{
            let addr = ctx.address();
            addr.do_send(NamingListenerCmd::AddHeartbeat(ServiceInstanceKey::from_str(&key)));
        }
        None
    }

    pub fn hb(&self,ctx:&mut actix::Context<Self>) {
        ctx.run_later(Duration::new(1,0), |act,ctx|{
            let current_time = now_millis();
            let addr = ctx.address();
            for key in act.timeout_set.timeout(current_time){
                addr.do_send(NamingListenerCmd::Heartbeat(key,current_time));
            }
            act.hb(ctx);
        });
    }
}

impl Actor for InnerNamingListener {
    type Context = Context<Self>;

    fn started(&mut self,ctx: &mut Self::Context) {
        log::info!(" InnerNamingListener started");
        self.hb(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub enum NamingListenerCmd {
    Add(ServiceInstanceKey,u64,Box<InstanceListener+Send+'static>),
    Remove(ServiceInstanceKey,u64),
    AddHeartbeat(ServiceInstanceKey),
    Heartbeat(String,u64),
}

impl Handler<NamingListenerCmd> for InnerNamingListener {
    type Result = Result<(),std::io::Error>;

    fn handle(&mut self,msg:NamingListenerCmd,ctx:&mut Context<Self>) -> Self::Result  {
        match msg {
            NamingListenerCmd::Add(key,id,listener) => {
                let key_str = key.get_key();
                //如果已经存在，则直接触发一次
                if let Some(instance_wrap) = self.instances.get(&key_str) {
                    if instance_wrap.instances.len() > 0{
                        listener.change(&key, &instance_wrap.instances,&instance_wrap.instances,&vec![]);
                    }
                }
                let listener_value = ListenerValue::new(key.clone(),listener,id);
                if let Some(list) = self.listeners.get_mut(&key_str) {
                    list.push(listener_value);
                }
                else{
                    self.listeners.insert(key_str.clone(), vec![listener_value]);
                    let addr = ctx.address();
                    addr.do_send(NamingListenerCmd::AddHeartbeat(key));
                }
            },
            NamingListenerCmd::AddHeartbeat(key) => {
                let key_str = key.get_key();
                if let Some(_) = self.instances.get(&key_str) {
                    return Ok(());
                }
                else{
                    //println!("======== AddHeartbeat ,key:{}",&key_str);
                    let current_time = now_millis();
                    let mut instances=InstancesWrap::default();
                    instances.params.group_name=key.group_name;
                    instances.params.service_name=key.service_name;
                    instances.params.namespace_id=self.namespace_id.to_owned();
                    instances.params.healthy_only=false;
                    instances.params.client_ip=Some(self.client_ip.clone());
                    instances.params.udp_port = Some(self.udp_port);
                    instances.next_time=current_time;
                    self.instances.insert(key_str.clone(), instances);
                    //self.timeout_set.add(0u64,key_str);
                    let addr = ctx.address();
                    addr.do_send(NamingListenerCmd::Heartbeat(key_str,current_time));
                }
            },
            NamingListenerCmd::Remove(key,id) => {
                let key_str = key.get_key();
                if let Some(list) = self.listeners.get_mut(&key_str) {
                    let mut indexs = Vec::new();
                    for i in 0..list.len() {
                        if let Some(item) = list.get(i){
                            if item.id==id {
                                indexs.push(i);
                            }
                        }
                    }
                    for i in indexs.iter().rev() {
                        list.remove(*i);
                    }
                }
            },
            NamingListenerCmd::Heartbeat(key, time) => {
                let mut is_query=false;
                if let Some(instance_warp) = self.instances.get_mut(&key) {
                    if instance_warp.next_time> time {
                        self.timeout_set.add(instance_warp.next_time,key.clone());
                        return Ok(())
                    }
                    is_query=true;
                    let current_time = now_millis();
                    instance_warp.next_time = current_time+self.period;
                    self.timeout_set.add(instance_warp.next_time,key.clone());
                }
                if is_query {
                    self.query_instance(key, ctx);
                }
            },
        };
        Ok(())
    }
}

impl Handler<UdpDataCmd> for InnerNamingListener {
    type Result = Result<(),std::io::Error>;
    fn handle(&mut self,msg:UdpDataCmd,ctx: &mut Context<Self>) -> Self::Result {
        let data = match Utils::gz_decode(&msg.data){
            Some(data) => data,
            None => msg.data,
        };
        let map:HashMap<String,String> = serde_json::from_slice(&data).unwrap_or_default();
        if let Some(str_data) = map.get("data") {
            let result:QueryListResult=serde_json::from_str(str_data)?;
            let ref_time  = result.lastRefTime.clone().unwrap_or_default();
            let key = result.name.clone().unwrap_or_default();
            //send to client
            let mut map = HashMap::new();
            map.insert("type", "push-ack".to_owned());
            map.insert("lastRefTime",ref_time.to_string());
            map.insert("data","".to_owned());
            let ack = serde_json::to_string(&map).unwrap();
            let send_msg = UdpDataCmd{
                data:ack.as_bytes().to_vec(),
                target_addr:msg.target_addr,
            };
            self.udp_addr.do_send(send_msg);
            //update
            self.update_instances_and_notify(key,result);
        }
        Ok(())
    }
}

type ListenerSenderType = tokio::sync::oneshot::Sender<NamingQueryResult>;
type ListenerReceiverType = tokio::sync::oneshot::Receiver<NamingQueryResult>;

#[derive(Message)]
#[rtype(result = "Result<NamingQueryResult,std::io::Error>")]
pub enum NamingQueryCmd{
    QueryList(QueryInstanceListParams,ListenerSenderType),
    Select(QueryInstanceListParams,ListenerSenderType),
}

pub enum NamingQueryResult {
    None,
    One(Arc<Instance>),
    List(Vec<Arc<Instance>>),
}

impl Handler<NamingQueryCmd> for InnerNamingListener {
    type Result = Result<NamingQueryResult,std::io::Error>;
    fn handle(&mut self,msg:NamingQueryCmd,ctx:&mut Context<Self>) -> Self::Result  {
        match msg {
            NamingQueryCmd::QueryList(param,sender) => {
                if let Some(list) = self.filter_instances(&param,ctx) {
                    sender.send(NamingQueryResult::List(list));
                }
                else{
                    let request_client = self.request_client.clone();
                    async move {
                        (request_client.get_instance_list(&param).await,sender)
                    }
                    .into_actor(self)
                    .map(|(res,sender),_,_|{
                        match res {
                            Ok(list_result) => {
                                if let Some(hosts) = list_result.hosts {
                                    let list:Vec<Arc<Instance>> = hosts.into_iter()
                                        .map(|e| Arc::new(e.to_instance()))
                                        .filter(|e|e.weight>0.001f32)
                                        .collect();
                                    sender.send(NamingQueryResult::List(list));
                                    return;
                                }
                            },
                            Err(_) => {},
                        }
                        sender.send(NamingQueryResult::None);
                    })
                    .spawn(ctx);
                }
            },
            NamingQueryCmd::Select(param,sender) => {
                if let Some(list) = self.filter_instances(&param,ctx) {
                    let index = NamingUtils::select_by_weight_fn(&list, |e| (e.weight*1000f32) as u64); 
                    if let Some(e) = list.get(index) {
                        sender.send(NamingQueryResult::One(e.clone()));
                    }
                    else{
                        sender.send(NamingQueryResult::None);
                    }
                }
                else{
                    let request_client = self.request_client.clone();
                    async move {
                        (request_client.get_instance_list(&param).await,sender)
                    }
                    .into_actor(self)
                    .map(|(res,sender),_,_|{
                        match res {
                            Ok(list_result) => {
                                if let Some(hosts) = list_result.hosts {
                                    let list:Vec<Arc<Instance>> = hosts.into_iter()
                                        .map(|e| Arc::new(e.to_instance()))
                                        .filter(|e|e.weight>0.001f32)
                                        .collect();
                                    let index = NamingUtils::select_by_weight_fn(&list, |e| (e.weight*1000f32) as u64); 
                                    if let Some(e) = list.get(index) {
                                        sender.send(NamingQueryResult::One(e.clone()));
                                    }
                                    return;
                                }
                            },
                            Err(_) => {},
                        }
                        sender.send(NamingQueryResult::None);
                    })
                    .spawn(ctx); 
                }
            },
        }
        Ok(NamingQueryResult::None)
    }
}

pub struct NamingClient{
    pub namespace_id:String,
    register:Addr<InnerNamingRegister>,
    listener_addr:Addr<InnerNamingListener>,
    pub current_ip:String
}

impl Drop for NamingClient {

    fn drop(&mut self) { 
        log::info!("NamingClient droping");
        self.register.do_send(NamingRegisterCmd::Close());
        std::thread::sleep(utils::ms(500));
    }
}

impl NamingClient {
    pub fn new(host:HostInfo,namespace_id:String) -> Arc<Self> {
        let current_ip = match env::var("IP"){
            Ok(v) => v,
            Err(_) => {
                local_ipaddress::get().unwrap()
            },
        };
        let endpoint = Arc::new(ServerEndpointInfo{
            hosts:vec![host]
        });
        let request_client = InnerNamingRequestClient::new_with_endpoint(endpoint);
        let addrs=Self::init_register(namespace_id.clone(),current_ip.clone(),request_client,None);
        Arc::new(Self{
            namespace_id,
            register:addrs.0,
            listener_addr:addrs.1,
            current_ip,
        })
    }

    pub fn new_with_addr(addrs:&str,namespace_id:String,auth_info:Option<AuthInfo>) -> Arc<Self> {
        let endpoint = Arc::new(ServerEndpointInfo::new(addrs));
        let request_client = InnerNamingRequestClient::new_with_endpoint(endpoint);
        let current_ip = match env::var("IP"){
            Ok(v) => v,
            Err(_) => {
                local_ipaddress::get().unwrap()
            },
        };
        let addrs=Self::init_register(namespace_id.clone(),current_ip.clone(),request_client,auth_info);
        Arc::new(Self{
            namespace_id,
            register:addrs.0,
            listener_addr:addrs.1,
            current_ip,
        })
    }

    fn init_register(namespace_id:String,client_ip:String,mut request_client:InnerNamingRequestClient,auth_info:Option<AuthInfo>) -> (Addr<InnerNamingRegister>,Addr<InnerNamingListener>) {
        use tokio::net::{UdpSocket};
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let rt = System::new();
            let endpoint=request_client.endpoints.clone();
            let addrs = rt.block_on(async {
                let auth_addr = AuthActor::new(endpoint,auth_info).start();
                request_client.set_auth_addr(auth_addr);
                let socket=UdpSocket::bind("0.0.0.0:0").await.unwrap();
                let port = socket.local_addr().unwrap().port();
                let new_request_client = request_client.clone();
                let listener_addr=InnerNamingListener::create(move |ctx| {
                    let udp_addr = UdpWorker::new_with_socket(socket, ctx.address()).start();
                    InnerNamingListener::new(&namespace_id,&client_ip,port, new_request_client,udp_addr) 
                });
                (InnerNamingRegister::new(request_client).start(),
                    listener_addr
                )
            });
            tx.send(addrs);
            rt.run();
        });
        let addrs = rx.recv().unwrap();
        addrs
    }

    pub fn register(&self,instance:Instance) {
        self.register.do_send(NamingRegisterCmd::Register(instance));
    }

    pub fn unregister(&self,instance:Instance) {
        self.register.do_send(NamingRegisterCmd::Remove(instance));
    }

    pub async fn query_instances(&self,params:QueryInstanceListParams) -> anyhow::Result<Vec<Arc<Instance>>>{
        let (tx,rx) = tokio::sync::oneshot::channel();
        self.listener_addr.do_send(NamingQueryCmd::QueryList(params,tx));
        match rx.await? {
            NamingQueryResult::List(list) => {
                Ok(list)
            },
            _ => {
                Err(anyhow::anyhow!("not found instance"))
            }
        }
    }

    pub async fn select_instance(&self,params:QueryInstanceListParams) -> anyhow::Result<Arc<Instance>>{
        let (tx,rx) = tokio::sync::oneshot::channel();
        self.listener_addr.do_send(NamingQueryCmd::Select(params,tx));
        match rx.await? {
            NamingQueryResult::One(one) => {
                Ok(one)
            },
            _ => {
                Err(anyhow::anyhow!("not found instance"))
            }
        }
    }

    pub async fn subscribe<T:InstanceListener + Send + 'static>(&self,listener:Box<T>) -> anyhow::Result<()> {
        let key = listener.get_key();
        self.subscribe_with_key(key, listener).await
    }

    pub async fn subscribe_with_key<T:InstanceListener + Send + 'static>(&self,key:ServiceInstanceKey,listener:Box<T>) -> anyhow::Result<()> {
        //let msg=NamingListenerCmd::AddHeartbeat(key.clone());
        //self.listener_addr.do_send(msg);
        let id=0u64;
        /*
        let params = QueryInstanceListParams::new(&self.namespace_id,&key.group_name,&key.service_name,None,true);
        match self.query_instances(params).await {
            Ok(v) => {
                listener.change(&key, &v,&v,&vec![]);
            },
            Err(_) => {},
        };
         */
        let msg=NamingListenerCmd::Add(key,id,listener);
        self.listener_addr.do_send(msg);
        Ok(())
    }

    pub async fn unsubscribe(&self,key:ServiceInstanceKey) -> anyhow::Result<()>{
        let id=0u64;
        let msg = NamingListenerCmd::Remove(key, id);
        self.listener_addr.do_send(msg);
        Ok(())
    }


}