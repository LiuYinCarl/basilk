use crate::project::Project;

//                              sha of 0.1.0     0.2.0    0.2.2
pub static JSON_VERSIONS: [&str; 3] = ["6ad96", "911fc", "a4e1b"];

pub struct Migration;

impl Migration {
    pub fn get_migrations(version: &str, original_json: Vec<Project>) -> Vec<(&str, Vec<Project>)> {
        // Mapper between json version and the relative migration
        let mapper: Vec<(&str, fn(Vec<Project>) -> Vec<Project>)> = vec![
            ("6ad96", |data| data),
            ("911fc", Migration::add_priority),
            ("a4e1b", Migration::add_note),
        ];

        // The start index where the migration are picked
        let start_index = mapper
            .iter()
            .position(|(key, _val)| *key == version);

        if start_index.is_none() {
            return vec![];
        }

        let mut results = vec![];
        let mut current_data = original_json;

        for (v, migration_fn) in mapper.into_iter().skip(start_index.unwrap() + 1) {
            current_data = migration_fn(current_data);
            results.push((v, current_data.clone()));
        }

        return results;
    }

    // Migrations
    fn add_priority(mut data: Vec<Project>) -> Vec<Project> {
        for p in data.iter_mut() {
            for t in p.tasks.iter_mut() {
                t.priority = 0;
            }
        }
        data
    }

    fn add_note(mut data: Vec<Project>) -> Vec<Project> {
        for p in data.iter_mut() {
            for t in p.tasks.iter_mut() {
                t.note = "".to_string();
            }
        }
        data
    }
}
