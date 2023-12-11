use std::{collections::HashMap, sync::Arc, time::Duration};

use actix::{prelude::*, WeakAddr};

use crate::{
    client::{
        auth::AuthActor,
        config_client::{
            inner::ConfigInnerCmd, inner_client::ConfigInnerRequestClient, model::NotifyConfigItem,
            ConfigInnerActor,
        },
        get_md5,
        nacos_client::{ActixSystemCmd, ActixSystemResult},
        naming_client::{
            InnerNamingListener, InnerNamingRegister, InnerNamingRequestClient, NamingListenerCmd,
            NamingQueryCmd, NamingRegisterCmd, NamingUtils,
        },
        AuthInfo, ClientInfo, HostInfo, ServerEndpointInfo,
    },
    grpc::grpc_client::InnerGrpcClient,
    init_global_system_actor, ActorCreate,
};

use super::{
    breaker::BreakerConfig,
    conn_msg::{
        ConfigRequest, ConfigResponse, ConnCallbackMsg, NamingRequest, NamingResponse,
        ServiceResult,
    },
    inner_conn::InnerConn,
    NotifyCallbackAddr,
};

#[derive(Default, Clone)]
pub struct ConnManage {
    conns: Vec<InnerConn>,
    conn_map: HashMap<u32, u32>,
    current_index: usize,
    support_grpc: bool,
    auth_info: Option<AuthInfo>,
    conn_globda_id: u32,
    breaker_config: Arc<BreakerConfig>,
    pub(crate) callback: NotifyCallbackAddr,
    reconnecting: bool,
    client_info: Arc<ClientInfo>,
}

impl ConnManage {
    pub fn new(
        hosts: Vec<HostInfo>,
        support_grpc: bool,
        auth_info: Option<AuthInfo>,
        breaker_config: BreakerConfig,
        client_info: Arc<ClientInfo>,
    ) -> Self {
        assert!(hosts.len() > 0);
        let mut id = 0;
        let breaker_config = Arc::new(breaker_config);
        let mut conns = Vec::with_capacity(hosts.len());
        let mut conn_map = HashMap::new();
        for host in hosts {
            let conn = InnerConn::new(
                id,
                host,
                support_grpc,
                breaker_config.clone(),
                client_info.clone(),
            );
            conn_map.insert(id, id);
            id += 1;
            conns.push(conn);
        }
        Self {
            conns,
            conn_map,
            support_grpc,
            auth_info,
            conn_globda_id: id,
            breaker_config,
            client_info,
            ..Default::default()
        }
    }

    pub fn set_client_info(mut self, client_info: Arc<ClientInfo>) -> Self {
        self.client_info = client_info;
        self
    }

    fn init_conn(&mut self, ctx: &mut Context<Self>) {
        self.current_index = self.select_index();
        let conn = self.conns.get_mut(self.current_index).unwrap();
        log::info!(
            "ConnManage init connect,host {}:{}",
            &conn.host_info.ip,
            &conn.host_info.port
        );
        conn.breaker.clear();
        if self.support_grpc {
            let addr = ctx.address().downgrade();
            conn.init_grpc(addr).ok();
        } else {
            Self::init_http_request(conn, &self.auth_info);
        }
    }

    fn init_http_request(conn: &mut InnerConn, auth_info: &Option<AuthInfo>) {
        let endpoints = Arc::new(ServerEndpointInfo {
            hosts: vec![conn.host_info.clone()],
        });
        let auth_actor = AuthActor::new(endpoints.clone(), auth_info.clone());
        let auth_actor_addr = auth_actor.start();
        //config http
        let mut config_client = ConfigInnerRequestClient::new_with_endpoint(endpoints.clone());
        config_client.set_auth_addr(auth_actor_addr.clone());
        conn.config_request_client = Some(Arc::new(config_client));
        //naming http
        let mut naming_client = InnerNamingRequestClient::new_with_endpoint(endpoints);
        naming_client.set_auth_addr(auth_actor_addr);
        conn.naming_request_client = Some(Arc::new(naming_client));
    }

    pub fn start_at_global_system(self) -> Addr<Self> {
        let system_addr = init_global_system_actor();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::ConnManage(self, tx);
        system_addr.do_send(msg);
        if let ActixSystemResult::ConnManage(addr) = rx.recv().unwrap() {
            addr
        } else {
            panic!("create manage actor error")
        }
    }

    fn select_index(&self) -> usize {
        NamingUtils::select_by_weight_fn(&self.conns, |e| {
            if e.breaker.is_close() {
                1000
            } else if e.breaker.is_half_open() {
                10
            } else {
                1
            }
        })
    }

    fn reconnect(&mut self, old_index: usize, ctx: &mut Context<Self>) {
        if self.reconnecting || self.current_index != old_index {
            //log::debug!("ConnManage reconnect,ignore repeated");
            //已经重链过
            return;
        }
        self.reconnecting = true;
        ctx.run_later(Duration::from_millis(1000), move |act, ctx| {
            log::info!("ConnManage reconnect");
            if act.conns.len() == 1 {
                act.init_conn(ctx);
            } else {
                if let Some(conn) = act.conns.get_mut(old_index) {
                    conn.close_grpc().ok();
                    conn.weight = 0;
                }
                act.init_conn(ctx);
                if let Some(conn) = act.conns.get_mut(old_index) {
                    conn.weight = 1;
                }
            }
            act.reconnect_notify(ctx);
            act.reconnecting = false;
        });
    }

    fn reconnect_notify(&mut self, _ctx: &mut Context<Self>) {
        if !self.support_grpc {
            return;
        }
        if let Some(config_addr) = &self.callback.config_inner_addr {
            if let Some(config_addr) = config_addr.upgrade() {
                config_addr.do_send(ConfigInnerCmd::GrpcResubscribe);
            }
        }
        if let Some(naming_register_addr) = &self.callback.naming_register_addr {
            if let Some(naming_register_addr) = naming_register_addr.upgrade() {
                naming_register_addr.do_send(NamingRegisterCmd::Reregister);
            }
        }
        if let Some(naming_listener_addr) = &self.callback.naming_listener_addr {
            if let Some(naming_listener_addr) = naming_listener_addr.upgrade() {
                naming_listener_addr.do_send(NamingListenerCmd::GrpcResubscribe);
            }
        }
    }

    fn check_reconnect(
        &mut self,
        current_index: usize,
        request_is_ok: bool,
        ctx: &mut Context<Self>,
    ) {
        let can_try = if let Some(conn) = self.conns.get_mut(current_index) {
            if request_is_ok {
                conn.breaker.success();
                true
            } else {
                conn.breaker.error();
                conn.breaker.can_try()
            }
        } else {
            true
        };
        if !can_try {
            self.reconnect(current_index, ctx);
        }
    }

    async fn do_config_request(
        msg: ConfigRequest,
        support_grpc: bool,
        conn_addr: Option<Addr<InnerGrpcClient>>,
        config_client: Option<Arc<ConfigInnerRequestClient>>,
    ) -> anyhow::Result<ConfigResponse> {
        if support_grpc {
            if let Some(conn_addr) = conn_addr {
                if let Ok(Ok(res)) = conn_addr.send(msg).await {
                    Ok(res)
                } else {
                    Err(anyhow::anyhow!("grpc request error"))
                }
            } else {
                Err(anyhow::anyhow!("grpc conn is empty"))
            }
        } else {
            if let Some(config_client) = config_client {
                match msg {
                    ConfigRequest::GetConfig(config_key) => {
                        let value = config_client.get_config(&config_key).await?;
                        let md5 = get_md5(&value);
                        Ok(ConfigResponse::ConfigValue(value, md5))
                    }
                    ConfigRequest::SetConfig(config_key, value) => {
                        config_client.set_config(&config_key, &value).await?;
                        Ok(ConfigResponse::None)
                    }
                    ConfigRequest::DeleteConfig(config_key) => {
                        config_client.del_config(&config_key).await?;
                        Ok(ConfigResponse::None)
                    }
                    ConfigRequest::V1Listen(content) => {
                        let config_keys = config_client.listene(&content, None).await?;
                        Ok(ConfigResponse::ChangeKeys(config_keys))
                    }
                    ConfigRequest::Listen(_, _) => Err(anyhow::anyhow!("http not support")),
                }
            } else {
                Err(anyhow::anyhow!("config client is empty"))
            }
        }
    }

    async fn do_naming_request(
        msg: NamingRequest,
        support_grpc: bool,
        conn_addr: Option<Addr<InnerGrpcClient>>,
        naming_client: Option<Arc<InnerNamingRequestClient>>,
    ) -> anyhow::Result<NamingResponse> {
        if support_grpc {
            if let Some(conn_addr) = conn_addr {
                let res: NamingResponse = conn_addr.send(msg).await??;
                Ok(res)
            } else {
                Err(anyhow::anyhow!("grpc conn is empty"))
            }
        } else {
            if let Some(naming_client) = naming_client {
                match msg {
                    NamingRequest::Register(instance) => {
                        naming_client.register(&instance).await?;
                        Ok(NamingResponse::None)
                    }
                    NamingRequest::Unregister(instance) => {
                        naming_client.remove(&instance).await?;
                        Ok(NamingResponse::None)
                    }
                    NamingRequest::BatchRegister(_) => Err(anyhow::anyhow!("http not support")),
                    NamingRequest::Subscribe(_) => Err(anyhow::anyhow!("http not support")),
                    NamingRequest::Unsubscribe(_) => Err(anyhow::anyhow!("http not support")),
                    NamingRequest::QueryInstance(param) => {
                        let result = naming_client.get_instance_list(&param).await?;
                        let hosts = result
                            .hosts
                            .unwrap_or_default()
                            .into_iter()
                            .map(|e| Arc::new(e.to_instance()))
                            .collect();
                        let service_result = ServiceResult {
                            hosts,
                            cache_millis: result.cache_millis,
                        };
                        Ok(NamingResponse::ServiceResult(service_result))
                    }
                    NamingRequest::V1Heartbeat(heartbeat) => {
                        naming_client.heartbeat(heartbeat).await?;
                        Ok(NamingResponse::None)
                    }
                }
            } else {
                Err(anyhow::anyhow!("naming client is empty"))
            }
        }
    }
}

impl ActorCreate for ConnManage {
    fn create(&self) -> () {
        ()
    }
}

impl Actor for ConnManage {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("ConnManage started");
        self.init_conn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum ConnManageCmd {
    ConfigInnerActorAddr(WeakAddr<ConfigInnerActor>),
    NamingListenerActorAddr(WeakAddr<InnerNamingListener>),
    NamingRegisterActorAddr(WeakAddr<InnerNamingRegister>),
    GrpcRequestCheckError { id: u32 },
}

impl Handler<ConnManageCmd> for ConnManage {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: ConnManageCmd, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ConnManageCmd::ConfigInnerActorAddr(addr) => {
                self.callback.config_inner_addr = Some(addr);
            }
            ConnManageCmd::NamingListenerActorAddr(addr) => {
                self.callback.naming_listener_addr = Some(addr);
            }
            ConnManageCmd::NamingRegisterActorAddr(addr) => {
                self.callback.naming_register_addr = Some(addr);
            }
            ConnManageCmd::GrpcRequestCheckError { id } => self.reconnect(id as usize, ctx),
        }
        Ok(())
    }
}

impl Handler<ConnCallbackMsg> for ConnManage {
    type Result = ResponseActFuture<Self, anyhow::Result<()>>;
    fn handle(&mut self, msg: ConnCallbackMsg, _ctx: &mut Self::Context) -> Self::Result {
        let callback = self.callback.clone();
        let fut = async move {
            match msg {
                ConnCallbackMsg::ConfigChange(config_key, content, md5) => {
                    if let Some(config_addr) = callback.config_inner_addr {
                        if let Some(config_addr) = config_addr.upgrade() {
                            config_addr.do_send(ConfigInnerCmd::Notify(vec![NotifyConfigItem {
                                key: config_key,
                                content,
                                md5,
                            }]));
                        }
                    }
                }
                ConnCallbackMsg::InstanceChange(key, service_result) => {
                    if let Some(config_addr) = callback.naming_listener_addr {
                        if let Some(config_addr) = config_addr.upgrade() {
                            config_addr.do_send(NamingQueryCmd::ChangeResult(key, service_result));
                        }
                    }
                }
            }
            //...
            Ok(())
        }
        .into_actor(self)
        .map(|r, _act, _ctx| r);
        Box::pin(fut)
    }
}

impl Handler<ConfigRequest> for ConnManage {
    type Result = ResponseActFuture<Self, anyhow::Result<ConfigResponse>>;

    fn handle(&mut self, msg: ConfigRequest, _ctx: &mut Self::Context) -> Self::Result {
        let conn = self.conns.get(self.current_index).unwrap();
        let conn_addr = conn.grpc_client_addr.clone();
        let current_index = self.current_index.to_owned();
        let support_grpc = self.support_grpc;
        let config_client = conn.config_request_client.clone();
        let fut = async move {
            let r = Self::do_config_request(msg, support_grpc, conn_addr, config_client).await;
            (r, current_index)
        }
        .into_actor(self)
        .map(|(r, current_index), act, ctx| {
            act.check_reconnect(current_index, r.is_ok(), ctx);
            r
        });
        Box::pin(fut)
    }
}

impl Handler<NamingRequest> for ConnManage {
    type Result = ResponseActFuture<Self, anyhow::Result<NamingResponse>>;

    fn handle(&mut self, msg: NamingRequest, ctx: &mut Self::Context) -> Self::Result {
        let conn = self.conns.get(self.current_index).unwrap();
        let conn_addr = conn.grpc_client_addr.clone();
        let support_grpc = self.support_grpc;
        let naming_client = conn.naming_request_client.clone();
        let current_index = self.current_index.to_owned();
        let fut = async move {
            let r = Self::do_naming_request(msg, support_grpc, conn_addr, naming_client).await;
            (r, current_index)
        }
        .into_actor(self)
        .map(|(r, current_index), act, ctx| {
            act.check_reconnect(current_index, r.is_ok(), ctx);
            r
        });
        Box::pin(fut)
    }
}
