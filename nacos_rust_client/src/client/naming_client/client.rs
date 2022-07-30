use crate::client::nacos_client::ActixSystemActorSetCmd;
use crate::client::nacos_client::ActixSystemCmd;
use crate::client::nacos_client::ActixSystemResult;
use crate::init_global_system_actor;
use crate::client::auth::AuthActor;
use crate::client::AuthInfo;
use crate::client::ServerEndpointInfo;
use std::sync::Arc;
use std::env;

use crate::client::{HostInfo, utils};
use actix::prelude::*;
use super::Instance;
use super::InstanceListener;
use super::NamingQueryCmd;
use super::NamingQueryResult;
use super::QueryInstanceListParams;
use super::ServiceInstanceKey;
use super::udp_actor::UdpWorkerCmd;
use super::{InnerNamingRegister,InnerNamingListener
    ,NamingListenerCmd,NamingRegisterCmd,InnerNamingRequestClient
    ,UdpWorker
};

pub struct NamingClient{
    pub namespace_id:String,
    pub(crate) register:Addr<InnerNamingRegister>,
    pub(crate) listener_addr:Addr<InnerNamingListener>,
    pub current_ip:String
}

impl Drop for NamingClient {

    fn drop(&mut self) { 
        self.droping();
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
        let addrs=Self::init_register2(namespace_id.clone(),current_ip.clone(),request_client,None);
        let r=Arc::new(Self{
            namespace_id,
            register:addrs.0,
            listener_addr:addrs.1,
            current_ip,
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastNamingClient(r.clone()));
        r
    }

    pub fn new_with_addrs(addrs:&str,namespace_id:String,auth_info:Option<AuthInfo>) -> Arc<Self> {
        let endpoint = Arc::new(ServerEndpointInfo::new(addrs));
        let request_client = InnerNamingRequestClient::new_with_endpoint(endpoint);
        let current_ip = match env::var("IP"){
            Ok(v) => v,
            Err(_) => {
                local_ipaddress::get().unwrap()
            },
        };
        let addrs=Self::init_register2(namespace_id.clone(),current_ip.clone(),request_client,auth_info);
        let r = Arc::new(Self{
            namespace_id,
            register:addrs.0,
            listener_addr:addrs.1,
            current_ip,
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastNamingClient(r.clone()));
        r
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
                //let socket=UdpSocket::bind("0.0.0.0:0").await.unwrap();
                //let port = socket.local_addr().unwrap().port();
                let port = 0;
                //let udp_addr = UdpWorker::new(None).start();
                //let listener_addr = InnerNamingListener::new(&namespace_id,&client_ip,port, new_request_client,udp_addr).start();
                let new_request_client = request_client.clone();
                let listener_addr=InnerNamingListener::create(move |ctx| {
                    //let udp_addr = UdpWorker::new_with_socket(socket, Some(ctx.address())).start();
                    let udp_addr = UdpWorker::new(Some(ctx.address())).start();
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

    fn init_register2(namespace_id:String,client_ip:String,mut request_client:InnerNamingRequestClient,auth_info:Option<AuthInfo>) 
        -> (Addr<InnerNamingRegister>,Addr<InnerNamingListener>) {
        let system_addr =  init_global_system_actor();
        let mut endpoint=request_client.endpoints.clone();
        let actor = AuthActor::new(endpoint,auth_info);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::AuthActor(actor,tx);
        system_addr.do_send(msg);
        let auth_addr= match rx.recv().unwrap() {
            ActixSystemResult::AuthActorAddr(auth_addr) => auth_addr,
            _ => panic!("init actor error"),
        };
        request_client.set_auth_addr(auth_addr);

        let actor = InnerNamingRegister::new(request_client.clone());
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::InnerNamingRegister(actor,tx);
        system_addr.do_send(msg);
        let register_addr= match rx.recv().unwrap() {
            ActixSystemResult::InnerNamingRegister(addr) => addr,
            _ => panic!("init actor error"),
        };

        let actor = UdpWorker::new(None);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::UdpWorker(actor,tx);
        system_addr.do_send(msg);
        let udp_work_addr= match rx.recv().unwrap() {
            ActixSystemResult::UdpWorker(addr) => addr,
            _ => panic!("init actor error"),
        };

        let actor = InnerNamingListener::new(&namespace_id,&client_ip,0, request_client,udp_work_addr);
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::InnerNamingListener(actor,tx);
        system_addr.do_send(msg);
        let listener_addr= match rx.recv().unwrap() {
            ActixSystemResult::InnerNamingListener(addr) => addr,
            _ => panic!("init actor error"),
        };
        (register_addr,listener_addr)

    }

    pub(crate) fn droping(&self){
        log::info!("NamingClient droping");
        self.register.do_send(NamingRegisterCmd::Close());
        self.listener_addr.do_send(NamingListenerCmd::Close);
    }

    pub fn register(&self,mut instance:Instance) {
        instance.namespace_id=self.namespace_id.clone();
        self.register.do_send(NamingRegisterCmd::Register(instance));
    }

    pub fn unregister(&self,mut instance:Instance) {
        instance.namespace_id=self.namespace_id.clone();
        self.register.do_send(NamingRegisterCmd::Remove(instance));
    }

    pub async fn query_instances(&self,mut params:QueryInstanceListParams) -> anyhow::Result<Vec<Arc<Instance>>>{
        params.namespace_id=self.namespace_id.clone();
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

    pub async fn select_instance(&self,mut params:QueryInstanceListParams) -> anyhow::Result<Arc<Instance>>{
        params.namespace_id=self.namespace_id.clone();
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
        //如果之前没有数据，会触发加载数据
        let params = QueryInstanceListParams::new(&self.namespace_id,&key.group_name,&key.service_name,None,true);
        match self.query_instances(params).await {
            Ok(v) => {
                //listener.change(&key, &v,&v,&vec![]);
            },
            Err(_) => {},
        };
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