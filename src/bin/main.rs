use std::cell::Cell;
use std::collections::BTreeMap;
use std::collections::HashMap;

use naocs_client::client::{
    HostInfo
};
use naocs_client::client::config_client::{
    ConfigClient,ConfigKey,ConfigListener
};

struct MyItem {
    v:u32,
    map:HashMap<u32,Vec<String>>,
}

impl MyItem {
    fn new() -> MyItem {
        MyItem{
            v:0,
            map:HashMap::new(),
        }
    }

    fn add(&mut self,key:u32,val:&str){
        match self.map.get_mut(&key) {
            Some(list) => {
                list.remove(0);
            },
            None => {},
        };
    }
}

#[tokio::main]
async fn main() {
    let mut bmap = BTreeMap::<usize,usize>::new();
    bmap.insert(9,9);
    bmap.insert(8,8);
    bmap.insert(7,7);
    bmap.insert(6,6);
    bmap.insert(5,5);
    bmap.insert(4,4);
    bmap.insert(3,3);
    bmap.insert(2,2);
    bmap.insert(1,1);
    bmap.insert(0,0);
    for item in bmap.iter() {
        println!("{:?}",item);
    }
    for item in bmap.iter().take(20) {
        println!("{:?}",item);
    }

    //let a = Cell::new(Some(MyItem::new()));
    //a.replace(None);


    println!("--------");
    println!("Hello, world!");
    test01().await;
}

struct L01;

impl ConfigListener for L01 {
    fn enable(&self) -> bool {
        true
    }
    fn change(&self,key:&ConfigKey,value:&str) -> () {
        println!("{:?}",&key);
        ()
    }
}

async fn test01(){
    let host = HostInfo::parse("127.0.0.1:8848");
    let config_client = ConfigClient::new(host,String::new());
    let key = config_client.gene_config_key("foo", "001");
    config_client.set_config(&key, "1234").await.unwrap();
    let v=config_client.get_config(&key).await.unwrap();
    println!("{:?},{}",&key,v);
    config_client.del_config(&key).await.unwrap();
    let v=config_client.get_config(&key).await;
    println!("{:?},{:?}",&key,v);
    let a = L01;
    config_client.subscribe(key, a).await;
}
