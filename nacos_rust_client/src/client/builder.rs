use std::{collections::HashMap, sync::Arc};

use crate::{conn_manage::manage::ConnManage, init_global_system_actor};

use super::{
    config_client::inner_client::ConfigInnerRequestClient, nacos_client::ActixSystemActorSetCmd,
    naming_client::InnerNamingRequestClient, AuthInfo, ClientInfo, ConfigClient, HostInfo,
    NamingClient, ServerEndpointInfo,
};

#[derive(Clone, Debug)]
pub struct ClientBuilder {
    endpoint: ServerEndpointInfo,
    tenant: String,
    auth_info: Option<AuthInfo>,
    use_grpc: bool,
    client_info: ClientInfo,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientBuilder {
    pub fn new() -> Self {
        let endpoint = ServerEndpointInfo::new("");
        Self {
            endpoint,
            tenant: "public".to_owned(),
            auth_info: None,
            use_grpc: true,
            client_info: Default::default(),
        }
    }

    pub fn set_tenant(mut self, tenant: String) -> Self {
        self.tenant = tenant;
        self
    }

    pub fn set_endpoint_addrs(mut self, addrs: &str) -> Self {
        self.endpoint = ServerEndpointInfo::new(addrs);
        self
    }

    pub fn set_endpoint(mut self, endpoint: ServerEndpointInfo) -> Self {
        self.endpoint = endpoint;
        self
    }

    pub fn set_hosts(mut self, hosts: Vec<HostInfo>) -> Self {
        self.endpoint = ServerEndpointInfo { hosts };
        self
    }

    pub fn set_auth_info(mut self, auth_info: Option<AuthInfo>) -> Self {
        self.auth_info = auth_info;
        self
    }

    pub fn set_use_grpc(mut self, use_grpc: bool) -> Self {
        self.use_grpc = use_grpc;
        self
    }

    pub fn set_client_info(mut self, client_info: ClientInfo) -> Self {
        self.client_info = client_info;
        self
    }

    pub fn set_client_ip(mut self, client_ip: String) -> Self {
        self.client_info.client_ip = client_ip;
        self
    }

    pub fn set_app_name(mut self, app_name: String) -> Self {
        self.client_info.app_name = app_name;
        self
    }

    pub fn set_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.client_info.headers = headers;
        self
    }

    pub fn build_config_client(self) -> Arc<ConfigClient> {
        let (config_client, _) = self.build();
        config_client
    }

    pub fn build_naming_client(self) -> Arc<NamingClient> {
        let (_, naming_client) = self.build();
        naming_client
    }

    pub fn build(self) -> (Arc<ConfigClient>, Arc<NamingClient>) {
        let use_grpc = self.use_grpc;
        let auth_info = self.auth_info;
        let endpoint = Arc::new(self.endpoint);
        let namespace_id = self.tenant.clone();
        let tenant = self.tenant;
        let current_ip = self.client_info.client_ip.clone();

        let conn_manage = ConnManage::new(
            endpoint.hosts.clone(),
            use_grpc,
            auth_info.clone(),
            Default::default(),
            Arc::new(self.client_info),
        );
        let conn_manage_addr = conn_manage.start_at_global_system();
        let request_client = InnerNamingRequestClient::new_with_endpoint(endpoint.clone());
        let addrs = NamingClient::init_register(
            namespace_id.clone(),
            current_ip.clone(),
            request_client,
            auth_info.clone(),
            Some(conn_manage_addr.clone().downgrade()),
            use_grpc,
        );
        let naming_client = Arc::new(NamingClient {
            namespace_id,
            register: addrs.0,
            listener_addr: addrs.1,
            current_ip,
            _conn_manage_addr: conn_manage_addr.clone(),
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastNamingClient(
            naming_client.clone(),
        ));

        let mut request_client = ConfigInnerRequestClient::new_with_endpoint(endpoint);
        let (config_inner_addr, auth_addr) = ConfigClient::init_register(
            request_client.clone(),
            auth_info,
            Some(conn_manage_addr.clone().downgrade()),
            use_grpc,
        );
        request_client.set_auth_addr(auth_addr);
        let config_client = Arc::new(ConfigClient {
            tenant,
            request_client,
            config_inner_addr,
            conn_manage_addr,
        });
        //let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastConfigClient(
            config_client.clone(),
        ));
        (config_client, naming_client)
    }
}
