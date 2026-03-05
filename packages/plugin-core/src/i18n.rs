use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, utoipa::ToSchema, Default)]
#[schema(example = json!({
    "sidebar.problems": "题目",
    "sidebar.plugins": "插件"
}))]
pub struct TranslationMap(HashMap<String, String>);

impl TranslationMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl From<HashMap<String, String>> for TranslationMap {
    fn from(value: HashMap<String, String>) -> Self {
        Self(value)
    }
}

pub type I18nData = HashMap<String, TranslationMap>;

#[derive(Default)]
pub struct I18nRegistry {
    data: Arc<RwLock<I18nData>>,
}

impl I18nRegistry {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }

    pub fn merge(&self, lang: String, translation: TranslationMap) {
        self.data.write().unwrap().insert(lang, translation);
    }

    pub fn get_locales(&self) -> Vec<String> {
        self.data
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<String>>()
    }

    pub fn get_translations(&self, lang: &str) -> Option<TranslationMap> {
        self.data.read().unwrap().get(lang).cloned()
    }
}
