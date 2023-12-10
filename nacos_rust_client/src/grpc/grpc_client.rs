use std::{collections::HashMap, sync::Arc, time::Duration};

use actix::{prelude::*, WeakAddr};
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use crate::{
    client::{config_client::ConfigKey, naming_client::ServiceInstanceKey},
    conn_manage::{
        conn_msg::{
            ConfigRequest, ConfigResponse, ConnCallbackMsg, NamingRequest, NamingResponse,
            ServiceResult,
        },
        manage::{ConnManage, ConnManageCmd},
    },
    grpc::{
        api_model::{BaseResponse, ConnectionSetupRequest},
        channel::CloseableChannel,
        utils::PayloadUtils,
    },
};

use super::{
    api_model::{ConfigChangeNotifyRequest, NotifySubscriberRequest},
    config_request_utils::GrpcConfigRequestUtils,
    constant::*,
    nacos_proto::{
        bi_request_stream_client::BiRequestStreamClient, request_client::RequestClient, Payload,
    },
    naming_request_utils::GrpcNamingRequestUtils,
};

//type SenderType = tokio::sync::mpsc::Sender<Result<Payload, tonic::Status>>;
type ReceiverStreamType = tonic::Streaming<Payload>;
type BiStreamSenderType = tokio::sync::mpsc::Sender<Option<Payload>>;
type PayloadSenderType = tokio::sync::oneshot::Sender<Result<Payload, String>>;

#[derive(Clone)]
pub struct InnerGrpcClient {
    id: u32,
    channel: Channel,
    //bi_request_stream_client: BiRequestStreamClient<Channel>,
    //request_client: RequestClient<Channel>,
    stream_sender: Option<BiStreamSenderType>,
    stream_reader: bool,
    conn_reader: bool,
    manage_addr: WeakAddr<ConnManage>,
    request_id: u64,
    error_time: u8,
}

impl InnerGrpcClient {
    pub fn new(id: u32, addr: String, manage_addr: WeakAddr<ConnManage>) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(addr)?.connect_lazy();
        Self::new_by_channel(id, channel, manage_addr)
    }

    pub fn new_by_channel(
        id: u32,
        channel: Channel,
        manage_addr: WeakAddr<ConnManage>,
    ) -> anyhow::Result<Self> {
        //let bi_request_stream_client = BiRequestStreamClient::new(channel.clone());
        //let request_client = RequestClient::new(channel.clone());
        Ok(Self {
            id,
            channel,
            //bi_request_stream_client,
            //request_client,
            stream_sender: Default::default(),
            stream_reader: false,
            conn_reader: false,
            manage_addr,
            request_id: 0,
            error_time: 0,
        })
    }

    fn next_request_id(&mut self) -> String {
        if self.request_id >= 0x7fff_ffff_ffff_ffff {
            self.request_id = 0;
        } else {
            self.request_id += 1;
        }
        self.request_id.to_string()
    }

    fn conn_bi_stream(&mut self, ctx: &mut Context<Self>) -> anyhow::Result<()> {
        let (tx, rx) = tokio::sync::mpsc::channel(5);
        let r_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let r_stream = CloseableChannel::new(r_stream);
        let req = tonic::Request::new(r_stream);
        let channel = self.channel.clone();
        self.stream_sender = Some(tx);
        async move {
            /*
            let val="{}";
            let payload=PayloadUtils::build_payload("ServerCheckRequest", val.to_string());
            let  mut request_client = RequestClient::new(channel.clone());
            let response =request_client.request(tonic::Request::new(payload)).await;
            let res = response.unwrap().into_inner();
            log::info!("first check response:{}",&PayloadUtils::get_payload_string(&res));
            */

            let mut bi_request_stream_client = BiRequestStreamClient::new(channel);
            let response = bi_request_stream_client.request_bi_stream(req).await;
            response
        }
        .into_actor(self)
        .map(|response, actor, ctx| {
            match response {
                Ok(response) => {
                    let stream = response.into_inner();
                    actor.stream_reader = true;
                    //actor.bi_stream_setup(ctx);
                    actor.receive_bi_stream(ctx, stream);
                }
                Err(err) => {
                    log::error!("conn_bi_stream error,{:?}", &err);
                    ctx.stop();
                }
            };
        })
        .spawn(ctx);
        Ok(())
    }

    fn wait_check_register(&mut self, ctx: &mut Context<Self>) {
        let channel = self.channel.clone();
        async move {
            for _ in 0..100 {
                match GrpcConfigRequestUtils::check_register(channel.clone()).await {
                    Ok(r) => {
                        if r {
                            return true;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(err) => {
                        log::error!("check_register error,{}", err.to_string());
                        return false;
                    }
                }
            }
            return false;
        }
        .into_actor(self)
        .map(|_r, act, _ctx| {
            //TODO 如果注册失败，触发重新建立链接
            //act.conn_reader=r;
            act.conn_reader = true;
        })
        .wait(ctx);
    }

    fn bi_stream_setup(&mut self, ctx: &mut Context<Self>) {
        let tx = self.stream_sender.clone().unwrap();
        async move {
            let mut setup_request = ConnectionSetupRequest::default();
            setup_request
                .labels
                .insert("AppName".to_owned(), "rust_nacos_client".to_owned());
            setup_request
                .labels
                .insert(LABEL_MODULE.to_owned(), LABEL_MODULE_NAMING.to_owned());
            //setup_request.labels.insert(LABEL_MODULE.to_owned(), LABEL_MODULE_CONFIG.to_owned());
            setup_request.client_version = Some("0.3.".to_owned());
            match tx
                .send(Some(PayloadUtils::build_full_payload(
                    "ConnectionSetupRequest",
                    serde_json::to_string(&setup_request).unwrap(),
                    "127.0.0.1",
                    HashMap::new(),
                )))
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    log::error!(
                        "ConnectionSetupRequest error,{}",
                        &PayloadUtils::get_payload_string(&err.0.unwrap())
                    );
                }
            }
            //manage.do_send(BiStreamManageCmd::ConnClose(client_id));
        }
        .into_actor(self)
        .map(|_, _, ctx| {
            ctx.run_later(Duration::from_millis(50), |act, inner_ctx| {
                act.wait_check_register(inner_ctx);
            });
        })
        //.wait(ctx);
        .wait(ctx);
    }

    async fn do_config_change_notify(
        channel: Channel,
        request_id: String,
        manage_addr: &WeakAddr<ConnManage>,
        config_key: ConfigKey,
    ) -> anyhow::Result<()> {
        //debug
        //log::info!( "config change notify:{}#{}#{}", &config_key.data_id, &config_key.group, &config_key.tenant);
        if let ConfigResponse::ConfigValue(content, md5) =
            GrpcConfigRequestUtils::config_query(channel, Some(request_id), config_key.clone())
                .await?
        {
            let msg = ConnCallbackMsg::ConfigChange(config_key, content, md5);
            if let Some(addr) = manage_addr.upgrade() {
                addr.do_send(msg);
            }
        };
        Ok(())
    }

    fn receive_bi_stream(
        &mut self,
        ctx: &mut Context<Self>,
        mut receiver_stream: ReceiverStreamType,
    ) {
        //let addr = ctx.address();
        let channel = self.channel.clone();
        let tx = self.stream_sender.clone().unwrap();
        let manage_addr = self.manage_addr.clone();
        async move {
            let mut stream_id = 0u128;
            while let Some(item) = receiver_stream.next().await {
                if let Ok(payload) = item {
                    //debug
                    //log::info!( "receive_bi_stream,{}", &PayloadUtils::get_payload_string(&payload));
                    if let Some(t) = PayloadUtils::get_metadata_type(&payload.metadata) {
                        let body_vec = payload.body.unwrap_or_default().value;
                        if t == "ConfigChangeNotifyRequest" {
                            match serde_json::from_slice::<ConfigChangeNotifyRequest>(&body_vec) {
                                Ok(request) => {
                                    let config_key = ConfigKey {
                                        data_id: request.data_id,
                                        group: request.group,
                                        tenant: request.tenant.unwrap_or_default(),
                                    };
                                    let request_id = stream_id.to_string();
                                    stream_id += 1;
                                    Self::do_config_change_notify(
                                        channel.clone(),
                                        request_id,
                                        &manage_addr,
                                        config_key,
                                    )
                                    .await
                                    .ok();
                                    let response =
                                        BaseResponse::build_with_request_id(request.request_id);
                                    let val = serde_json::to_string(&response).unwrap();
                                    let res_payload = PayloadUtils::build_payload(
                                        "ConfigChangeNotifyResponse",
                                        val,
                                    );
                                    tx.send(Some(res_payload)).await.ok();
                                }
                                Err(e) => {
                                    log::error!("ConfigChangeNotifyRequest error {}", e);
                                }
                            }
                        } else if t == "NotifySubscriberRequest" {
                            match serde_json::from_slice::<NotifySubscriberRequest>(&body_vec) {
                                Ok(request) => {
                                    let request_id = request.request_id.clone();
                                    if let Some(manage_addr) = (&manage_addr).upgrade() {
                                        let (service_key, service_result) =
                                            Self::convert_to_service_result(request);
                                        manage_addr.do_send(ConnCallbackMsg::InstanceChange(
                                            service_key,
                                            service_result,
                                        ));
                                    }
                                    let response = BaseResponse::build_with_request_id(request_id);
                                    let val = serde_json::to_string(&response).unwrap();
                                    let res_payload = PayloadUtils::build_payload(
                                        "NotifySubscriberResponse",
                                        val,
                                    );
                                    tx.send(Some(res_payload)).await.ok();
                                }
                                Err(e) => {
                                    log::error!("NotifySubscriberRequest error {}", e);
                                }
                            };
                        }
                    }
                } else {
                    break;
                }
            }
            //manage.do_send(BiStreamManageCmd::ConnClose(client_id));
        }
        .into_actor(self)
        .map(|_, _, ctx| {
            ctx.stop();
        })
        .spawn(ctx);
    }

    fn do_request(
        &mut self,
        ctx: &mut Context<Self>,
        payload: Payload,
        sender: Option<PayloadSenderType>,
    ) {
        let channel = self.channel.clone();
        async move {
            let mut request_client = RequestClient::new(channel);
            let response = request_client.request(tonic::Request::new(payload)).await;
            match response {
                Ok(response) => {
                    let res = response.into_inner();
                    //debug
                    //log::info!("check response:{}", &PayloadUtils::get_payload_header(&res));
                    if let Some(sender) = sender {
                        sender.send(Ok(res)).ok();
                    }
                    true
                }
                Err(err) => {
                    if let Some(sender) = sender {
                        sender.send(Err("request error".to_owned())).ok();
                    }
                    log::error!("do_request error, {:?}", &err);
                    false
                }
            }
        }
        .into_actor(self)
        .map(|r, ctx, _| {
            if !r {
                ctx.error_time += 1;
                if ctx.error_time > 1 {
                    log::warn!("GrpcRequestCheckError");
                    if let Some(manage_conn) = ctx.manage_addr.upgrade() {
                        manage_conn.do_send(ConnManageCmd::GrpcRequestCheckError { id: ctx.id });
                    }
                    ctx.error_time = 0;
                }
            } else {
                ctx.error_time = 0;
            }
        })
        .spawn(ctx);
    }

    fn convert_to_service_result(
        request: NotifySubscriberRequest,
    ) -> (ServiceInstanceKey, ServiceResult) {
        if let Some(service_info) = request.service_info {
            let service_key = ServiceInstanceKey {
                namespace_id: request.namespace,
                group_name: service_info.group_name.clone().unwrap_or_default(),
                service_name: service_info.name.clone().unwrap_or_default(),
            };
            let hosts = service_info
                .hosts
                .unwrap_or_default()
                .into_iter()
                .map(|e| Arc::new(GrpcNamingRequestUtils::convert_to_instance(e, &service_key)))
                .collect();
            (
                service_key,
                ServiceResult {
                    hosts,
                    cache_millis: Some(service_info.cache_millis as u64),
                },
            )
        } else {
            (Default::default(), Default::default())
        }
    }

    fn check_heartbeat(&mut self, ctx: &mut Context<Self>) {
        if self.stream_reader {
            let val = "{}";
            let payload = PayloadUtils::build_payload("ServerCheckRequest", val.to_string());
            self.do_request(ctx, payload, None);
        }
    }

    pub fn heartbeat(&self, ctx: &mut actix::Context<Self>) {
        ctx.run_later(Duration::from_millis(5000), |act, ctx| {
            act.check_heartbeat(ctx);
            act.heartbeat(ctx);
        });
    }
}

impl Actor for InnerGrpcClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("InnerGrpcClient started");
        self.conn_bi_stream(ctx).ok();
        self.bi_stream_setup(ctx);
        self.heartbeat(ctx);
    }
}

#[derive(Debug, Message)]
#[rtype(result = "anyhow::Result<InnerGrpcClientResult>")]
pub enum InnerGrpcClientCmd {
    ReceiverStreamItem(Payload),
    Request(Payload, Option<PayloadSenderType>),
    Ping,
}

pub enum InnerGrpcClientResult {
    None,
}

impl Handler<InnerGrpcClientCmd> for InnerGrpcClient {
    type Result = anyhow::Result<InnerGrpcClientResult>;

    fn handle(&mut self, msg: InnerGrpcClientCmd, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            InnerGrpcClientCmd::ReceiverStreamItem(payload) => Ok(InnerGrpcClientResult::None),
            InnerGrpcClientCmd::Ping => Ok(InnerGrpcClientResult::None),
            InnerGrpcClientCmd::Request(payload, sender) => {
                self.do_request(ctx, payload, sender);
                Ok(InnerGrpcClientResult::None)
            }
        }
    }
}

impl Handler<ConfigRequest> for InnerGrpcClient {
    type Result = ResponseActFuture<Self, anyhow::Result<ConfigResponse>>;

    fn handle(&mut self, config_request: ConfigRequest, ctx: &mut Self::Context) -> Self::Result {
        let channel = self.channel.clone();
        let manage_addr = self.manage_addr.clone();
        let conn_reader = self.conn_reader;
        let request_id = self.next_request_id();
        let fut = async move {
            if !conn_reader {
                //等链接确认后再请求
                log::info!("wait check register,than handle msg");
                tokio::time::sleep(Duration::from_millis(60)).await;
            }
            match config_request {
                ConfigRequest::GetConfig(config_key) => {
                    return GrpcConfigRequestUtils::config_query(
                        channel,
                        Some(request_id),
                        config_key,
                    )
                    .await;
                }
                ConfigRequest::SetConfig(config_key, content) => {
                    let res = GrpcConfigRequestUtils::config_publish(
                        channel.clone(),
                        Some(request_id),
                        config_key.clone(),
                        content,
                    )
                    .await;
                    /*
                    Self::do_config_change_notify(
                        channel.clone(),
                        &manage_addr,
                        config_key,
                    )
                    .await
                    .ok();
                    */
                    return res;
                }
                ConfigRequest::DeleteConfig(config_key) => {
                    let res = GrpcConfigRequestUtils::config_remove(
                        channel.clone(),
                        Some(request_id),
                        config_key.clone(),
                    )
                    .await;
                    /*
                    Self::do_config_change_notify(
                        channel.clone(),
                        &manage_addr,
                        config_key,
                    )
                    .await
                    .ok();
                    */
                    return res;
                }
                ConfigRequest::V1Listen(_) => {
                    return Err(anyhow::anyhow!("grpc not support"));
                }
                ConfigRequest::Listen(listen_items, listen) => {
                    //println!("grpc Listen");
                    let res = GrpcConfigRequestUtils::config_change_batch_listen(
                        channel.clone(),
                        Some(request_id),
                        listen_items,
                        listen,
                    )
                    .await?;
                    if let ConfigResponse::ChangeKeys(keys) = res {
                        for config_key in keys {
                            Self::do_config_change_notify(
                                channel.clone(),
                                config_key.build_key(),
                                &manage_addr,
                                config_key,
                            )
                            .await
                            .ok();
                        }
                    }
                    return Ok(ConfigResponse::None);
                }
            }
        }
        .into_actor(self)
        .map(|r, _, _| r);
        Box::pin(fut)
    }
}

impl Handler<NamingRequest> for InnerGrpcClient {
    type Result = ResponseActFuture<Self, anyhow::Result<NamingResponse>>;

    fn handle(&mut self, request: NamingRequest, ctx: &mut Self::Context) -> Self::Result {
        let channel = self.channel.clone();
        let conn_reader = self.conn_reader;
        let manage_addr = self.manage_addr.clone();
        let fut = async move {
            if !conn_reader {
                //等链接确认后再请求
                log::info!("wait check register,than handle msg");
                tokio::time::sleep(Duration::from_millis(60)).await;
            }
            match request {
                NamingRequest::Register(instance) => {
                    GrpcNamingRequestUtils::instance_register(channel, instance, true).await
                }
                NamingRequest::Unregister(instance) => {
                    GrpcNamingRequestUtils::instance_register(channel, instance, false).await
                }
                NamingRequest::BatchRegister(instances) => {
                    GrpcNamingRequestUtils::batch_register(channel, instances).await
                }
                NamingRequest::Subscribe(service_keys) => {
                    let mut res = Ok(NamingResponse::None);
                    for service_key in service_keys {
                        res = GrpcNamingRequestUtils::subscribe(
                            channel.clone(),
                            service_key.clone(),
                            true,
                            Some("".to_owned()),
                        )
                        .await;
                        if let Ok(res) = &res {
                            if let NamingResponse::ServiceResult(service_result) = res {
                                if let Some(manage_addr) = (&manage_addr).upgrade() {
                                    manage_addr.do_send(ConnCallbackMsg::InstanceChange(
                                        service_key,
                                        service_result.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    res
                }
                NamingRequest::Unsubscribe(service_keys) => {
                    let mut res = Ok(NamingResponse::None);
                    for service_key in service_keys {
                        res = GrpcNamingRequestUtils::subscribe(
                            channel.clone(),
                            service_key,
                            false,
                            Some("".to_owned()),
                        )
                        .await;
                    }
                    res
                }
                NamingRequest::QueryInstance(param) => {
                    let service_key = param.build_key();
                    let clusters = if let Some(clusters) = param.clusters {
                        Some(clusters.join(","))
                    } else {
                        Some("".to_owned())
                    };
                    GrpcNamingRequestUtils::query_service(
                        channel,
                        service_key,
                        clusters,
                        Some(param.healthy_only),
                    )
                    .await
                }
                NamingRequest::V1Heartbeat(_) => todo!(),
            }
            //Ok(NamingResponse::None)
        }
        .into_actor(self)
        .map(|r, act, ctx| r);
        Box::pin(fut)
    }
}
