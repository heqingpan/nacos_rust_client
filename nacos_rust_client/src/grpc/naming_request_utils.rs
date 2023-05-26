use tonic::transport::Channel;

use crate::{client::naming_client::Instance, conn_manage::conn_msg::NamingResponse};

use super::{api_model::{Instance as ApiInstance, BatchInstanceRequest, BaseResponse}, utils::PayloadUtils, nacos_proto::request_client::RequestClient};


const REGISTER_INSTANCE: &str = "registerInstance";

const DE_REGISTER_INSTANCE: &str = "deregisterInstance";

pub(crate) struct GrpcNamingRequestUtils;

impl GrpcNamingRequestUtils {

    fn convert_to_api_instance(input:Instance) -> ApiInstance {
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

    pub async fn register(channel:Channel,instances:Vec<Instance>) -> anyhow::Result<NamingResponse> {
        if instances.len()==0 {
            return Err(anyhow::anyhow!("register instances is empty"));
        }
        let first_instance =instances.get(0).unwrap();
        let mut request = BatchInstanceRequest {
            namespace:Some(first_instance.namespace_id.to_owned()),
            service_name:Some(first_instance.service_name.to_owned()),
            group_name:Some(first_instance.group_name.to_owned()),
            r#type:Some(REGISTER_INSTANCE.to_owned()),
            ..Default::default()
        };
        let api_instances:Vec<ApiInstance> = instances.into_iter().map(|e|Self::convert_to_api_instance(e)).collect::<Vec<_>>();
        request.instances = Some(api_instances);
        
        let val = serde_json::to_string(&request).unwrap();
        let payload = PayloadUtils::build_payload("BatchInstanceRequest", val);
        let  mut request_client = RequestClient::new(channel);
        let response =request_client.request(tonic::Request::new(payload)).await?;
        let body_vec = response.into_inner().body.unwrap_or_default().value;
        let res:BaseResponse= serde_json::from_slice(&body_vec)?;
        if res.error_code!=200u16 {
            return Err(anyhow::anyhow!("response error code"))
        }
        Ok(NamingResponse::None)
    }
}