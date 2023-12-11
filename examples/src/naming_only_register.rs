#![allow(unused_imports, unreachable_code)]
use actix::{Actor, Addr};
use ctrlc;
use nacos_rust_client::client::naming_client::{InstanceDefaultListener, ServiceInstanceKey};
use nacos_rust_client::conn_manage;
use nacos_rust_client::conn_manage::conn_msg::NamingRequest;
use nacos_rust_client::conn_manage::manage::ConnManage;
use std::sync::mpsc::channel;
use std::sync::Arc;

use std::time::Duration;

use nacos_rust_client::client::naming_client::{Instance, NamingClient, QueryInstanceListParams};
use nacos_rust_client::client::{AuthInfo, HostInfo, ServerEndpointInfo};

fn get_service_ip_list(hundreds_count: u64) -> Vec<String> {
    let mut rlist = vec![];
    for i in 100..(100 + hundreds_count) {
        for j in 100..200 {
            let ip = format!("192.168.{}.{}", &i, &j);
            rlist.push(ip);
        }
    }
    rlist
}

/*
async fn register(service_name:&str,group_name:&str,ips:&Vec<String>,port:u32,client:&NamingClient) {
    log::info!("register,{},{}",service_name,group_name);
    for ip in ips {
        let instance = Instance::new_simple(&ip,port,service_name,group_name);
        //注册
        client.register(instance);
    }
}
*/

fn register(
    service_name: &str,
    group_name: &str,
    ips: &Vec<String>,
    port: u32,
    conn_manage: &Addr<ConnManage>,
) {
    log::info!("register,{},{}", service_name, group_name);
    for ip in ips {
        let instance = Instance::new_simple(&ip, port, service_name, group_name);
        //注册
        let msg = NamingRequest::Register(instance);
        conn_manage.do_send(msg);
    }
}

fn start_actor_at_new_thread<T>(actor: T) -> Addr<T>
where
    T: Actor<Context = actix::Context<T>> + Send,
{
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let rt = actix::System::new();
        let addrs = rt.block_on(async { actor.start() });
        tx.send(addrs).unwrap();
        rt.run().unwrap();
    });
    let addrs = rx.recv().unwrap();
    addrs
}

/*
fn build_one_manage_addr(conn_manage:ConnManage) -> Addr<ConnManage> {
    let (tx,rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let rt = actix::System::new();
        let addrs = rt.block_on(async {
            conn_manage.start()
        });
        tx.send(addrs).unwrap();
        rt.run().unwrap();
    });
    let addrs = rx.recv().unwrap();
    addrs
}
*/

fn build_conn_manages(thread_num: u16) -> Vec<Addr<ConnManage>> {
    let mut rlist = vec![];
    let endpoint = ServerEndpointInfo::new("127.0.0.1:8848,127.0.0.1:8848");
    for _ in 0..thread_num {
        let conn_manage = ConnManage::new(
            endpoint.hosts.clone(),
            true,
            None,
            Default::default(),
            Default::default(),
        );
        let conn_manage_addr = start_actor_at_new_thread(conn_manage);
        rlist.push(conn_manage_addr);
    }
    rlist
}

fn main() {
    let (tx, rx) = channel();
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");
    std::env::set_var("RUST_LOG", "INFO");
    env_logger::init();
    let thread_num = 10usize;
    let addrs = build_conn_manages(thread_num as u16);
    std::thread::sleep(Duration::from_millis(1000));

    let ips = get_service_ip_list(1);
    let group_name = "DEFAULT_GROUP";
    for i in 0..100 {
        let index = i % thread_num;
        let conn_manage_addr = &addrs.get(index).unwrap();
        let service_name = format!("foo_{}", i);
        register(&service_name, group_name, &ips, 10000, &conn_manage_addr);
    }

    println!("Waiting for Ctrl-C...");
    rx.recv().expect("Could not receive from channel.");
    println!("Got it! Exiting...");
}
