use actix::prelude::*;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

use super::InnerNamingListener;

const MAX_DATAGRAM_SIZE: usize = 65_507;
pub struct UdpWorker {
    local_addr_str: Option<String>,
    socket: Option<Arc<UdpSocket>>,
    addr: Option<Addr<InnerNamingListener>>,
    udp_port: u16,
    buf: Option<Vec<u8>>,
}

impl UdpWorker {
    pub fn new(addr: Option<Addr<InnerNamingListener>>) -> Self {
        Self {
            local_addr_str: None,
            socket: None,
            addr,
            udp_port: 0,
            buf: Some(vec![]),
        }
    }

    pub fn new_with_socket(socket: UdpSocket, addr: Option<Addr<InnerNamingListener>>) -> Self {
        let local_addr = socket.local_addr().unwrap();
        let udp_port = local_addr.port();
        Self {
            local_addr_str: None,
            socket: Some(Arc::new(socket)),
            addr,
            udp_port,
            buf: Some(vec![]),
        }
    }

    fn init(&mut self, ctx: &mut actix::Context<Self>) {
        self.init_socket(ctx);
        //self.init_loop_recv(ctx);
    }

    fn init_socket(&mut self, ctx: &mut actix::Context<Self>) {
        if self.socket.is_some() {
            self.init_loop_recv(ctx);
            return;
        }
        let local_addr_str = if let Some(addr) = self.local_addr_str.as_ref() {
            addr.to_owned()
        } else {
            "0.0.0.0:0".to_owned()
        };
        async move { UdpSocket::bind(&local_addr_str).await.unwrap() }
            .into_actor(self)
            .map(|r, act, ctx| {
                act.udp_port = r.local_addr().unwrap().port();
                if let Some(_addr) = &act.addr {
                    _addr.do_send(InitLocalAddr { port: act.udp_port });
                }
                act.socket = Some(Arc::new(r));
                act.init_loop_recv(ctx);
            })
            .wait(ctx);
    }

    fn init_loop_recv(&mut self, ctx: &mut actix::Context<Self>) {
        let socket = self.socket.as_ref().unwrap().clone();
        let notify_addr = self.addr.clone();
        let buf = self.buf.replace(Vec::new());
        async move {
            let mut buf = buf.unwrap_or_default();
            if buf.len() < MAX_DATAGRAM_SIZE {
                buf = vec![0u8; MAX_DATAGRAM_SIZE];
            }
            //let mut buf = buf.unwrap_or_else(|| vec![0u8; MAX_DATAGRAM_SIZE]);
            //buf=vec![0u8;MAX_DATAGRAM_SIZE];
            if let Ok((len, addr)) = socket.recv_from(&mut buf).await {
                //let mut data:Vec<u8> = Vec::with_capacity(len);
                let mut data: Vec<u8> = vec![0u8; len];
                data.clone_from_slice(&buf[..len]);
                let msg = UdpDataCmd {
                    data,
                    target_addr: addr,
                };
                //let s=String::from_utf8_lossy(&buf[..len]);
                //println!("rece from:{} | len:{} | str:{}",&addr,len,s);
                if let Some(_notify_addr) = notify_addr {
                    _notify_addr.do_send(msg);
                }
            }
            buf
        }
        .into_actor(self)
        .map(|buf, act, ctx| {
            act.buf.replace(buf);
            ctx.run_later(Duration::new(1, 0), |act, ctx| {
                act.init_loop_recv(ctx);
            });
        })
        .spawn(ctx);
    }
}

impl Actor for UdpWorker {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!(" UdpWorker started");
        self.init(ctx);
    }
}

#[derive(Debug, Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub struct UdpDataCmd {
    pub data: Vec<u8>,
    pub target_addr: SocketAddr,
}

#[derive(Debug, Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub struct InitLocalAddr {
    pub port: u16,
}

impl UdpDataCmd {
    pub fn new(data: Vec<u8>, addr: SocketAddr) -> Self {
        Self {
            data,
            target_addr: addr,
        }
    }
}

impl Handler<UdpDataCmd> for UdpWorker {
    type Result = Result<(), std::io::Error>;
    fn handle(&mut self, msg: UdpDataCmd, ctx: &mut Context<Self>) -> Self::Result {
        let socket = self.socket.as_ref().unwrap().clone();
        async move {
            socket
                .send_to(&msg.data, msg.target_addr)
                .await
                .unwrap_or_default();
        }
        .into_actor(self)
        .map(|_, _, _| {})
        .spawn(ctx);
        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "Result<UdpWorkerResult,std::io::Error>")]
pub enum UdpWorkerCmd {
    QueryUdpPort,
    SetListenerAddr(Addr<InnerNamingListener>),
    Close,
}

pub enum UdpWorkerResult {
    None,
    UdpPort(u16),
}

impl Handler<UdpWorkerCmd> for UdpWorker {
    type Result = Result<UdpWorkerResult, std::io::Error>;

    fn handle(&mut self, msg: UdpWorkerCmd, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            UdpWorkerCmd::QueryUdpPort => {
                if let Some(_addr) = &self.addr {
                    _addr.do_send(InitLocalAddr {
                        port: self.udp_port,
                    });
                }
                return Ok(UdpWorkerResult::UdpPort(self.udp_port));
            }
            UdpWorkerCmd::Close => {
                log::info!("UdpWorker close");
                self.addr = None;
                self.socket = None;
                ctx.stop();
            }
            UdpWorkerCmd::SetListenerAddr(addr) => {
                self.addr = Some(addr);
            }
        };
        Ok(UdpWorkerResult::None)
    }
}
