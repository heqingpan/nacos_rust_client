

use std::collections::HashMap;

use actix::prelude::*;

use super::inner_conn::InnerConn;

#[derive(Default,Debug)]
pub(crate) struct ConnManage {
    conns: Vec<InnerConn>,
    conn_map: HashMap<u32,u32>,
    current_index:usize,
}