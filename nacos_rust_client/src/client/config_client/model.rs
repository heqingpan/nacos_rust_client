use super::ConfigKey;


#[derive(Debug,Default,Clone)]
pub struct NotifyConfigItem {
    pub key:ConfigKey,
    pub content:String,
    pub md5:String,
}

