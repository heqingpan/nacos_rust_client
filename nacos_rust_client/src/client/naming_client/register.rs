

use crate::client::naming_client::REGISTER_PERIOD;
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
    request_client:InnerNamingRequestClient,
    period: u64,
    stop_remove_all:bool
}

impl InnerNamingRegister {

    pub fn new(request_client:InnerNamingRequestClient) -> Self{
        Self{
            instances:Default::default(),
            timeout_set:Default::default(),
            request_client,
            period: REGISTER_PERIOD,
            stop_remove_all:false,
        }
    }

    pub fn hb(&self,ctx:&mut actix::Context<Self>) {
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
        let client = self.request_client.clone();
        async move {
            client.remove(&instance).await;
            instance
        }.into_actor(self)
        .map(|_,_,ctx|{}).spawn(ctx);
    }

    fn remove_all_instance(&mut self,ctx:&mut actix::Context<Self>) {
        let client = self.request_client.clone();
        let instances = self.instances.clone();
        for (_,instance) in instances {
            self.remove_instance(instance,ctx);
        }
        self.instances = HashMap::new();
        /*
        async move {
            for (_,instance) in instances.iter() {
                client.remove(&instance).await;
            }
            ()
        }.into_actor(self)
        .map(|_,act,ctx|{
            act.stop_remove_all=true;
            ctx.stop();
        }).spawn(ctx);
        */
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
                let time = now_millis();
                self.timeout_set.add(time+self.period,key.clone());
                let client = self.request_client.clone();
                async move {
                    client.register(&instance).await;
                    instance
                }.into_actor(self)
                .map(|instance,act,_|{
                    act.instances.insert(key, instance);
                })
                .spawn(ctx);
            },
            NamingRegisterCmd::Remove(instance) => {
                let key = instance.generate_key();
                if let Some(instatnce)=self.instances.remove(&key) {
                    // request unregister
                    self.remove_instance(instance, ctx);

                }
            },
            NamingRegisterCmd::Heartbeat(key,time) => {
                if let Some(instance)=self.instances.get(&key) {
                    // request heartbeat
                    let client = self.request_client.clone();
                    if let Some(beat_string) = &instance.beat_string {
                        let beat_string = beat_string.clone();
                        async move {
                            client.heartbeat(beat_string).await;
                        }.into_actor(self)
                        .map(|_,_,_|{}).spawn(ctx);
                    }
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