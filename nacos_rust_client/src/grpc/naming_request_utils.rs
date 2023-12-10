use std::sync::Arc;

use tonic::transport::Channel;

use crate::{
    client::naming_client::{Instance, ServiceInstanceKey},
    conn_manage::conn_msg::{NamingResponse, ServiceResult},
    grpc::{api_model::InstanceRequest, constant::LABEL_MODULE_NAMING},
};

use super::{
    api_model::{
        BaseResponse, BatchInstanceRequest, Instance as ApiInstance, ServiceQueryRequest,
        ServiceQueryResponse, SubscribeServiceRequest, SubscribeServiceResponse,
    },
    nacos_proto::request_client::RequestClient,
    utils::PayloadUtils,
};

const REGISTER_INSTANCE: &str = "registerInstance";

const DE_REGISTER_INSTANCE: &str = "deregisterInstance";

const BATCH_REGISTER_INSTANCE: &str = "batchRegisterInstance";

pub(crate) struct GrpcNamingRequestUtils;

impl GrpcNamingRequestUtils {
    pub(crate) fn convert_to_api_instance(input: Instance) -> ApiInstance {
        ApiInstance {
            ip: Some(input.ip),
            port: input.port,
            weight: input.weight,
            enabled: input.enabled,
            healthy: input.healthy,
            ephemeral: input.ephemeral,
            cluster_name: Some(input.cluster_name),
            service_name: Some(input.service_name),
            metadata: input.metadata.unwrap_or_default(),
            ..Default::default()
        }
    }

    pub(crate) fn convert_to_instance(
        input: ApiInstance,
        service_key: &ServiceInstanceKey,
    ) -> Instance {
        Instance {
            ip: input.ip.unwrap_or_default(),
            port: input.port,
            weight: input.weight,
            enabled: input.enabled,
            healthy: input.healthy,
            ephemeral: input.ephemeral,
            cluster_name: input.cluster_name.unwrap_or_default(),
            service_name: input.service_name.unwrap_or_default(),
            metadata: Some(input.metadata),
            group_name: service_key.group_name.clone(),
            namespace_id: service_key.namespace_id.clone().unwrap_or_default(),
            ..Default::default()
        }
    }

    pub async fn instance_register(
        channel: Channel,
        instance: Instance,
        is_reqister: bool,
    ) -> anyhow::Result<NamingResponse> {
        let request = InstanceRequest {
            namespace: Some(instance.namespace_id.to_owned()),
            service_name: Some(instance.service_name.to_owned()),
            group_name: Some(instance.group_name.to_owned()),
            r#type: Some(if is_reqister {
                REGISTER_INSTANCE.to_owned()
            } else {
                DE_REGISTER_INSTANCE.to_owned()
            }),
            instance: Some(Self::convert_to_api_instance(instance)),
            module: Some(LABEL_MODULE_NAMING.to_owned()),
            ..Default::default()
        };

        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("InstanceRequest", val);
        //debug
        //log::info!("instance_register request,{}",&PayloadUtils::get_payload_string(&payload));
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("instance_register,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let res: BaseResponse = serde_json::from_slice(&body_vec)?;
        if res.result_code != 200u16 {
            log::warn!(
                "instance_register response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        Ok(NamingResponse::None)
    }

    pub async fn batch_register(
        channel: Channel,
        instances: Vec<Instance>,
    ) -> anyhow::Result<NamingResponse> {
        if instances.len() == 0 {
            return Err(anyhow::anyhow!("register instances is empty"));
        }
        let first_instance = instances.get(0).unwrap();
        let mut request = BatchInstanceRequest {
            namespace: Some(first_instance.namespace_id.to_owned()),
            service_name: Some(first_instance.service_name.to_owned()),
            group_name: Some(first_instance.group_name.to_owned()),
            r#type: Some(BATCH_REGISTER_INSTANCE.to_owned()),
            module: Some(LABEL_MODULE_NAMING.to_owned()),
            ..Default::default()
        };
        let api_instances: Vec<ApiInstance> = instances
            .into_iter()
            .map(|e| Self::convert_to_api_instance(e))
            .collect::<Vec<_>>();
        request.instances = Some(api_instances);

        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("BatchInstanceRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("batch_register,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let res: BaseResponse = serde_json::from_slice(&body_vec)?;
        if res.result_code != 200u16 {
            log::warn!(
                "batch_register response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        Ok(NamingResponse::None)
    }

    pub async fn subscribe(
        channel: Channel,
        service_key: ServiceInstanceKey,
        is_subscribe: bool,
        clusters: Option<String>,
    ) -> anyhow::Result<NamingResponse> {
        let clone_key = service_key.clone();
        let request = SubscribeServiceRequest {
            namespace: service_key.namespace_id,
            group_name: Some(service_key.group_name),
            service_name: Some(service_key.service_name),
            subscribe: is_subscribe,
            clusters,
            module: Some(LABEL_MODULE_NAMING.to_owned()),
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("SubscribeServiceRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //debug
        //log::info!("subscribe,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let res: SubscribeServiceResponse = serde_json::from_slice(&body_vec)?;
        if res.result_code != 200u16 {
            log::warn!("subscribe response error,{}", String::from_utf8(body_vec)?);
            return Err(anyhow::anyhow!("response error code"));
        }
        if let Some(service_info) = res.service_info {
            let hosts = service_info.hosts.unwrap_or_default();
            let hosts = hosts
                .into_iter()
                .map(|e| Arc::new(Self::convert_to_instance(e, &clone_key)))
                .collect::<Vec<Arc<Instance>>>();
            let service_result = ServiceResult {
                cache_millis: Some(service_info.cache_millis as u64),
                hosts,
            };
            Ok(NamingResponse::ServiceResult(service_result))
        } else {
            if is_subscribe {
                log::warn!("subscribe service result is empty");
            }
            Ok(NamingResponse::None)
        }
    }

    pub async fn query_service(
        channel: Channel,
        service_key: ServiceInstanceKey,
        cluster: Option<String>,
        healthy_only: Option<bool>,
    ) -> anyhow::Result<NamingResponse> {
        let clone_key = service_key.clone();
        let request = ServiceQueryRequest {
            namespace: service_key.namespace_id,
            group_name: Some(service_key.group_name),
            service_name: Some(service_key.service_name),
            cluster,
            healthy_only,
            module: Some(LABEL_MODULE_NAMING.to_owned()),
            ..Default::default()
        };
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("ServiceQueryRequest", val);
        let mut request_client = RequestClient::new(channel);
        let response = request_client.request(tonic::Request::new(payload)).await?;
        let payload = response.into_inner();
        //log::info!("query_service,{}",&PayloadUtils::get_payload_string(&payload));
        let body_vec = payload.body.unwrap_or_default().value;
        let res: ServiceQueryResponse = serde_json::from_slice(&body_vec)?;
        if res.result_code != 200u16 {
            log::warn!(
                "query_service response error,{}",
                String::from_utf8(body_vec)?
            );
            return Err(anyhow::anyhow!("response error code"));
        }
        if let Some(service_info) = res.service_info {
            let hosts = service_info.hosts.unwrap_or_default();
            let hosts = hosts
                .into_iter()
                .map(|e| Arc::new(Self::convert_to_instance(e, &clone_key)))
                .collect::<Vec<Arc<Instance>>>();
            let service_result = ServiceResult {
                cache_millis: Some(service_info.cache_millis as u64),
                hosts,
            };
            Ok(NamingResponse::ServiceResult(service_result))
        } else {
            log::warn!("subscribe service result is empty");
            Ok(NamingResponse::None)
        }
    }
}
