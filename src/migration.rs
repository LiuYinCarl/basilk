use serde_json::{
    json, to_string,
    Value::{self},
};

//                              sha of 0.1.0     0.2.0    0.2.2
pub static JSON_VERSIONS: [&str; 3] = ["6ad96", "911fc", "a4e1b"];

pub struct Migration;

impl Migration {
    pub fn get_migrations(version: &str, original_json: Vec<Value>) -> Vec<(&str, String)> {
        // Mapper between json version and the relative migration
        let mapper: Vec<(&str, String)> = vec![
            ("6ad96", "".to_string()),
            ("911fc", Migration::add_priority(original_json.clone())),
            ("a4e1b", Migration::add_note(original_json)),
        ];

        // The start index where the migration are picked
        let start_index = mapper
            .clone()
            .into_iter()
            .position(|(key, _val)| key == version);

        if start_index.is_none() {
            return vec![];
        }

        let all_migrations: Vec<(&str, String)> = mapper.into_iter().collect();

        // Slice for pick only the useful migration
        return all_migrations[(start_index.unwrap() + 1)..].to_vec();
    }

    // Migrations
    fn add_priority(mut json: Vec<Value>) -> String {
        for t in json.iter_mut().flat_map(|p| p.get_mut("tasks")).flat_map(|t| t.as_array_mut()).flatten() {
            t.as_object_mut().unwrap().insert("priority".to_string(), json!(0));
        }
        to_string(&json).unwrap()
    }

    fn add_note(mut json: Vec<Value>) -> String {
        for t in json.iter_mut().flat_map(|p| p.get_mut("tasks")).flat_map(|t| t.as_array_mut()).flatten() {
            t.as_object_mut().unwrap().insert("note".to_string(), json!(""));
        }
        to_string(&json).unwrap()
    }
}
