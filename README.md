
# nacos_rust_client

## 介绍
rust实现的nacos客户端。

1. 支持配置中心的推送、获取、监听
2. 支持注册中心的服务实例注册(自动维护心跳)、服务实例获取(自动监听缓存实例列表)


## 实现方式

使用 actix + tokio 实现


## 使用方式

```toml
[dependencies]
nacos_rust_client = "0.1"
```


## 例子

运行下面的例子，需要先启动nacos服务，把例子中的host改成实际的地址。

例子完整依赖与代码可以参考 examples/下的代码。

### 配置中心例子

```rust
use nacos_rust_client::client::{
    HostInfo
};
use nacos_rust_client::client::config_client::{
    ConfigClient,ConfigKey,ConfigListener,ConfigDefaultListener
};

#[tokio::main]
async fn main() {
    let host = HostInfo::parse("127.0.0.1:8848");
    let mut config_client = ConfigClient::new(host,String::new());
    let key = ConfigKey::new("001","foo","");
    //设置
    config_client.set_config(&key, "1234").await.unwrap();
    //获取
    let v=config_client.get_config(&key).await.unwrap();
    println!("{:?},{}",&key,v);

    let key = ConfigKey::new("003","foo","");
    let c = Box::new(ConfigDefaultListener::new(key.clone(),Arc::new(|s|{
        //字符串反序列化为对象，如:serde_json::from_str::<T>(s)
        Some(s.to_owned())
    })));
    config_client.set_config(&key,"1234").await.unwrap();
    //监听
    config_client.subscribe(c.clone()).await;
    //从监听对象中获取
    println!("003 value:{:?}",c.get_value());
    tokio::signal::ctrl_c().await.expect("failed to listen for event");

}
```


### 服务中心例子

```rust
use std::sync::Arc;
use std::io::{Read, stdin};

use std::time::Duration;

use nacos_rust_client::client::{HostInfo, naming_client::{NamingClient, Instance,QueryInstanceListParams}};



#[tokio::main]
async fn main(){
    std::env::set_var("RUST_LOG","INFO");
    env_logger::init();
    let host = HostInfo::parse("127.0.0.1:8848");
    let client = NamingClient::new(host,"".to_owned());

    let ip = local_ipaddress::get().unwrap();
    for i in 0..10{
        let port=10000+i;
        let instance = Instance::new(&ip,port,"foo","","","",None);
        //注册
        client.register(instance);
    }

    //tokio::spawn(async{query_params2().await.unwrap();});
    let client2 = client.clone();
    tokio::spawn(
        async move {
            query_params(client2.clone()).await;
        }
    );

    //let mut buf = vec![0u8;1];
    //stdin().read(&mut buf).unwrap();
    tokio::signal::ctrl_c().await.expect("failed to listen for event");
    println!("n:{}",&client.namespace_id);
}

async fn query_params(client:Arc<NamingClient>) -> anyhow::Result<()>{
    let params = QueryInstanceListParams::new("","","foo",None,true);
    // 模拟每秒钟获取一次实例
    loop{
        //查询并按权重随机选择其中一个实例
        match client.select_instance(params.clone()).await{
            Ok(instances) =>{
                println!("select instance {}:{}",&instances.ip,&instances.port);
            },
            Err(e) => {
                println!("select_instance error {:?}",&e)
            },
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Ok(())
}

```


