use self::nacos_proto::request_client::RequestClient;
use crate::client::auth::{get_token_result, AuthActor};
use crate::client::ClientInfo;
use crate::grpc::nacos_proto::Payload;
use crate::grpc::utils::PayloadUtils;
use actix::Addr;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tonic::transport::Channel;

pub mod api_model;
pub mod channel;
pub mod config_request_utils;
pub mod constant;
pub mod grpc_client;
pub mod nacos_proto;
pub mod naming_request_utils;
pub mod utils;

const REQUEST_TIMEOUT: Duration = Duration::from_millis(3000);

pub const AUTHORIZATION_HEADER: &str = "Authorization";
pub const ACCESS_TOKEN_HEADER: &str = "accessToken";

pub async fn do_timeout_request(
    channel: Channel,
    payload: nacos_proto::Payload,
) -> anyhow::Result<nacos_proto::Payload> {
    do_timeout_request_with_duration(channel, payload, None).await
}

pub async fn do_timeout_request_with_duration(
    channel: Channel,
    payload: Payload,
    duration: Option<Duration>,
) -> anyhow::Result<Payload> {
    let mut request_client = RequestClient::new(channel);
    let duration = if let Some(duration) = duration {
        duration
    } else {
        REQUEST_TIMEOUT
    };
    log::debug!(
        "grpc request:{}",
        &PayloadUtils::get_payload_string(&payload)
    );
    let response = timeout(
        duration,
        request_client.request(tonic::Request::new(payload)),
    )
    .await??;
    let payload = response.into_inner();
    log::debug!(
        "grpc response:{}",
        &PayloadUtils::get_payload_string(&payload)
    );
    Ok(payload)
}

pub(crate) async fn build_request_payload<T: Serialize>(
    request_type: &str,
    request: &T,
    auth_addr: &Addr<AuthActor>,
    client_info: &Arc<ClientInfo>,
) -> anyhow::Result<Payload> {
    let val = serde_json::to_string(request)?;
    let mut payload_header = HashMap::new();
    let token = get_token_result(auth_addr).await?;
    payload_header.insert(ACCESS_TOKEN_HEADER.to_string(), token.as_ref().to_owned());
    let payload =
        PayloadUtils::build_full_payload(request_type, val, &client_info.client_ip, payload_header);
    Ok(payload)
}
