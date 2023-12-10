use std::time::Duration;

use tokio::time::timeout;
use tonic::transport::Channel;

use self::nacos_proto::request_client::RequestClient;

pub mod api_model;
pub mod channel;
pub mod config_request_utils;
pub mod constant;
pub mod grpc_client;
pub mod nacos_proto;
pub mod naming_request_utils;
pub mod utils;

const REQUEST_TIMEOUT: Duration = Duration::from_millis(3000);

pub async fn do_timeout_request(
    channel: Channel,
    payload: nacos_proto::Payload,
) -> anyhow::Result<nacos_proto::Payload> {
    let mut request_client = RequestClient::new(channel);
    //let response = timeout(Duration::from_millis(3000), request_client.request(tonic::Request::new(payload))).await??;
    let response = timeout(
        REQUEST_TIMEOUT,
        request_client.request(tonic::Request::new(payload)),
    )
    .await??;
    let payload = response.into_inner();
    Ok(payload)
}
