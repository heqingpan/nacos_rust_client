use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSearchPage<T>
where
    T: Debug + Sized + Serialize + Clone + Default,
{
    pub total_count: Option<usize>,
    pub page_number: Option<usize>,
    pub pages_available: Option<usize>,
    pub page_items: Option<Vec<T>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInfoDto {
    pub tenant: Option<String>,
    pub group: String,
    pub data_id: String,
    pub content: Option<String>,
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigQueryParams {
    pub data_id: String,
    pub group: String,
    pub tenant: Option<String>,
    pub desc: Option<String>,
    pub r#type: Option<String>,
    pub search: Option<String>,   //search type
    pub page_no: Option<usize>,   //use at search
    pub page_size: Option<usize>, //use at search
}
