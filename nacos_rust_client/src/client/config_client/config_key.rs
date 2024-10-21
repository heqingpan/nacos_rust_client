#[derive(Debug, Hash, PartialEq, Eq, Clone, Default)]
pub struct ConfigKey {
    pub data_id: String,
    pub group: String,
    pub tenant: String,
}

impl ConfigKey {
    pub fn new(data_id: &str, group: &str, tenant: &str) -> Self {
        Self {
            data_id: data_id.to_owned(),
            group: group.to_owned(),
            tenant: tenant.to_owned(),
        }
    }

    pub fn build_key(&self) -> String {
        if self.tenant.is_empty() {
            return format!("{}\x02{}", self.data_id, self.group);
        }
        format!("{}\x02{}\x02{}", self.data_id, self.group, self.tenant)
    }
}

/*
impl PartialEq for ConfigKey {
    fn eq(&self, o: &Self) -> bool {
        self.data_id == o.data_id && self.group == o.group && self.tenant == self.tenant
    }
}
*/
