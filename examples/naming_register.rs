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
    loop{
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
