
# nacos_rust_client

## 介绍
rust实现的nacos客户端。

目前暂时只支持`1.x`版http协议，`2.x`服务兼容`1.x`协议。

1. 使用 actix + tokio 实现。
2. 支持配置中心的推送、获取、监听。
3. 支持注册中心的服务实例注册(自动维护心跳)、服务实例获取(自动监听缓存实例列表)。
4. 创建的客户端后台处理，都放在同一个actix环境线程;高性能，不会有线程膨胀，稳定可控。


## 使用方式

首先加入引用

```toml
[dependencies]
nacos_rust_client = "0.2"
```

### 使用配置中心

1. 创建客户端

使用`ConfigClient::new_with_addrs`创建配置客户端，支持设置集群地址列表，支持验权校验.

```rust
use nacos_rust_client::client::config_client::ConfigClient;
//...
let config_client = ConfigClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848",tenant,auth_info);
```

创建客户端,后会把客户端的一个引用加入到全局对象,可以通过 `get_last_config_client`获取.

```rust
let config_client = nacos_rust_client::get_last_config_client().unwrap();
```

2. 设置获取配置信息

```rust
let key = ConfigKey::new("data_id","group_name","" /*tenant_id*/);
config_client.set_config(&key, "config_value").await.unwrap();
let v=config_client.get_config(&key).await.unwrap();
```

3. 配置监听器

实时的接收服务端的变更推送，更新监听器的内容；用户应用配置动态下发。

```rust
#[derive(Debug,Serialize,Deserialize,Default,Clone)]
pub struct Foo {
    pub name: String,
    pub number: u64,
}
//...
let foo_config_obj_listener = Box::new(ConfigDefaultListener::new(key.clone(),Arc::new(|s|{
    //字符串反序列化为对象，如:serde_json::from_str::<T>(s)
    Some(serde_json::from_str::<Foo>(s).unwrap())
})));
config_client.subscribe(foo_config_obj_listener.clone()).await;
let foo_obj_from_listener = foo_config_obj_listener.get_value().unwrap();
```

### 使用注册中心

1. 创建客户端

使用`NamingClient::new_with_addrs`创建配置客户端，支持设置集群地址列表，支持验权校验.

```rust
use nacos_rust_client::client::naming_client::NamingClient;
//...
let naming_client = NamingClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848",namespace_id,auth_info);
```

创建客户端,后会把客户端的一个引用加入到全局对象,可以通过 `get_last_naming_client`获取.

```rust
let naming_client = nacos_rust_client::get_last_naming_client().unwrap();
```

2. 注册服务实例

只要调拨一次注册实例，客户端会自动在后面维持心跳保活。

```rust
let instance = Instance::new_simple(&ip,port,service_name,group_name);
naming_client.register(instance);
```

3. 服务地址路由

查询指定服务的地址列表

```rust
let params = QueryInstanceListParams::new_simple(service_name,group_name);
let instance_list_result=client.query_instances(params).await;
```

查询指定服务的地址列表并按重选中一个地址做服务调用。

```rust
let params = QueryInstanceListParams::new_simple(service_name,group_name);
let instance_result=client.select_instance(params).await;
```

如果要在tonic中使用服务地址选择,可以使用对tonic的适配 (nacos-tonic-discover)[https://crates.io/crates/nacos-tonic-discover]

### 其它

应用结束时，nacos_rust_client可能还有后台的调用，可以调用`nacos_rust_client::close_current_system()` 优雅退出nacos_rust_client后台线程。


## 例子

运行下面的例子，需要先启动nacos服务，把例子中的host改成实际的地址。

例子完整依赖与代码可以参考 examples/下的代码。


### 配置中心例子

```rust
use std::time::Duration;
use std::sync::Arc;

use nacos_rust_client::client::{ HostInfo, AuthInfo };
use nacos_rust_client::client::config_client::{
    ConfigClient,ConfigKey,ConfigDefaultListener
};
use serde::{Serialize,Deserialize};
use serde_json;

#[derive(Debug,Serialize,Deserialize,Default,Clone)]
pub struct Foo {
    pub name: String,
    pub number: u64,
}

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG","INFO");
    env_logger::init();
    //let host = HostInfo::parse("127.0.0.1:8848");
    //let config_client = ConfigClient::new(host,String::new());
    let tenant = "public".to_owned(); //default teant
    //let auth_info = Some(AuthInfo::new("nacos","nacos"));
    let auth_info = None;
    let config_client = ConfigClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848",tenant,auth_info);
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let key = ConfigKey::new("001","foo","");
    //设置
    config_client.set_config(&key, "1234").await.unwrap();
    //获取
    let v=config_client.get_config(&key).await.unwrap();
    println!("{:?},{}",&key,v);

    let mut foo_obj= Foo {
        name:"foo name".to_owned(),
        number:0u64,
    };
    let key = ConfigKey::new("foo_config","foo","");
    let foo_config_obj_listener = Box::new(ConfigDefaultListener::new(key.clone(),Arc::new(|s|{
        //字符串反序列化为对象，如:serde_json::from_str::<T>(s)
        Some(serde_json::from_str::<Foo>(s).unwrap())
    })));
    let foo_config_string_listener = Box::new(ConfigDefaultListener::new(key.clone(),Arc::new(|s|{
        //字符串反序列化为对象，如:serde_json::from_str::<T>(s)
        Some(s.to_owned())
    })));
    config_client.set_config(&key,&serde_json::to_string(&foo_obj).unwrap()).await.unwrap();
    //监听
    config_client.subscribe(foo_config_obj_listener.clone()).await;
    config_client.subscribe(foo_config_string_listener.clone()).await;
    //从监听对象中获取
    println!("key:{:?} ,value:{:?}",&key.data_id,foo_config_string_listener.get_value());
    for i in 1..10 {
        foo_obj.number=i;
        let foo_json_string = serde_json::to_string(&foo_obj).unwrap();
        config_client.set_config(&key,&foo_json_string).await.unwrap();
        // 配置推送到服务端后， 监听更新需要一点时间
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let foo_obj_from_listener = foo_config_obj_listener.get_value().unwrap();
        let foo_obj_string_from_listener = foo_config_string_listener.get_value().unwrap();
        // 监听项的内容有变更后会被服务端推送,监听项会自动更新为最新的配置
        println!("foo_obj_from_listener :{}",&foo_obj_string_from_listener);
        assert_eq!(foo_obj_string_from_listener.to_string(),foo_json_string);
        assert_eq!(foo_obj_from_listener.number,foo_obj.number);
        assert_eq!(foo_obj_from_listener.number,i);
    }

}
```


### 服务中心例子

```rust
use nacos_rust_client::client::naming_client::{ServiceInstanceKey, InstanceDefaultListener};
use std::sync::Arc;

use std::time::Duration;

use nacos_rust_client::client::{HostInfo, AuthInfo, naming_client::{NamingClient, Instance,QueryInstanceListParams}};



#[tokio::main]
async fn main(){
    //std::env::set_var("RUST_LOG","INFO");
    std::env::set_var("RUST_LOG","INFO");
    env_logger::init();
    //let host = HostInfo::parse("127.0.0.1:8848");
    //let client = NamingClient::new(host,"".to_owned());
    let namespace_id = "public".to_owned(); //default teant
    //let auth_info = Some(AuthInfo::new("nacos","nacos"));
    let auth_info = None;
    let client = NamingClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848", namespace_id, auth_info);
    let servcie_key = ServiceInstanceKey::new("foo","DEFAULT_GROUP");
    //可以通过监听器获取指定服务的最新实现列表，并支持触发变更回调函数,可用于适配微服务地址选择器。
    let default_listener = InstanceDefaultListener::new(servcie_key,Some(Arc::new(
        |instances,add_list,remove_list| {
            println!("service instances change,count:{},add count:{},remove count:{}",instances.len(),add_list.len(),remove_list.len());
        })));
    client.subscribe(Box::new(default_listener.clone())).await.unwrap();
    let ip = local_ipaddress::get().unwrap();
    let service_name = "foo";
    let group_name="DEFAULT_GROUP";
    for i in 0..10{
        let port=10000+i;
        let instance = Instance::new_simple(&ip,port,service_name,group_name);
        //注册
        client.register(instance);
        tokio::time::sleep(Duration::from_millis(1000)).await;
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
    let service_name = "foo";
    let group_name="DEFAULT_GROUP";
    let params = QueryInstanceListParams::new_simple(service_name,group_name);
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


