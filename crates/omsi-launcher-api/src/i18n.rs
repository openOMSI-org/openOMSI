//! The interface texts in the player's language: the game's own tables
//! (`crates/omsi-app/locales/app.yml`, the English text is the key), handed to the page
//! whole for its language.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Every text of the tables in `lang` (`ru`, `de` ...; empty for English: nothing).
pub fn table(lang: &str) -> Arc<HashMap<String, String>> {
    static READ: Mutex<Option<HashMap<String, Arc<HashMap<String, String>>>>> = Mutex::new(None);
    let mut g = READ.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(HashMap::new).entry(lang.to_string()).or_insert_with(|| Arc::new(omsi_launcher_lib::i18n::table(omsi_launcher_lib::i18n::TABLES, lang))).clone()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tables_parse() {
        assert!(super::table("ru").contains_key("Current time"));
        assert!(super::table("").is_empty());
    }
}
