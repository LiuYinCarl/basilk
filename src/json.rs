use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

use crate::{
    migration::{Migration, JSON_VERSIONS},
    project::Project,
};

pub struct Json;

static DIR_CONFIG_NAME: &str = env!("CARGO_PKG_NAME");
static DATA_FILE_NAME: &str = "basilk_data.json";
static VERSION: Mutex<String> = Mutex::new(String::new());

#[derive(Serialize, Deserialize)]
struct DataWrapper {
    version: String,
    data: Vec<Project>,
}

impl Json {
    pub fn get_dir_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap();
        path.push(DIR_CONFIG_NAME);

        return path;
    }

    fn get_data_path() -> PathBuf {
        let mut path = PathBuf::new();
        path.push(Json::get_dir_path().as_path());
        path.push(DATA_FILE_NAME);

        return path;
    }

    fn get_json_path(version: String) -> PathBuf {
        let mut path = PathBuf::new();
        path.push(Json::get_dir_path().as_path());
        path.push(format!("{version}.json"));

        return path;
    }

    pub fn check() -> Result<bool, Box<dyn Error>> {
        fs::create_dir_all(Json::get_dir_path())?;

        let mut version_state = VERSION.lock().unwrap();
        let data_path = Json::get_data_path();

        if data_path.is_file() {
            let json_raw = fs::read_to_string(&data_path)?;
            if json_raw.trim().is_empty() {
                let last_json_version = JSON_VERSIONS.last().unwrap();
                version_state.clear();
                version_state.push_str(last_json_version);
                drop(version_state);
                Json::write(vec![]);
                return Ok(false);
            }
            let wrapper: DataWrapper = from_str(&json_raw)?;
            version_state.clear();
            version_state.push_str(&wrapper.version);

            let migrations = Migration::get_migrations(&wrapper.version, wrapper.data);

            if migrations.is_empty() {
                return Ok(false);
            }

            for (version, migration_data) in migrations.iter() {
                version_state.clear();
                version_state.push_str(version);
                Json::write_internal(&data_path, version_state.to_string(), migration_data.clone());
            }

            return Ok(true);
        }

        // Migration from old versioned files
        let json_version_from_file: Vec<&str> = JSON_VERSIONS
            .into_iter()
            .filter(|version| {
                let p = Json::get_json_path(version.to_string());
                p.is_file()
            })
            .collect();

        if json_version_from_file.is_empty() {
            let last_json_version = JSON_VERSIONS.last().unwrap();
            version_state.clear();
            version_state.push_str(last_json_version);
            let projects: Vec<Project> = vec![];
            let wrapper = DataWrapper { version: last_json_version.to_string(), data: projects };
            fs::write(&data_path, to_string(&wrapper).unwrap()).unwrap();
            return Ok(false);
        }

        let old_version = json_version_from_file[0];
        let old_path = Json::get_json_path(old_version.to_string());
        let json_raw = fs::read_to_string(&old_path)?;
        let data = from_str::<Vec<Project>>(&json_raw)?;

        version_state.clear();
        version_state.push_str(old_version);
        let wrapper = DataWrapper { version: old_version.to_string(), data };
        fs::write(&data_path, to_string(&wrapper).unwrap()).unwrap();

        // Optionally delete old file
        let _ = fs::remove_file(old_path);

        // Re-run check to apply any further migrations
        drop(version_state);
        return Json::check();
    }

    pub fn read() -> Vec<Project> {
        let path = Json::get_data_path();
        let json = fs::read_to_string(path).unwrap();
        let wrapper: DataWrapper = from_str(&json).unwrap();

        let mut version_state = VERSION.lock().unwrap();
        version_state.clear();
        version_state.push_str(&wrapper.version);

        return wrapper.data;
    }

    pub fn write(projects: Vec<Project>) {
        let version = VERSION.lock().unwrap().to_string();
        let path = Json::get_data_path();

        Json::write_internal(&path, version, projects);
    }

    fn write_internal(path: &PathBuf, version: String, data: Vec<Project>) {
        let wrapper = DataWrapper { version, data };
        fs::write(path, to_string(&wrapper).unwrap()).unwrap();
    }
}
