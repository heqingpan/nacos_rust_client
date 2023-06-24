

use crate::client::naming_client::REGISTER_PERIOD;
use crate::conn_manage::conn_msg::NamingRequest;
use crate::conn_manage::manage::ConnManage;
use actix::WeakAddr;
use actix::prelude::*;
use crate::client::now_millis;
use std::time::Duration;
use std::collections::HashMap;
use crate::client::naming_client::InnerNamingRequestClient;
use crate::client::naming_client::TimeoutSet;
use crate::client::naming_client::Instance;


//#[derive()]
pub struct InnerNamingRegister{
    instances:HashMap<String,Instance>,
    timeout_set:TimeoutSet<String>,
    conn_manage:Option<WeakAddr<ConnManage>>,
    period: u64,
    stop_remove_all:bool,
    use_grpc:bool,
}

impl InnerNamingRegister {

    pub fn new(use_grpc:bool,conn_manage:Option<WeakAddr<ConnManage>>) -> Self{
        Self{
            instances:Default::default(),
            timeout_set:Default::default(),
            period: REGISTER_PERIOD,
            stop_remove_all:false,
            conn_manage,
            use_grpc,
        }
    }

    pub fn hb(&self,ctx:&mut actix::Context<Self>) {
        if self.use_grpc {
            return;
        }
        ctx.run_later(Duration::new(1,0), |act,ctx|{
            let current_time = now_millis();
            let addr = ctx.address();
            for key in act.timeout_set.timeout(current_time){
                addr.do_send(NamingRegisterCmd::Heartbeat(key,current_time));
            }
            act.hb(ctx);
        });
    }

    fn remove_instance(&self,instance:Instance,ctx:&mut actix::Context<Self>){
        if let Some(conn_manage) = &self.conn_manage {
            if let Some(addr) = conn_manage.upgrade() {
                addr.do_send(NamingRequest::Unregister(instance.clone()));
            }
        }
    }

    fn remove_all_instance(&mut self,ctx:&mut actix::Context<Self>) {
        let instances = self.instances.clone();
        for (_,instance) in instances {
            self.remove_instance(instance,ctx);
        }
        self.instances = HashMap::new();
    }

    fn register_instance(&self,instance:Instance) {
        if let Some(conn_manage) = &self.conn_manage {
            if let Some(addr) = conn_manage.upgrade() {
                addr.do_send(NamingRequest::Register(instance));
            }
        }
    }

    fn heartbeat_instance(&self,instance:&Instance) {
        if let Some(conn_manage) = &self.conn_manage {
            if let Some(addr) = conn_manage.upgrade() {
                addr.do_send(NamingRequest::V1Heartbeat(instance.beat_string.clone().unwrap_or_default()));
            }
        }
    }
}

impl Actor for InnerNamingRegister {
    type Context = Context<Self>;

    fn started(&mut self,ctx: &mut Self::Context) {
        log::info!(" InnerNamingRegister started");
        self.hb(ctx);
    }

    fn stopping(&mut self,ctx: &mut Self::Context) -> Running {
        log::info!(" InnerNamingRegister stopping ");
        if self.stop_remove_all {
            return Running::Stop;
        }
        self.remove_all_instance(ctx);
        Running::Continue
    }
}


#[derive(Debug,Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub enum NamingRegisterCmd {
    Register(Instance),
    Remove(Instance),
    Heartbeat(String,u64),
    Close(),
}

impl Handler<NamingRegisterCmd> for InnerNamingRegister {
    type Result = Result<(),std::io::Error>;

    fn handle(&mut self,msg:NamingRegisterCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg{
            NamingRegisterCmd::Register(mut instance) => {
                instance.init_beat_string();
                let key = instance.generate_key();
                if self.instances.contains_key(&key) {
                    return Ok(());
                }
                // request register
                self.register_instance(instance.clone());
                self.instances.insert(key.clone(), instance);
                if !self.use_grpc {
                    let time = now_millis();
                    self.timeout_set.add(time+self.period,key);
                }
            },
            NamingRegisterCmd::Remove(instance) => {
                let key = instance.generate_key();
                if let Some(instance)=self.instances.remove(&key) {
                    // request unregister
                    self.remove_instance(instance, ctx);

                }
            },
            NamingRegisterCmd::Heartbeat(key,time) => {
                if self.use_grpc {
                    //不需要单独维持心跳
                    return Ok(());
                }
                if let Some(instance)=self.instances.get(&key) {
                    self.heartbeat_instance(&instance);
                    self.timeout_set.add(time+self.period, key);
                }
            },
            NamingRegisterCmd::Close() => {
                ctx.stop();
            }
        }
        Ok(())
    }
}