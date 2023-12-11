
# nacos-tonic-discover

## 介绍
nacos_rust_client 对 tonic 服务地址选择器的适配

## 使用方式

```toml
[dependencies]
nacos-tonic-discover = "0.1"
nacos_rust_client = "0.3"
tonic = "0.7"
```

### 地址选择器工厂

地址选择器工厂,背后会缓存channel,监听服务地址变更然后更新到channel。

1. 创建地址选择器工厂

```rust
let discover_factory = TonicDiscoverFactory::new(naming_client.clone());
```

创建地址选择器工厂,后会把工厂的一个引用加入到全局对象,可以通过 `get_last_factory`获取.

```rust
let discover_factory = nacos_tonic_discover::get_last_factory().unwrap();```

2. 构建指定服务对应的 `tonic::transport::Channel`

```rust
let service_key = ServiceInstanceKey::new("helloworld","AppName");
let channel = discover_factory.build_service_channel(service_key.clone()).await?;
```

地址选择器工厂,背后会维护 `Channel`的最新有效地址;对同一个service_key实际只创建一个`Channel`,第一次构建，后续直接返回缓存拷贝。

3. 使用Channel创建service client,并使用

```rust
let mut client = GreeterClient::new(channel);
let request = tonic::Request::new(HelloRequest {
    name: format!("Tonic"),
});
let response = client.say_hello(request).await?;
```

## 例子

运行下面的例子，需要先启动nacos服务，把例子中的host改成实际的地址。

例子完整依赖与代码可以参考 examples/下的代码。




### tonic service示例

```sh
# 运行方式
cargo run --example tonic_discover_service
```


```rust
// file: examples/src/tonic-discover/service.rs

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
        let addr = format!("{}:{}","0.0.0.0",port).parse()?;
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
```

### tonic client示例

```sh
# 运行方式
cargo run --example tonic_discover_client
```

```rust
// file: examples/src/tonic-discover/client.rs

use std::time::Duration;
use nacos_rust_client::client::naming_client::{ServiceInstanceKey, QueryInstanceListParams};
use nacos_tonic_discover::TonicDiscoverFactory;
use nacos_rust_client::client::naming_client::NamingClient;
use hello_world::greeter_client::GreeterClient;
use hello_world::HelloRequest;

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let namespace_id = "public".to_owned(); //default teant
    let auth_info = None; // Some(AuthInfo::new("nacos","nacos"))
    let naming_client = NamingClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848", namespace_id, auth_info);


    let service_key = ServiceInstanceKey::new("helloworld","AppName");
    //build client by discover factory
    let discover_factory = TonicDiscoverFactory::new(naming_client.clone());
    let channel = discover_factory.build_service_channel(service_key.clone()).await?;
    let mut client = GreeterClient::new(channel);

    for i in 0..5 {
        let request = tonic::Request::new(HelloRequest {
            name: format!("Tonic {} [client by discover factory]",i),
        });
        let response = client.say_hello(request).await?;
        println!("RESPONSE={:?}", response);
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    //build client by naming client select
    let param = QueryInstanceListParams::new_by_serivce_key(&service_key);

    for i in 5..10 {
        let instance=naming_client.select_instance(param.clone()).await?;
        let mut client = GreeterClient::connect(format!("http://{}:{}",&instance.ip,&instance.port)).await?;
        let request = tonic::Request::new(HelloRequest {
            name: format!("Tonic {} [client by naming client select]",i),
        });
        let response = client.say_hello(request).await?;
        println!("RESPONSE={:?}", response);
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Ok(())
}

```