use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleResult<T>
where
    T: Sized + Serialize + Clone + Default,
{
    pub code: i64,
    pub message: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceInfo {
    pub namespace: Option<String>,
    pub namespace_show_name: Option<String>,
    pub namespace_desc: Option<String>,
    pub quota: i64,
    pub config_count: i64,
    pub r#type: i64,
}
