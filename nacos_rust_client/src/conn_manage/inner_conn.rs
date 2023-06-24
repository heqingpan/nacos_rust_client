use std::{sync::Arc};

use actix::{Actor, Addr, WeakAddr};
use tonic::transport::Channel;

use crate::{grpc::grpc_client::InnerGrpcClient, client::{HostInfo, AuthInfo, config_client::inner_client::ConfigInnerRequestClient, naming_client::InnerNamingRequestClient}};

use super::{breaker::{Breaker, BreakerConfig}, manage::ConnManage};



#[derive(Default,Clone)]
pub(crate) struct InnerConn {
    pub id:u32,
    pub host_info:HostInfo,
    pub breaker:Breaker,
    pub support_grpc: bool,
    pub channel: Option<Channel>,
    pub grpc_client_addr:Option<Addr<InnerGrpcClient>>,
    pub manage_addr:Option<WeakAddr<ConnManage>>,
    pub config_request_client:Option<ConfigInnerRequestClient>,
    pub naming_request_client:Option<InnerNamingRequestClient>,
}

impl InnerConn {
    pub fn new(id:u32,host_info:HostInfo,support_grpc: bool,breaker_config: Arc<BreakerConfig>
        ,config_request_client:Option<ConfigInnerRequestClient>,naming_request_client:Option<InnerNamingRequestClient>) -> Self {
        Self {
            id,
            host_info,
            support_grpc,
            breaker: Breaker::new(Default::default(),breaker_config),
            channel:None,
            grpc_client_addr:None,
            manage_addr:None,
            config_request_client,
            naming_request_client,
        }
    }

    pub fn init_grpc(&mut self,manage_addr:WeakAddr<ConnManage>) -> anyhow::Result<()>{
        if self.support_grpc {
            let addr = format!("http://{}:{}",&self.host_info.ip,&self.host_info.grpc_port);
            let channel = Channel::from_shared(addr)?.connect_lazy();
            let grpc_client= InnerGrpcClient::new_by_channel(channel.clone(),manage_addr)?;
            self.channel = Some(channel);
            self.grpc_client_addr = Some(grpc_client.start());
        }
        Ok(())
    }

    pub fn close_grpc(&mut self) -> anyhow::Result<()> {
        if self.support_grpc {
            self.channel=None;
            self.grpc_client_addr=None;
        }
        Ok(())
    }

}