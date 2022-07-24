use std::io::stdin;
use std::error::Error;
use std::env;
use std::net::SocketAddr;
use std::sync::{Arc};
use std::borrow::Cow;
use actix::prelude::*;
use tokio::net::{UdpSocket};
use tokio::signal;
use tokio::sync::Mutex;

use super::InnerNamingListener;


const MAX_DATAGRAM_SIZE: usize = 65_507;
pub struct UdpWorker{
    local_addr:Option<String>,
    socket:Option<Arc<UdpSocket>>,
    addr:Addr<InnerNamingListener>,
}

impl UdpWorker {
    /*
    pub fn new(addr:Option<String>) -> Self{
        Self{
            local_addr:addr,
            socket:None,
        }
    } 
    */

    pub fn new_with_socket(socket:UdpSocket,addr:Addr<InnerNamingListener>) -> Self{
        Self{
            local_addr:None,
            socket:Some(Arc::new(socket)),
            addr,
        }
    }

    fn init(&self,ctx:&mut actix::Context<Self>){
        self.init_socket(ctx);
        //self.init_loop_recv(ctx);
    }

    fn init_socket(&self,ctx:&mut actix::Context<Self>){
        if self.socket.is_some(){
            self.init_loop_recv(ctx);
            return;
        }
        let local_addr =if let Some(addr)= self.local_addr.as_ref() {
            addr.to_owned()
        }else {"0.0.0.0:0".to_owned()};
        async move {
            UdpSocket::bind(&local_addr).await.unwrap()
        }
        .into_actor(self).map(|r,act,ctx|{
            act.socket = Some(Arc::new(r));
            act.init_loop_recv(ctx);
        }).wait(ctx);
    }

    fn init_loop_recv(&self,ctx:&mut actix::Context<Self>) {
        let socket = self.socket.as_ref().unwrap().clone();
        let notify_addr = self.addr.clone();
        async move {
                    let mut buf=vec![0u8;MAX_DATAGRAM_SIZE];
                    loop{
                        match socket.recv_from(&mut buf).await{
                            Ok((len,addr)) => {
                                //let mut data:Vec<u8> = Vec::with_capacity(len);
                                let mut data:Vec<u8> = vec![0u8;len];
                                data.clone_from_slice(&buf[..len]);
                                let msg = UdpDataCmd{data:data,target_addr:addr};
                                //let s=String::from_utf8_lossy(&buf[..len]);
                                //println!("rece from:{} | len:{} | str:{}",&addr,len,s);
                                notify_addr.do_send(msg);

                            },
                            _ => {}
                        }
                    }
        }
        .into_actor(self).map(|_,_,_|{}).spawn(ctx);
    }
}

impl Actor for UdpWorker {
    type Context = Context<Self>;

    fn started(&mut self,ctx: &mut Self::Context) {
        log::info!(" UdpWorker started");
        self.init(ctx);
    }
}

#[derive(Debug,Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub struct UdpDataCmd{
    pub data:Vec<u8>,
    pub target_addr:SocketAddr,
}

impl UdpDataCmd{
    fn new(data:Vec<u8>,addr:SocketAddr) -> Self {
        Self{
            data,
            target_addr:addr,
        }
    }
}

impl Handler<UdpDataCmd> for UdpWorker {
    type Result = Result<(),std::io::Error>;
    fn handle(&mut self,msg:UdpDataCmd,ctx: &mut Context<Self>) -> Self::Result {
        let socket = self.socket.as_ref().unwrap().clone();
        async move{
            socket.send_to(&msg.data, msg.target_addr).await;
        }
        .into_actor(self).map(|_,_,_|{}).spawn(ctx);
        Ok(())
    }
}