use crate::client::auth::AuthActor;
use crate::client::nacos_client::ActixSystemActorSetCmd;
use crate::client::nacos_client::ActixSystemCmd;
use crate::client::nacos_client::ActixSystemResult;
use crate::client::AuthInfo;
use crate::client::ServerEndpointInfo;
use crate::conn_manage::manage::ConnManage;
use crate::init_global_system_actor;
use std::env;
use std::sync::Arc;

use super::Instance;
use super::InstanceListener;
use super::NamingQueryCmd;
use super::NamingQueryResult;
use super::QueryInstanceListParams;
use super::ServiceInstanceKey;
use super::{
    InnerNamingListener, InnerNamingRegister, InnerNamingRequestClient, NamingListenerCmd,
    NamingRegisterCmd, UdpWorker,
};
use crate::client::{utils, HostInfo};
use actix::prelude::*;
use actix::WeakAddr;

pub struct NamingClient {
    pub namespace_id: String,
    pub(crate) register: Addr<InnerNamingRegister>,
    pub(crate) listener_addr: Addr<InnerNamingListener>,
    pub(crate) _conn_manage_addr: Addr<ConnManage>,
    pub current_ip: String,
}

impl Drop for NamingClient {
    fn drop(&mut self) {
        self.droping();
        std::thread::sleep(utils::ms(500));
    }
}

impl NamingClient {
    pub fn new(host: HostInfo, namespace_id: String) -> Arc<Self> {
        let use_grpc = false;
        let current_ip = match env::var("NACOS_CLIENT_IP") {
            Ok(v) => v,
            Err(_) => local_ipaddress::get().unwrap_or("127.0.0.1".to_owned()),
        };
        let endpoint = Arc::new(ServerEndpointInfo { hosts: vec![host] });
        let conn_manage =
            ConnManage::new(endpoint.hosts.clone(), use_grpc, None, Default::default());
        let conn_manage_addr = conn_manage.start_at_global_system();
        let request_client = InnerNamingRequestClient::new_with_endpoint(endpoint);
        let addrs = Self::init_register(
            namespace_id.clone(),
            current_ip.clone(),
            request_client,
            None,
            Some(conn_manage_addr.clone().downgrade()),
            use_grpc,
        );
        let r = Arc::new(Self {
            namespace_id,
            register: addrs.0,
            listener_addr: addrs.1,
            current_ip,
            _conn_manage_addr: conn_manage_addr,
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastNamingClient(r.clone()));
        r
    }

    pub fn new_with_addrs(
        addrs: &str,
        namespace_id: String,
        auth_info: Option<AuthInfo>,
    ) -> Arc<Self> {
        let use_grpc = false;
        let endpoint = Arc::new(ServerEndpointInfo::new(addrs));
        let conn_manage = ConnManage::new(
            endpoint.hosts.clone(),
            use_grpc,
            auth_info.clone(),
            Default::default(),
        );
        let conn_manage_addr = conn_manage.start_at_global_system();
        let request_client = InnerNamingRequestClient::new_with_endpoint(endpoint);
        let current_ip = match env::var("NACOS_CLIENT_IP") {
            Ok(v) => v,
            Err(_) => local_ipaddress::get().unwrap_or("127.0.0.1".to_owned()),
        };
        let addrs = Self::init_register(
            namespace_id.clone(),
            current_ip.clone(),
            request_client,
            auth_info,
            Some(conn_manage_addr.clone().downgrade()),
            use_grpc,
        );
        let r = Arc::new(Self {
            namespace_id,
            register: addrs.0,
            listener_addr: addrs.1,
            current_ip,
            _conn_manage_addr: conn_manage_addr,
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastNamingClient(r.clone()));
        r
    }

    pub(crate) fn init_register(
        namespace_id: String,
        client_ip: String,
        mut request_client: InnerNamingRequestClient,
        auth_info: Option<AuthInfo>,
        conn_manage_addr: Option<WeakAddr<ConnManage>>,
        use_grpc: bool,
    ) -> (Addr<InnerNamingRegister>, Addr<InnerNamingListener>) {
        let system_addr = init_global_system_actor();
        let endpoint = request_client.endpoints.clone();
        let actor = AuthActor::new(endpoint, auth_info);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::AuthActor(actor, tx);
        system_addr.do_send(msg);
        let auth_addr = match rx.recv().unwrap() {
            ActixSystemResult::AuthActorAddr(auth_addr) => auth_addr,
            _ => panic!("init actor error"),
        };
        request_client.set_auth_addr(auth_addr);

        let actor = InnerNamingRegister::new(use_grpc, conn_manage_addr.clone());
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::InnerNamingRegister(actor, tx);
        system_addr.do_send(msg);
        let register_addr = match rx.recv().unwrap() {
            ActixSystemResult::InnerNamingRegister(addr) => addr,
            _ => panic!("init actor error"),
        };

        let actor = UdpWorker::new(None);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::UdpWorker(actor, tx);
        system_addr.do_send(msg);
        let udp_work_addr = match rx.recv().unwrap() {
            ActixSystemResult::UdpWorker(addr) => addr,
            _ => panic!("init actor error"),
        };

        let actor = InnerNamingListener::new(
            &namespace_id,
            &client_ip,
            0,
            request_client,
            udp_work_addr,
            conn_manage_addr,
            use_grpc,
        );
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::InnerNamingListener(actor, tx);
        system_addr.do_send(msg);
        let listener_addr = match rx.recv().unwrap() {
            ActixSystemResult::InnerNamingListener(addr) => addr,
            _ => panic!("init actor error"),
        };
        (register_addr, listener_addr)
    }

    pub(crate) fn droping(&self) {
        log::info!("NamingClient droping");
        self.register.do_send(NamingRegisterCmd::Close());
        self.listener_addr.do_send(NamingListenerCmd::Close);
    }

    pub fn register(&self, mut instance: Instance) {
        instance.namespace_id = self.namespace_id.clone();
        self.register.do_send(NamingRegisterCmd::Register(instance));
    }

    pub fn unregister(&self, mut instance: Instance) {
        instance.namespace_id = self.namespace_id.clone();
        self.register.do_send(NamingRegisterCmd::Remove(instance));
    }

    pub async fn query_instances(
        &self,
        mut params: QueryInstanceListParams,
    ) -> anyhow::Result<Vec<Arc<Instance>>> {
        params.namespace_id = self.namespace_id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.listener_addr
            .do_send(NamingQueryCmd::QueryList(params, tx));
        match rx.await? {
            NamingQueryResult::List(list) => Ok(list),
            _ => Err(anyhow::anyhow!("not found instance")),
        }
    }

    pub async fn select_instance(
        &self,
        mut params: QueryInstanceListParams,
    ) -> anyhow::Result<Arc<Instance>> {
        params.namespace_id = self.namespace_id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.listener_addr
            .do_send(NamingQueryCmd::Select(params, tx));
        match rx.await? {
            NamingQueryResult::One(one) => Ok(one),
            _ => Err(anyhow::anyhow!("not found instance")),
        }
    }

    pub async fn subscribe<T: InstanceListener + Send + 'static>(
        &self,
        listener: Box<T>,
    ) -> anyhow::Result<()> {
        let key = listener.get_key();
        self.subscribe_with_key(key, listener).await
    }

    pub async fn subscribe_with_key<T: InstanceListener + Send + 'static>(
        &self,
        key: ServiceInstanceKey,
        listener: Box<T>,
    ) -> anyhow::Result<()> {
        //let msg=NamingListenerCmd::AddHeartbeat(key.clone());
        //self.listener_addr.do_send(msg);
        let id = 0u64;
        //如果之前没有数据，会触发加载数据
        let params = QueryInstanceListParams::new(
            &self.namespace_id,
            &key.group_name,
            &key.service_name,
            None,
            true,
        );
        match self.query_instances(params).await {
            Ok(_) => {
                //listener.change(&key, &v,&v,&vec![]);
            }
            Err(_) => {}
        };
        let msg = NamingListenerCmd::Add(key, id, listener);
        self.listener_addr.do_send(msg);
        Ok(())
    }

    pub async fn unsubscribe(&self, key: ServiceInstanceKey) -> anyhow::Result<()> {
        let id = 0u64;
        let msg = NamingListenerCmd::Remove(key, id);
        self.listener_addr.do_send(msg);
        Ok(())
    }
}
