use std::collections::HashMap;
use serde::{Serialize,Deserialize};

use super::Instance;


#[derive(Debug,Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct BeatInfo {
    pub cluster:String,
    pub ip:String,
    pub port:u32,
    pub metadata:HashMap<String,String>,
    pub period:i64,
    pub scheduled:bool,
    pub service_name:String,
    pub stopped:bool,
    pub weight:f32,
}

#[derive(Debug,Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct BeatRequest{
    pub namespace_id:String,
    pub service_name:String,
    pub cluster_name:String,
    pub group_name:String,
    pub ephemeral:Option<String>,
    pub beat:String,
}

#[derive(Debug,Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct InstanceWebParams {
    pub ip:String,
    pub port:u32,
    pub namespace_id:String,
    pub weight: f32,
    pub enabled:bool,
    pub healthy:bool,
    pub ephemeral:bool,
    pub metadata:String,
    pub cluster_name:String,
    pub service_name:String,
    pub group_name:String,
}

#[derive(Debug,Default,Serialize,Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceWebQueryListParams {
    pub namespace_id:String,
    pub service_name:String,
    pub group_name:String,
    pub clusters:String,
    pub healthy_only:bool,
    #[serde(rename = "clientIP")]
    pub client_ip:Option<String>,
    pub udp_port:Option<u16>,
}

#[derive(Debug,Serialize,Deserialize,Default,Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstanceVO {
    service:Option<String>,
    ip:Option<String>,
    port:Option<u32>,
    cluster_name:Option<String>,
    weight:Option<f32>,
    healthy:Option<bool>,
    instance_id:Option<String>,
    metadata:Option<HashMap<String,String>>,
    marked:Option<bool>,
    enabled:Option<bool>,
    service_name:Option<String>,
    ephemeral:Option<bool>,
}

impl InstanceVO {
    pub fn get_group_name(&self) -> String {
        if let Some(service) = &self.service {
            if let Some((group_name,_)) = NamingUtils::split_group_and_serivce_name(service){
                return group_name;
            }
        }
        "DEFAULT_GROUP".to_owned()
    }

    pub fn to_instance(self) -> Instance {
        let mut instance = Instance::default();
        if let Some(service) = &self.service {
            if let Some((group_name,service_name)) = NamingUtils::split_group_and_serivce_name(service){
                instance.group_name = group_name;
                instance.service_name = service_name;
            }
        }
        //if let Some(service_name) = self.serviceName { instance.service_name = service_name; }
        if let Some(ip) = self.ip { instance.ip = ip; }
        if let Some(port) = self.port { instance.port = port; }
        if let Some(cluster_name) = self.cluster_name { instance.cluster_name= cluster_name; }
        if let Some(weight) = self.weight { instance.weight= weight; }
        if let Some(healthy) = self.healthy { instance.healthy= healthy; }
        instance.metadata=self.metadata;
        if let Some(enabled) = self.enabled { instance.enabled= enabled; }
        if let Some(ephemeral) = self.ephemeral { instance.ephemeral= ephemeral; }
        instance
    }
}

#[derive(Debug,Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryListResult {
    pub name:Option<String>,
    pub clusters:Option<String>,
    pub cache_millis:Option<u64>,
    pub hosts:Option<Vec<InstanceVO>>,
    pub last_ref_time:Option<i64>,
    pub checksum:Option<String>,
    #[serde(rename = "useSpecifiedURL")]
    pub use_specified_url:Option<bool>,
    pub env:Option<String>,
    pub protect_threshold:Option<f32>,
    pub reach_local_site_call_threshold:Option<bool>,
    pub dom:Option<String>,
    pub metadata:Option<HashMap<String,String>>,
}


pub struct NamingUtils;

impl NamingUtils {
    pub fn get_group_and_service_name(service_name:&str,group_name:&str) -> String {
        if group_name.len()==0 {
            return format!("DEFAULT_GROUP@@{}",service_name)
        }
        format!("{}@@{}",group_name,service_name)
    }

    pub fn split_group_and_serivce_name(grouped_name:&str) -> Option<(String,String)> {
        let split = grouped_name.split("@@").collect::<Vec<_>>();
        if split.len() ==0 {
            return None
        }
        let a = split.get(0);
        let b = split.get(1);
        match b {
            Some(b) => {
                let a = a.unwrap();
                if a.len()==0 {
                    return None;
                }
                Some(((*a).to_owned(),(*b).to_owned()))
            },
            None=>{
                match a{
                    Some(a) => {
                        if a.len()==0{
                            return None;
                        }
                        Some(("DEFAULT_GROUP".to_owned(),(*a).to_owned()))
                    },
                    None => {
                        None
                    }
                }
            }
        }
    }


    fn do_select_index(list:&Vec<u64>,rand_value:u64) -> usize {
        match list.binary_search(&rand_value) {
            Ok(i) => {
                i
            },
            Err(i) => {
                let len = list.len();
                if i >= len {len} else {i}
            },
        }
    }

    pub fn select_by_weight(weight_list:&Vec<u64>) -> usize {
        use rand::prelude::*;
        use rand::distributions::Uniform;

        let mut superposition_list = vec![];
        let mut sum=0;
        for v in weight_list {
            sum+= *v;
            superposition_list.push(sum);
        }
        if sum==0 {
            return 0;
        }
        //let rng = rand::thread_rng();
        let mut rng: StdRng = StdRng::from_entropy();
        let range_uniform=Uniform::new(0,sum);
        let rand_value = range_uniform.sample(&mut rng);
        //let rand_value= rand::thread_rng().gen_range(0..sum);
        Self::do_select_index(&superposition_list, rand_value)
    }

    pub fn select_by_weight_fn<T,F>(list:&Vec<T>,f:F) -> usize
    where F: Fn(&T) -> u64
    {
        let weight_list:Vec<u64> = list.iter().map(|e|f(e)).collect();
        Self::select_by_weight(&weight_list)
    }
}