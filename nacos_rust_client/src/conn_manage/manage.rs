

use std::{collections::HashMap, sync::Arc};

use actix::{prelude::*, WeakAddr};

use crate::{client::{AuthInfo, HostInfo, naming_client::NamingUtils, config_client::{inner::ConfigInnerCmd, model::NotifyConfigItem, ConfigInnerActor}, nacos_client::{ActixSystemCmd, ActixSystemResult}}, init_global_system_actor, ActixSystemCreateCmd, ActorCreate};

use super::{inner_conn::InnerConn, breaker::BreakerConfig, conn_msg::{ConnCmd, ConnMsgResult, ConnCallbackMsg}, NotifyCallbackAddr};

#[derive(Default,Clone)]
pub struct ConnManage {
    conns: Vec<InnerConn>,
    conn_map: HashMap<u32,u32>,
    current_index:usize,
    support_grpc: bool,
    auth_info:Option<AuthInfo>,
    conn_globda_id:u32,
    breaker_config:Arc<BreakerConfig>,
    pub(crate) callback: NotifyCallbackAddr,
}

impl ConnManage {
    pub fn new(hosts:Vec<HostInfo>,support_grpc: bool,auth_info:Option<AuthInfo>,breaker_config:BreakerConfig) -> Self {
        let mut id = 0;
        let breaker_config=Arc::new(breaker_config);
        let mut conns = Vec::with_capacity(hosts.len());
        let mut conn_map = HashMap::new();
        for host in hosts {
            let conn = InnerConn::new(id,host,support_grpc,breaker_config.clone(),auth_info.clone());
            conn_map.insert(id, id);
            id+=1;
            conns.push(conn);
        }
        Self {
            conns,
            conn_map,
            support_grpc,
            auth_info,
            conn_globda_id:id,
            breaker_config,
            ..Default::default()
        }
    }

    pub fn init_grpc_conn(&mut self,ctx: &mut Context<Self>) {
        self.current_index = self.select_index();
        let conn = self.conns.get_mut(self.current_index).unwrap();
        let addr = ctx.address().downgrade();
        conn.init_grpc(addr).ok();
    }

    pub fn start_at_global_system(self) -> Addr<Self> {
        let system_addr =  init_global_system_actor();
        let (tx,rx) = std::sync::mpsc::sync_channel(1);
        let msg =ActixSystemCmd::ConnManage(self,tx);
        system_addr.do_send(msg);
        if let ActixSystemResult::ConnManage(addr) = rx.recv().unwrap() {
            addr
        }
        else{
            panic!("create manage actor error")
        }
    }

    fn select_index(&self) -> usize {
        NamingUtils::select_by_weight_fn(&self.conns, |_| 1)
    }
}

impl ActorCreate for ConnManage {
    fn create(&self) -> () {
        ()
    }
}

impl Actor for ConnManage {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("ConnManage started");
        self.init_grpc_conn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "anyhow::Result<()>")]
pub enum ConnManageCmd{
    ConfigInnerActorAddr(WeakAddr<ConfigInnerActor>),
}

impl Handler<ConnManageCmd> for ConnManage {
    type Result=anyhow::Result<()>;

    fn handle(&mut self, msg: ConnManageCmd, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ConnManageCmd::ConfigInnerActorAddr(addr) => {
                self.callback.config_inner_addr = Some(addr);
            },
        }
        Ok(())
    }
}

impl Handler<ConnCallbackMsg> for ConnManage {
    type Result=ResponseActFuture<Self,anyhow::Result<()>>;
    fn handle(&mut self, msg: ConnCallbackMsg, _ctx: &mut Self::Context) -> Self::Result {
        let callbacb = self.callback.clone();
        let fut=async move {
            match msg {
                ConnCallbackMsg::ConfigChange(config_key,content,md5) => {
                    if let Some(config_addr)=callbacb.config_inner_addr {
                        if let Some(config_addr)=config_addr.upgrade() {
                            config_addr.do_send(ConfigInnerCmd::Notify(vec![NotifyConfigItem{
                                key:config_key,
                                content,
                                md5,
                            }]));
                        }
                    }
                },
                ConnCallbackMsg::InstanceChange(_, _) => todo!(),
            }
            //...
            Ok(())
        }.into_actor(self)
        .map(|r,_act,_ctx|{r});
        Box::pin(fut)
    }
}

impl Handler<ConnCmd> for ConnManage {
    type Result=ResponseActFuture<Self,anyhow::Result<ConnMsgResult>>;

    fn handle(&mut self, msg: ConnCmd, ctx: &mut Self::Context) -> Self::Result {
        let conn = self.conns.get(self.current_index).unwrap();
        let conn_addr =conn.grpc_client_addr.clone();
        let fut=async move {
            if let Some(conn_addr) = conn_addr {
                let res:ConnMsgResult= conn_addr.send(msg).await??;
                Ok(res)
            }
            else{
                Err(anyhow::anyhow!("grpc conn is empty"))
            }
        }.into_actor(self)
        .map(|r,_act,_ctx|{r});
        Box::pin(fut)

    }
}