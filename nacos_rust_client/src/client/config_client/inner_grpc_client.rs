/* 
//use std::sync::Arc;

use crate::{grpc::nacos_proto::request_client::RequestClient};



pub struct InnerGrpcClient<T> {
    //pub(crate) endpoints: Arc<ServerEndpointInfo>,
    pub(crate) request_client: RequestClient<T>,
}
*/