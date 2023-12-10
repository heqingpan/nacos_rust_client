use tonic::transport::Channel;

use crate::{
    client::{config_client::ConfigKey, get_md5, now_millis},
    conn_manage::conn_msg::ConfigResponse,
    grpc::constant::LABEL_MODULE_CONFIG,
};

use super::{
    api_model::{
        BaseResponse, ConfigBatchListenRequest, ConfigChangeBatchListenResponse,
        ConfigListenContext, ConfigPublishRequest, ConfigQueryRequest, ConfigQueryResponse,
        ConfigRemoveRequest,
    },
    nacos_proto::request_client::RequestClient,
    utils::PayloadUtils,
};

pub(crate) struct GrpcConfigRequestUtils;

impl GrpcConfigRequestUtils {
    pub async fn check_register(channel: Channel) -> anyhow::Result<bool> {
        let check_id = format!("__check_register_{}", now_millis());
        let config_key = ConfigKey::new(&check_id, "__check", "");
        let request = ConfigQueryRequest {
            data_id: config_key.data_id,
            group: config_key.group,
            tenant: config_key.tenant,
            module: Some(LABEL_MODULE_CONFIG.to_owned()),
            request_id: Some(check_id),
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("ConfigQueryRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("check_register,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let response: BaseResponse = serde_json::from_slice(&body_vec)?;
        if response.error_code == 301u16 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub async fn config_query(
        channel: Channel,
        request_id: Option<String>,
        config_key: ConfigKey,
    ) -> anyhow::Result<ConfigResponse> {
        let request = ConfigQueryRequest {
            data_id: config_key.data_id,
            group: config_key.group,
            tenant: config_key.tenant,
            module: Some(LABEL_MODULE_CONFIG.to_owned()),
            request_id,
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("ConfigQueryRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("config_query,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let response: ConfigQueryResponse = serde_json::from_slice(&body_vec)?;
        if response.result_code != 200u16 {
            log::warn!(
                "config_query response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        let md5 = response.md5.unwrap_or_else(|| get_md5(&response.content));
        Ok(ConfigResponse::ConfigValue(response.content, md5))
    }

    pub async fn config_publish(
        channel: Channel,
        request_id: Option<String>,
        config_key: ConfigKey,
        content: String,
    ) -> anyhow::Result<ConfigResponse> {
        let request = ConfigPublishRequest {
            data_id: config_key.data_id,
            group: config_key.group,
            tenant: config_key.tenant,
            content,
            request_id,
            module: Some(LABEL_MODULE_CONFIG.to_owned()),
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("ConfigPublishRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("config_publish,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let response: BaseResponse = serde_json::from_slice(&body_vec)?;
        if response.result_code != 200u16 {
            log::warn!(
                "config_publish response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        Ok(ConfigResponse::None)
    }

    pub async fn config_remove(
        channel: Channel,
        request_id: Option<String>,
        config_key: ConfigKey,
    ) -> anyhow::Result<ConfigResponse> {
        let request = ConfigRemoveRequest {
            data_id: config_key.data_id,
            group: config_key.group,
            tenant: config_key.tenant,
            module: Some(LABEL_MODULE_CONFIG.to_owned()),
            request_id,
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("ConfigRemoveRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("config_remove,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let response: BaseResponse = serde_json::from_slice(&body_vec)?;
        if response.result_code != 200u16 {
            log::warn!(
                "config_remove response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        Ok(ConfigResponse::None)
    }

    pub async fn config_change_batch_listen(
        channel: Channel,
        request_id: Option<String>,
        listen_items: Vec<(ConfigKey, String)>,
        listen: bool,
    ) -> anyhow::Result<ConfigResponse> {
        let config_listen_contexts: Vec<ConfigListenContext> = listen_items
            .into_iter()
            .map(|(config_key, md5)| ConfigListenContext {
                data_id: config_key.data_id,
                group: config_key.group,
                tenant: config_key.tenant,
                md5,
                ..Default::default()
            })
            .collect::<_>();
        let request = ConfigBatchListenRequest {
            config_listen_contexts,
            listen,
            module: Some(LABEL_MODULE_CONFIG.to_owned()),
            request_id,
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("ConfigBatchListenRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("config_change_batch_listen,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let response: ConfigChangeBatchListenResponse = serde_json::from_slice(&body_vec)?;
        if response.result_code != 200u16 {
            log::warn!(
                "config_change_batch_listen response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        let keys: Vec<ConfigKey> = response
            .changed_configs
            .into_iter()
            .map(|e| ConfigKey {
                tenant: e.tenant,
                data_id: e.data_id,
                group: e.group,
            })
            .collect::<_>();
        Ok(ConfigResponse::ChangeKeys(keys))
    }
}
