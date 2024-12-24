use examples::proto::helloworld::greeter_client::GreeterClient;
use examples::proto::helloworld::HelloRequest;
use nacos_rust_client::client::naming_client::NamingClient;
use nacos_rust_client::client::naming_client::{QueryInstanceListParams, ServiceInstanceKey};
use nacos_tonic_discover::TonicDiscoverFactory;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let namespace_id = "public".to_owned(); //default teant
    let auth_info = None; // Some(AuthInfo::new("nacos","nacos"))
    let naming_client =
        NamingClient::new_with_addrs("127.0.0.1:8848,127.0.0.1:8848", namespace_id, auth_info);

    let service_key = ServiceInstanceKey::new("helloworld", "AppName");
    //build client by discover factory
    let _ = TonicDiscoverFactory::new(naming_client.clone());
    let discover_factory = nacos_tonic_discover::get_last_factory().unwrap();
    let channel = discover_factory
        .build_service_channel(service_key.clone())
        .await?;
    let mut client = GreeterClient::new(channel);

    for i in 0..5 {
        let request = tonic::Request::new(HelloRequest {
            name: format!("Tonic {} [client by discover factory]", i),
        });
        let response = client.say_hello(request).await?;
        println!("RESPONSE={:?}", response);
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    //build client by naming client select
    let param = QueryInstanceListParams::new_by_serivce_key(&service_key);

    for i in 5..10 {
        let instance = naming_client.select_instance(param.clone()).await?;
        let mut client =
            GreeterClient::connect(format!("http://{}:{}", &instance.ip, &instance.port)).await?;
        let request = tonic::Request::new(HelloRequest {
            name: format!("Tonic {} [client by naming client select]", i),
        });
        let response = client.say_hello(request).await?;
        println!("RESPONSE={:?}", response);
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Ok(())
}
