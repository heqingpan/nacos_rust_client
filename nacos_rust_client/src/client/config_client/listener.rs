use std::sync::Arc;

use super::config_key::ConfigKey;

pub struct ListenerItem {
    pub key: ConfigKey,
    pub md5: String,
}

impl ListenerItem {
    pub fn new(key: ConfigKey, md5: String) -> Self {
        Self { key, md5 }
    }

    pub fn decode_listener_items(configs: &str) -> Vec<Self> {
        let mut list = vec![];
        let mut start = 0;
        let bytes = configs.as_bytes();
        let mut tmp_list = vec![];
        for i in 0..bytes.len() {
            let char = bytes[i];
            if char == 2 {
                if tmp_list.len() > 2 {
                    continue;
                }
                //tmpList.push(configs[start..i].to_owned());
                tmp_list.push(String::from_utf8(configs[start..i].into()).unwrap());
                start = i + 1;
            } else if char == 1 {
                let mut end_value = String::new();
                if start < i {
                    //endValue = configs[start..i].to_owned();
                    end_value = String::from_utf8(configs[start..i].into()).unwrap();
                }
                start = i + 1;
                if tmp_list.len() == 2 {
                    let key = ConfigKey::new(&tmp_list[0], &tmp_list[1], "");
                    list.push(ListenerItem::new(key, end_value));
                } else {
                    let key = ConfigKey::new(&tmp_list[0], &tmp_list[1], &end_value);
                    list.push(ListenerItem::new(key, tmp_list[2].to_owned()));
                }
                tmp_list.clear();
            }
        }
        list
    }

    pub fn decode_listener_change_keys(configs: &str) -> Vec<ConfigKey> {
        let mut list = vec![];
        let mut start = 0;
        let bytes = configs.as_bytes();
        let mut tmp_list = vec![];
        for i in 0..bytes.len() {
            let char = bytes[i];
            if char == 2 {
                if tmp_list.len() > 2 {
                    continue;
                }
                //tmpList.push(configs[start..i].to_owned());
                tmp_list.push(String::from_utf8(configs[start..i].into()).unwrap());
                start = i + 1;
            } else if char == 1 {
                let mut end_value = String::new();
                if start < i {
                    //endValue = configs[start..i].to_owned();
                    end_value = String::from_utf8(configs[start..i].into()).unwrap();
                }
                start = i + 1;
                if tmp_list.len() == 1 {
                    let key = ConfigKey::new(&tmp_list[0], &end_value, "");
                    list.push(key);
                } else {
                    let key = ConfigKey::new(&tmp_list[0], &tmp_list[1], &end_value);
                    list.push(key);
                }
                tmp_list.clear();
            }
        }
        list
    }
}

pub trait ConfigListener {
    fn get_key(&self) -> ConfigKey;
    fn change(&self, key: &ConfigKey, value: &str);
}

pub type ListenerConvert<T> = Arc<dyn Fn(&str) -> Option<T> + Send + Sync>;

#[derive(Clone)]
pub struct ConfigDefaultListener<T> {
    key: ConfigKey,
    pub content: Arc<std::sync::RwLock<Option<Arc<T>>>>,
    pub convert: ListenerConvert<T>,
}

impl<T> ConfigDefaultListener<T> {
    pub fn new(key: ConfigKey, convert: ListenerConvert<T>) -> Self {
        Self {
            key,
            content: Default::default(),
            convert,
        }
    }

    pub fn get_value(&self) -> Option<Arc<T>> {
        self.content.read().unwrap().as_ref().map(|c| c.clone())
    }

    fn set_value(content: Arc<std::sync::RwLock<Option<Arc<T>>>>, value: T) {
        let mut r = content.write().unwrap();
        *r = Some(Arc::new(value));
    }
}

impl<T> ConfigListener for ConfigDefaultListener<T> {
    fn get_key(&self) -> ConfigKey {
        self.key.clone()
    }

    fn change(&self, key: &ConfigKey, value: &str) {
        log::debug!("ConfigDefaultListener change:{:?},{}", key, value);
        let content = self.content.clone();
        let convert = self.convert.as_ref();
        if let Some(value) = convert(value) {
            Self::set_value(content, value);
        }
    }
}

pub(crate) struct ListenerValue {
    pub(crate) md5: String,
    listeners: Vec<(u64, Box<dyn ConfigListener + Send>)>,
}

impl ListenerValue {
    pub(crate) fn new(listeners: Vec<(u64, Box<dyn ConfigListener + Send>)>, md5: String) -> Self {
        Self { md5, listeners }
    }

    pub(crate) fn push(&mut self, id: u64, func: Box<dyn ConfigListener + Send>) {
        self.listeners.push((id, func));
    }

    pub(crate) fn notify(&self, key: &ConfigKey, content: &str) {
        for (_, func) in self.listeners.iter() {
            func.change(key, content);
        }
    }

    pub(crate) fn remove(&mut self, id: u64) -> usize {
        let mut indexs = Vec::new();
        for i in 0..self.listeners.len() {
            if let Some((item_id, _)) = self.listeners.get(i) {
                if id == *item_id {
                    indexs.push(i);
                }
            }
        }
        for index in indexs.iter().rev() {
            let index = *index;
            self.listeners.remove(index);
        }
        self.listeners.len()
    }
}
