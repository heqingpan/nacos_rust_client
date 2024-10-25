#![allow(unused_variables, dead_code)]
use std::sync::Arc;

use actix::{Addr, WeakAddr};

use super::{
    config_key::ConfigKey,
    inner::{ConfigInnerActor, ConfigInnerCmd},
    inner_client::ConfigInnerRequestClient,
    listener::ConfigListener,
};
use crate::client::naming_client::InnerNamingRequestClient;
use crate::{
    client::{
        auth::AuthActor,
        get_md5,
        nacos_client::{ActixSystemActorSetCmd, ActixSystemCmd, ActixSystemResult},
        AuthInfo, HostInfo, ServerEndpointInfo,
    },
    conn_manage::{
        conn_msg::{ConfigRequest, ConfigResponse},
        manage::ConnManage,
    },
    init_global_system_actor,
};

pub struct ConfigClient {
    pub(crate) tenant: String,
    pub(crate) request_client: ConfigInnerRequestClient,
    pub(crate) config_inner_addr: Addr<ConfigInnerActor>,
    pub(crate) conn_manage_addr: Addr<ConnManage>,
}

impl Drop for ConfigClient {
    fn drop(&mut self) {
        self.config_inner_addr.do_send(ConfigInnerCmd::Close);
        //std::thread::sleep(utils::ms(500));
    }
}

impl ConfigClient {
    pub fn new(host: HostInfo, tenant: String) -> Arc<Self> {
        let use_grpc = false;
        let endpoint = Arc::new(ServerEndpointInfo {
            hosts: vec![host.clone()],
        });
        let auth_actor = AuthActor::init_auth_actor(endpoint.clone(), None);
        let conn_manage = ConnManage::new(
            vec![host.clone()],
            use_grpc,
            None,
            Default::default(),
            Default::default(),
            auth_actor.clone(),
        );
        let conn_manage_addr = conn_manage.start_at_global_system();
        let request_client =
            ConfigInnerRequestClient::new_with_endpoint(endpoint, Some(auth_actor.clone()));
        let config_inner_addr = Self::init_register(
            request_client.clone(),
            Some(conn_manage_addr.clone().downgrade()),
            use_grpc,
        );
        //request_client.set_auth_addr(auth_addr);
        let r = Arc::new(Self {
            tenant,
            request_client,
            config_inner_addr,
            conn_manage_addr,
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastConfigClient(r.clone()));
        r
    }

    pub fn new_with_addrs(addrs: &str, tenant: String, auth_info: Option<AuthInfo>) -> Arc<Self> {
        let use_grpc = false;
        let endpoint = Arc::new(ServerEndpointInfo::new(addrs));
        let auth_actor = AuthActor::init_auth_actor(endpoint.clone(), auth_info.clone());
        let conn_manage = ConnManage::new(
            endpoint.hosts.clone(),
            use_grpc.to_owned(),
            auth_info.clone(),
            Default::default(),
            Default::default(),
            auth_actor.clone(),
        );
        let conn_manage_addr = conn_manage.start_at_global_system();
        let request_client =
            ConfigInnerRequestClient::new_with_endpoint(endpoint, Some(auth_actor));
        let config_inner_addr = Self::init_register(
            request_client.clone(),
            Some(conn_manage_addr.clone().downgrade()),
            use_grpc,
        );
        let r = Arc::new(Self {
            tenant,
            request_client,
            config_inner_addr,
            conn_manage_addr,
        });
        let system_addr = init_global_system_actor();
        system_addr.do_send(ActixSystemActorSetCmd::LastConfigClient(r.clone()));
        r
    }

    pub(crate) fn init_register(
        request_client: ConfigInnerRequestClient,
        conn_manage_addr: Option<WeakAddr<ConnManage>>,
        use_grpc: bool,
    ) -> Addr<ConfigInnerActor> {
        let system_addr = init_global_system_actor();
        let actor = ConfigInnerActor::new(request_client, use_grpc, conn_manage_addr);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::ConfigInnerActor(actor, tx);
        system_addr.do_send(msg);
        match rx.recv().unwrap() {
            ActixSystemResult::ConfigInnerActor(addr) => addr,
            _ => panic!("init actor error"),
        }
    }

    pub fn gene_config_key(&self, data_id: &str, group: &str) -> ConfigKey {
        ConfigKey {
            data_id: data_id.to_owned(),
            group: group.to_owned(),
            tenant: self.tenant.to_owned(),
        }
    }

    pub async fn get_config(&self, key: &ConfigKey) -> anyhow::Result<String> {
        let cmd = ConfigRequest::GetConfig(key.clone());
        let res: ConfigResponse = self.conn_manage_addr.send(cmd).await??;
        match res {
            ConfigResponse::ConfigValue(content, _md5) => Ok(content),
            _ => Err(anyhow::anyhow!("get config error")),
        }
    }

    pub async fn set_config(&self, key: &ConfigKey, value: &str) -> anyhow::Result<()> {
        let cmd = ConfigRequest::SetConfig(key.clone(), value.to_owned());
        let _res: ConfigResponse = self.conn_manage_addr.send(cmd).await??;
        Ok(())
    }

    pub async fn del_config(&self, key: &ConfigKey) -> anyhow::Result<()> {
        let cmd = ConfigRequest::DeleteConfig(key.clone());
        let _res: ConfigResponse = self.conn_manage_addr.send(cmd).await??;
        Ok(())
    }

    /*
    pub(crate) async fn listene(&self,content:&str,timeout:Option<u64>) -> anyhow::Result<Vec<ConfigKey>> {
        self.request_client.listene(content, timeout).await
    }
    */

    pub async fn subscribe<T: ConfigListener + Send + 'static>(
        &self,
        listener: Box<T>,
    ) -> anyhow::Result<()> {
        let key = listener.get_key();
        self.subscribe_with_key(key, listener).await
    }

    pub async fn subscribe_with_key<T: ConfigListener + Send + 'static>(
        &self,
        key: ConfigKey,
        listener: Box<T>,
    ) -> anyhow::Result<()> {
        let id = 0u64;
        let md5 = match self.get_config(&key).await {
            Ok(text) => {
                listener.change(&key, &text);
                get_md5(&text)
            }
            Err(_) => "".to_owned(),
        };
        let msg = ConfigInnerCmd::SUBSCRIBE(key, id, md5, listener);
        self.config_inner_addr.do_send(msg);
        //let msg=ConfigInnerMsg::SUBSCRIBE(key,id,md5,listener);
        //self.subscribe_sender.send(msg).await;
        Ok(())
    }

    pub async fn unsubscribe(&self, key: ConfigKey) -> anyhow::Result<()> {
        let id = 0u64;
        let msg = ConfigInnerCmd::REMOVE(key, id);
        self.config_inner_addr.do_send(msg);
        Ok(())
    }
}
