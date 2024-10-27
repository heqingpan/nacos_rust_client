#![allow(unused_imports)]
use std::sync::Arc;
use std::time::Duration;

use nacos_rust_client::client::api_model::NamespaceInfo;
use nacos_rust_client::client::config_client::api_model::ConfigQueryParams;
use nacos_rust_client::client::{AuthInfo, ClientBuilder, ConfigClient, HostInfo};
use serde::{Deserialize, Serialize};
use serde_json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    std::env::set_var("RUST_LOG", "INFO");
    env_logger::init();
    let tenant = "public".to_owned(); //default teant
    let auth_info = Some(AuthInfo::new("nacos", "nacos"));
    //let auth_info = None;
    let config_client = ClientBuilder::new()
        .set_endpoint_addrs("127.0.0.1:8848,127.0.0.1:8848")
        .set_auth_info(auth_info)
        .set_tenant(tenant)
        .set_use_grpc(false)
        .build_config_client();
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let result = config_client.get_namespace_list().await?;
    let namespaces = result.data.unwrap_or_default();
    log::info!("namespace count:{}", namespaces.len());
    for item in &namespaces {
        log::info!("namespace item: {:?}", item.namespace)
    }
    log::info!("---- query config list ----\n\n");
    for item in &namespaces {
        print_config_list(&config_client, item).await?;
    }
    nacos_rust_client::close_current_system();
    Ok(())
}

async fn print_config_list(
    config_client: &ConfigClient,
    namespace_info: &NamespaceInfo,
) -> anyhow::Result<()> {
    let namespace_id = if let Some(namespace_id) = namespace_info.namespace.as_ref() {
        namespace_id
    } else {
        return Err(anyhow::anyhow!("namespace_id is none"));
    };
    let mut current_page = 0;
    let mut total_page = 1;
    let mut total_count = 0;
    let mut params = ConfigQueryParams {
        tenant: Some(namespace_id.to_owned()),
        page_no: Some(1),
        page_size: Some(100),
        ..Default::default()
    };
    while current_page < total_page {
        current_page += 1;
        params.page_no = Some(current_page);
        let res = config_client
            .query_accurate_config_page(params.clone())
            .await?;
        total_page = res.pages_available.unwrap_or_default();
        total_count = res.total_count.unwrap_or_default();
        let configs = res.page_items.unwrap_or_default();
        for config in &configs {
            log::info!(
                "config item,tenant:{:?},data:{},group:{}",
                config.tenant,
                config.data_id,
                config.group,
            )
        }
        if configs.is_empty() {
            break;
        }
    }
    log::info!("[namespace {}],config count:{}", namespace_id, total_count);
    Ok(())
}
