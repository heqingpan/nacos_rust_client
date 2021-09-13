
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


const MAX_DATAGRAM_SIZE: usize = 65_507;
pub struct UdpWorker{
    local_addr:Option<String>,
    socket:Option<Arc<UdpSocket>>,
}

impl UdpWorker {
    pub fn new(addr:Option<String>) -> Self{
        Self{
            local_addr:addr,
            socket:None,
        }
    }

    fn init(&self,ctx:&mut actix::Context<Self>){
        self.init_socket(ctx);
        //self.init_loop_recv(ctx);
    }

    fn init_socket(&self,ctx:&mut actix::Context<Self>){
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
        async move {
                    let mut buf=vec![0u8;MAX_DATAGRAM_SIZE];
                    loop{
                        match socket.recv_from(&mut buf).await{
                            Ok((len,addr)) => {
                                /*
                                if let Some(addr) = self.actor{
                                    let mut data:Vec<u8> = Vec::with_capacity(len);
                                    data.clone_from_slice(&self.buf[..len]);
                                    addr.send(data).await;
                                }
                                */
                                let s=String::from_utf8_lossy(&buf[..len]);
                                println!("rece from:{} | len:{} | str:{}",&addr,len,s);
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
        println!(" UdpWorker started");
        self.init(ctx);
    }
}

#[derive(Debug,Message)]
#[rtype(result = "Result<(),std::io::Error>")]
pub struct UdpSenderCmd{
    pub data:Vec<u8>,
    pub target_addr:SocketAddr,
}

impl UdpSenderCmd{
    fn new(data:Vec<u8>,addr:SocketAddr) -> Self {
        Self{
            data,
            target_addr:addr,
        }
    }
}

impl Handler<UdpSenderCmd> for UdpWorker {
    type Result = Result<(),std::io::Error>;
    fn handle(&mut self,msg:UdpSenderCmd,ctx: &mut Context<Self>) -> Self::Result {
        let socket = self.socket.as_ref().unwrap().clone();
        async move{
            socket.send_to(&msg.data, msg.target_addr).await;
        }
        .into_actor(self).map(|_,_,_|{}).spawn(ctx);
        Ok(())
    }
}

fn get_stdin_data() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut s = String::new();
    stdin().read_line(&mut s)?;
    Ok(s.into_bytes())
}

fn send(sender:Addr<UdpWorker>,remote_addr:SocketAddr){
    loop{
        let data = get_stdin_data().unwrap();
        let msg = UdpSenderCmd::new(data,remote_addr.clone());
        sender.do_send(msg);
    }
}

fn init_actor(addr:&str) -> Addr<UdpWorker> {
    let addr = if addr.len()>0 {Some(addr.to_owned())}else{None};
    let (tx,rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let rt = System::new();
        let addrs = rt.block_on(async {
            UdpWorker::new(addr).start()
        });
        tx.send(addrs);
        rt.run();
    });
    let addrs = rx.recv().unwrap();
    addrs
}

//#[tokio::main]
#[tokio::main]
//#[actix_rt::main] 
async fn main() -> Result<(), Box<dyn Error>> {
    println!("notify udp");
    let remote_addr: SocketAddr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".into())
        .parse()?;
    println!("addr:{:?}", &remote_addr);

    // We use port 0 to let the operating system allocate an available port for us.
    let local_addr: String= env::args()
        .nth(2)
        .unwrap_or_else(|| "0.0.0.0:0".into())
        .parse()?;
    println!("local_addr:{:?}", &local_addr);

    //let socket = UdpSocket::bind(local_addr).await?;

    let worker = init_actor(&local_addr);
    std::thread::spawn(move || send(worker,remote_addr));
    let ctrl_c = signal::ctrl_c().await;
    Ok(())
}