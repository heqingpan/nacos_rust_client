use std::{collections::HashMap, time::Duration};

use actix::{prelude::*, WeakAddr};

use crate::{
    client::get_md5,
    conn_manage::{
        conn_msg::{ConfigRequest, ConfigResponse},
        manage::{ConnManage, ConnManageCmd},
    },
};

use super::{
    config_key::ConfigKey,
    inner_client::ConfigInnerRequestClient,
    listener::{ConfigListener, ListenerValue},
    model::NotifyConfigItem,
};

pub struct ConfigInnerActor {
    pub request_client: ConfigInnerRequestClient,
    subscribe_map: HashMap<ConfigKey, ListenerValue>,
    conn_manage: Option<WeakAddr<ConnManage>>,
    use_grpc: bool,
}

//type ConfigInnerHandleResultSender = tokio::sync::oneshot::Sender<ConfigInnerHandleResult>;

#[derive(Message)]
#[rtype(result = "Result<ConfigInnerHandleResult,std::io::Error>")]
pub enum ConfigInnerCmd {
    SUBSCRIBE(
        ConfigKey,
        u64,
        String,
        Box<dyn ConfigListener + Send + 'static>,
    ),
    REMOVE(ConfigKey, u64),
    Notify(Vec<NotifyConfigItem>),
    Close,
    GrpcResubscribe,
}

pub enum ConfigInnerHandleResult {
    None,
    Value(String),
}

impl ConfigInnerActor {
    pub(crate) fn new(
        request_client: ConfigInnerRequestClient,
        use_grpc: bool,
        conn_manage: Option<WeakAddr<ConnManage>>,
    ) -> Self {
        Self {
            request_client,
            subscribe_map: Default::default(),
            conn_manage,
            use_grpc,
        }
    }

    fn do_change_config(&mut self, key: &ConfigKey, content: String) {
        let md5 = get_md5(&content);
        if let Some(v) = self.subscribe_map.get_mut(key) {
            v.md5 = md5;
            v.notify(key, &content);
        }
    }

    async fn send(
        conn_manage: &Addr<ConnManage>,
        request: ConfigRequest,
    ) -> anyhow::Result<ConfigResponse> {
        match conn_manage.send(request).await {
            Ok(res) => res,
            _ => Err(anyhow::anyhow!("send msg to ConnManage failed")),
        }
    }

    fn grpc_resubscribe(&mut self, _ctx: &mut actix::Context<Self>) {
        if !self.use_grpc {
            return;
        }
        if let Some(addr) = &self.conn_manage {
            if let Some(addr) = addr.upgrade() {
                let items = self
                    .subscribe_map
                    .keys()
                    .map(|key| (key.clone(), "".to_owned()))
                    .collect::<Vec<_>>();
                addr.do_send(ConfigRequest::Listen(items, true));
            }
        }
    }

    fn listener(&mut self, ctx: &mut actix::Context<Self>) {
        if self.use_grpc {
            return;
        }
        if let Some(content) = self.get_listener_body() {
            let conn_manage = self.conn_manage.clone();
            async move {
                let mut list = vec![];
                if let Some(addr) = conn_manage {
                    if let Some(addr) = addr.upgrade() {
                        if let Ok(ConfigResponse::ChangeKeys(config_keys)) =
                            Self::send(&addr, ConfigRequest::V1Listen(content.clone())).await
                        {
                            for key in config_keys {
                                if let Ok(ConfigResponse::ConfigValue(value, _)) =
                                    Self::send(&addr, ConfigRequest::GetConfig(key.clone())).await
                                {
                                    list.push((key, value));
                                }
                            }
                        }
                    }
                }
                list
            }
            .into_actor(self)
            .map(|r, this, ctx| {
                for (key, context) in r {
                    this.do_change_config(&key, context)
                }
                if !this.subscribe_map.is_empty() {
                    ctx.run_later(Duration::from_millis(5), |act, ctx| {
                        act.listener(ctx);
                    });
                }
            })
            .spawn(ctx);
        }
    }

    fn get_listener_body(&self) -> Option<String> {
        let items = self.subscribe_map.iter().collect::<Vec<_>>();
        if items.is_empty() {
            return None;
        }
        let mut body = String::new();
        for (k, v) in items {
            body += &format!(
                "{}\x02{}\x02{}\x02{}\x01",
                k.data_id, k.group, v.md5, k.tenant
            );
        }
        Some(body)
    }
}

impl Actor for ConfigInnerActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("ConfigInnerActor started");
        if let Some(addr) = &self.conn_manage {
            if let Some(addr) = addr.upgrade() {
                addr.do_send(ConnManageCmd::ConfigInnerActorAddr(
                    ctx.address().downgrade(),
                ));
            }
        }
        //ctx.run_later(Duration::from_millis(5), |act, ctx| {
        //    act.listener(ctx);
        //});
    }
}

impl Handler<ConfigInnerCmd> for ConfigInnerActor {
    type Result = Result<ConfigInnerHandleResult, std::io::Error>;
    fn handle(&mut self, msg: ConfigInnerCmd, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            ConfigInnerCmd::SUBSCRIBE(key, id, md5, func) => {
                let first = self.subscribe_map.is_empty();
                let list = self.subscribe_map.get_mut(&key);
                match list {
                    Some(v) => {
                        v.push(id, func);
                        if !md5.is_empty() {
                            v.md5 = md5;
                        }
                    }
                    None => {
                        let v = ListenerValue::new(vec![(id, func)], md5.clone());
                        if self.use_grpc {
                            if let Some(addr) = &self.conn_manage {
                                if let Some(addr) = addr.upgrade() {
                                    addr.do_send(ConfigRequest::Listen(
                                        vec![(key.clone(), md5.clone())],
                                        true,
                                    ));
                                }
                            }
                        }
                        self.subscribe_map.insert(key, v);
                    }
                };
                if first {
                    ctx.run_later(Duration::from_millis(5), |act, ctx| {
                        act.listener(ctx);
                    });
                }
                Ok(ConfigInnerHandleResult::None)
            }
            ConfigInnerCmd::REMOVE(key, id) => {
                if let Some(v) = self.subscribe_map.get_mut(&key) {
                    let size = v.remove(id);
                    if size == 0 && self.subscribe_map.remove(&key).is_some() && self.use_grpc {
                        if let Some(Some(addr)) = self.conn_manage.as_ref().map(WeakAddr::upgrade) {
                            addr.do_send(ConfigRequest::Listen(
                                vec![(key.clone(), "".to_owned())],
                                false,
                            ));
                        }
                    }
                };
                Ok(ConfigInnerHandleResult::None)
            }
            ConfigInnerCmd::Close => {
                self.conn_manage = None;
                ctx.stop();
                Ok(ConfigInnerHandleResult::None)
            }
            ConfigInnerCmd::Notify(items) => {
                for item in items {
                    self.do_change_config(&item.key, item.content);
                }
                Ok(ConfigInnerHandleResult::None)
            }
            ConfigInnerCmd::GrpcResubscribe => {
                self.grpc_resubscribe(ctx);
                Ok(ConfigInnerHandleResult::None)
            }
        }
    }
}
