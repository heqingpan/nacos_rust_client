use actix::prelude::*;
use tonic::transport::Channel;

use super::nacos_proto::{bi_request_stream_client::BiRequestStreamClient, request_client::RequestClient};

pub struct InnerGrpcClient{
    bi_request_stream_client: BiRequestStreamClient<Channel>,
    request_client: RequestClient<Channel>,
}

impl InnerGrpcClient {
    pub fn new(addr:String) -> anyhow::Result<Self> {
        let channel=Channel::from_shared(addr)?.connect_lazy();
        let bi_request_stream_client = BiRequestStreamClient::new(channel.clone());
        let request_client = RequestClient::new(channel);
        Ok(Self{
            bi_request_stream_client,
            request_client,
        })
    }
}