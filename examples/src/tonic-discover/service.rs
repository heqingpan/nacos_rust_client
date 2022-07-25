use nacos_rust_client::client::naming_client::Instance;
use nacos_rust_client::client::naming_client::NamingClient;
use tokio::sync::mpsc;
use tonic::{transport::Server, Request, Response, Status};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[derive(Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //let ip="[::]" ; //only use at localhost
    let ip = local_ipaddress::get().unwrap();
    let addrs = [(ip.clone(),10051),(ip.clone(),10052)];
    let namespace_id = "public".to_owned(); //default teant
    let auth_info = None; // Some(AuthInfo::new("nacos","nacos"))
    let client = NamingClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848", namespace_id, auth_info);

    let (tx, mut rx) = mpsc::unbounded_channel();

    for (ip,port) in &addrs {
        let addr = format!("{}:{}","[::]",port).parse()?;
        let tx = tx.clone();
        let greeter = MyGreeter::default();
        let serve = Server::builder()
            .add_service(GreeterServer::new(greeter))
            .serve(addr);

        tokio::spawn(async move {
            if let Err(e) = serve.await {
                eprintln!("Error = {:?}", e);
            }

            tx.send(()).unwrap();
        });
    }

    for (ip,port) in &addrs {
        let instance = Instance::new_simple(&ip,port.to_owned(),"helloworld","AppName");
        client.register(instance);
    }

    rx.recv().await;

    Ok(())
}