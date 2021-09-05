use std::io::{Read, stdin};

use nacos_rust_client::client::{HostInfo, naming_client::{NamingClient, RegisterInstance}};



fn main(){
    let host = HostInfo::parse("127.0.0.1:8848");
    let client = NamingClient::new(host,"".to_owned());

    let ip = local_ipaddress::get().unwrap();

    let instance = RegisterInstance::new(&ip,8081,"foo","","","",None);
    client.register(instance);
    let instance = RegisterInstance::new(&ip,8081,"foo2","","","",None);
    client.register(instance);
    let mut buf = vec![0u8;1];
    stdin().read(&mut buf).unwrap();
}