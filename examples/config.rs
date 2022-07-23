use std::time::Duration;
use std::sync::Arc;

use nacos_rust_client::client::{
    HostInfo, AuthInfo
};
use nacos_rust_client::client::config_client::{
    ConfigClient,ConfigKey,ConfigListener,ConfigDefaultListener
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
    let auth_info = Some(AuthInfo::new("nacos","nacos"));
    //let auth_info = None;
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