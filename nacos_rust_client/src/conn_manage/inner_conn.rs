use std::{sync::Arc};

use actix::{Addr, Actor};
use tonic::transport::Channel;

use crate::grpc::grpc_client::InnerGrpcClient;

use super::{endpoint::Endpoint, breaker::{Breaker, BreakerConfig}};



#[derive(Default,Debug,Clone)]
pub(crate) struct InnerConn {
    pub id:u32,
    pub endpoint:Endpoint,
    pub breaker:Breaker,
    pub channel: Option<Channel>,
    pub grpc_client_addr:Option<Addr<InnerGrpcClient>>,
}

impl InnerConn {
    pub fn new(id:u32,endpoint:Endpoint,breaker_config: Arc<BreakerConfig>) -> Self {
        Self {
            id,
            endpoint,
            breaker: Breaker::new(Default::default(),breaker_config),
            channel:None,
            grpc_client_addr:None,
        }
    }

    pub fn init_grpc(&mut self) -> anyhow::Result<()>{
        if self.endpoint.support_grpc {
            let addr = format!("http://{}:{}",&self.endpoint.ip,&self.endpoint.grpc_port);
            let channel = Channel::from_shared(addr)?.connect_lazy();
            let grpc_client= InnerGrpcClient::new_by_channel(channel.clone())?;
            self.channel = Some(channel);
            self.grpc_client_addr = Some(grpc_client.start());
        }
        Ok(())
    }

    pub fn close_grpc(&mut self) -> anyhow::Result<()> {
        self.channel=None;
        self.grpc_client_addr=None;
        Ok(())
    }

}