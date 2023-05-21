

use std::{collections::HashMap, sync::Arc};

use actix::prelude::*;

use crate::client::{AuthInfo, HostInfo, naming_client::NamingUtils};

use super::{inner_conn::InnerConn, breaker::BreakerConfig, conn_msg::{ConnCmd, ConnMsgResult}};

#[derive(Default,Debug)]
pub(crate) struct ConnManage {
    conns: Vec<InnerConn>,
    conn_map: HashMap<u32,u32>,
    current_index:usize,
    support_grpc: bool,
    auth_info:Option<Arc<AuthInfo>>,
    conn_globda_id:u32,
    breaker_config:Arc<BreakerConfig>,
}

impl ConnManage {
    pub fn new(hosts:Vec<HostInfo>,support_grpc: bool,auth_info:Option<Arc<AuthInfo>>,breaker_config:Arc<BreakerConfig>) -> Self {
        let mut id = 0;
        let mut conns = Vec::with_capacity(hosts.len());
        let mut conn_map = HashMap::new();
        for host in hosts {
            let conn = InnerConn::new(id,host,support_grpc,breaker_config.clone(),auth_info.clone());
            conn_map.insert(id, id);
            id+=1;
            conns.push(conn);
        }
        let mut s=Self {
            conns,
            conn_map,
            support_grpc,
            auth_info,
            conn_globda_id:id,
            breaker_config,
            ..Default::default()
        };
        s
    }

    pub fn init_grpc_conn(&mut self) {
        self.current_index = self.select_index();
        let conn = self.conns.get_mut(self.current_index).unwrap();
        conn.init_grpc().ok();
    }

    fn select_index(&self) -> usize {
        NamingUtils::select_by_weight_fn(&self.conns, |_| 1)
    }
}

impl Actor for ConnManage {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("ConnManage started")
    }
}

impl Handler<ConnCmd> for ConnManage {
    type Result=anyhow::Result<ConnMsgResult>;

    fn handle(&mut self, msg: ConnCmd, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ConnCmd::ConfigCmd(_) => todo!(),
            ConnCmd::NamingCmd(_) => todo!(),
        }
        Ok(ConnMsgResult::None)
    }
}