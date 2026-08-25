use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

use crate::{
    migration::{Migration, JSON_VERSIONS},
    note::Note,
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
    /// Global notes; `default` keeps pre-notes data files loadable.
    #[serde(default)]
    notes: Vec<Note>,
}

impl Json {
    pub fn get_dir_path() -> PathBuf {
        if let Ok(dir) = std::env::var("BASILK_CONFIG_DIR") {
            return PathBuf::from(dir);
        }

        let mut path = dirs::config_dir().unwrap();
        path.push(DIR_CONFIG_NAME);

        path
    }

    fn get_data_path() -> PathBuf {
        let mut path = PathBuf::new();
        path.push(Json::get_dir_path().as_path());
        path.push(DATA_FILE_NAME);

        path
    }

    fn get_json_path(version: String) -> PathBuf {
        let mut path = PathBuf::new();
        path.push(Json::get_dir_path().as_path());
        path.push(format!("{version}.json"));

        path
    }

    pub fn check() -> Result<bool, Box<dyn Error>> {
        fs::create_dir_all(Json::get_dir_path())?;
        Json::check_data()
    }

    /// Decide how to bring the data file up to date: migrate the current
    /// file in place, or bootstrap it from the legacy versioned files.
    fn check_data() -> Result<bool, Box<dyn Error>> {
        let version_state = VERSION.lock().unwrap();
        let data_path = Json::get_data_path();

        if data_path.is_file() {
            Json::check_data_file(version_state, &data_path)
        } else {
            Json::check_legacy_files(version_state, &data_path)
        }
    }

    /// The current data file exists. An empty file is reset to the latest
    /// version; otherwise the stored version is recorded and any pending
    /// migrations are applied one by one.
    fn check_data_file(
        mut version_state: MutexGuard<String>,
        data_path: &PathBuf,
    ) -> Result<bool, Box<dyn Error>> {
        let json_raw = fs::read_to_string(data_path)?;

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
            Json::write_internal(
                data_path,
                version_state.to_string(),
                migration_data.clone(),
                wrapper.notes.clone(),
            );
        }

        Ok(true)
    }

    /// No current data file: migrate the oldest legacy versioned file into
    /// the new format, or seed a fresh empty data file when none exists.
    /// After a legacy migration the whole check re-runs so that any further
    /// migrations are applied on top.
    fn check_legacy_files(
        mut version_state: MutexGuard<String>,
        data_path: &PathBuf,
    ) -> Result<bool, Box<dyn Error>> {
        let old_version = JSON_VERSIONS
            .into_iter()
            .find(|version| Json::get_json_path(version.to_string()).is_file());

        let Some(old_version) = old_version else {
            let last_json_version = JSON_VERSIONS.last().unwrap();
            version_state.clear();
            version_state.push_str(last_json_version);
            drop(version_state);
            Json::write(vec![]);
            return Ok(false);
        };

        let old_path = Json::get_json_path(old_version.to_string());
        let json_raw = fs::read_to_string(&old_path)?;
        let data = from_str::<Vec<Project>>(&json_raw)?;

        version_state.clear();
        version_state.push_str(old_version);
        let wrapper = DataWrapper {
            version: old_version.to_string(),
            data,
            notes: vec![],
        };
        fs::write(data_path, to_string(&wrapper).unwrap()).unwrap();

        // Optionally delete old file
        let _ = fs::remove_file(old_path);

        // Re-run check to apply any further migrations
        drop(version_state);
        Json::check()
    }

    pub fn read() -> Vec<Project> {
        let path = Json::get_data_path();
        let json = fs::read_to_string(path).unwrap();
        let wrapper: DataWrapper = from_str(&json).unwrap();

        let mut version_state = VERSION.lock().unwrap();
        version_state.clear();
        version_state.push_str(&wrapper.version);

        wrapper.data
    }

    /// Write the project list, keeping the notes already on disk.
    pub fn write(projects: Vec<Project>) {
        let version = VERSION.lock().unwrap().to_string();
        let path = Json::get_data_path();

        Json::write_internal(&path, version, projects, Json::read_notes());
    }

    pub fn read_notes() -> Vec<Note> {
        let path = Json::get_data_path();
        fs::read_to_string(path)
            .ok()
            .and_then(|json| from_str::<DataWrapper>(&json).ok())
            .map(|wrapper| wrapper.notes)
            .unwrap_or_default()
    }

    /// Write the note list, keeping the projects already on disk.
    pub fn write_notes(notes: Vec<Note>) {
        let version = VERSION.lock().unwrap().to_string();
        let path = Json::get_data_path();

        Json::write_internal(&path, version, Json::read(), notes);
    }

    fn write_internal(path: &PathBuf, version: String, data: Vec<Project>, notes: Vec<Note>) {
        let wrapper = DataWrapper {
            version,
            data,
            notes,
        };
        fs::write(path, to_string(&wrapper).unwrap()).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use crate::task::{TASK_PRIORITY_NONE, TASK_STATUS_DONE};
    use crate::test_utils::{make_task, ENV_LOCK};

    #[test]
    fn check_creates_empty_data_file_and_read_returns_empty() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());

        let migrated = Json::check().unwrap();
        assert!(!migrated);
        assert!(Json::get_data_path().is_file());
        assert_eq!(Json::read(), vec![]);
    }

    #[test]
    fn write_then_read_round_trips() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());
        Json::check().unwrap();

        let projects = vec![Project {
            title: "p".to_string(),
            tasks: vec![make_task("t", TASK_STATUS_DONE, TASK_PRIORITY_NONE)],
        }];

        Json::write(projects.clone());
        assert_eq!(Json::read(), projects);
    }

    #[test]
    fn check_resets_an_empty_data_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());

        fs::write(Json::get_data_path(), "  ").unwrap();
        let migrated = Json::check().unwrap();

        assert!(!migrated);
        assert_eq!(Json::read(), vec![]);
    }

    #[test]
    fn notes_round_trip_and_projects_write_preserves_them() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());
        Json::check().unwrap();

        let notes = vec![Note {
            title: "n".to_string(),
            body: "# hi".to_string(),
            created_at: Some(1_700_000_000),
            updated_at: None,
        }];
        Json::write_notes(notes.clone());
        assert_eq!(Json::read_notes(), notes);

        // A projects write (the common mutation path) must not drop notes
        let projects = vec![Project {
            title: "p".to_string(),
            tasks: vec![make_task("t", TASK_STATUS_DONE, TASK_PRIORITY_NONE)],
        }];
        Json::write(projects.clone());
        assert_eq!(Json::read(), projects);
        assert_eq!(Json::read_notes(), notes);

        // ...and a notes write must not drop projects
        Json::write_notes(vec![]);
        assert_eq!(Json::read(), projects);
    }

    #[test]
    fn get_dir_path_honors_the_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());

        assert_eq!(Json::get_dir_path(), dir.path());
    }

    #[test]
    fn get_dir_path_falls_back_to_the_config_dir() {
        // Must not run in parallel with tests that set BASILK_CONFIG_DIR
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("BASILK_CONFIG_DIR");

        let path = Json::get_dir_path();

        assert!(path.ends_with(env!("CARGO_PKG_NAME")));
    }

    #[test]
    fn check_returns_false_when_the_data_file_is_already_current() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());
        Json::check().unwrap(); // creates the file at the current version

        // Second run reads the existing (current) file: no migrations
        let migrated = Json::check().unwrap();

        assert!(!migrated);
    }

    #[test]
    fn check_migrates_old_versioned_files() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());

        // Old format: a bare Vec<Project> stored in `<version>.json`
        let old_projects = vec![Project {
            title: "legacy".to_string(),
            tasks: vec![make_task("old task", TASK_STATUS_DONE, 3)],
        }];
        let old_path = Json::get_json_path(JSON_VERSIONS[0].to_string());
        fs::write(&old_path, to_string(&old_projects).unwrap()).unwrap();

        let migrated = Json::check().unwrap();

        assert!(migrated);
        assert!(!old_path.is_file(), "old versioned file is removed");

        let projects = Json::read();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "legacy");
        // Migrations reset priority to NONE and clear the note
        assert_eq!(projects[0].tasks[0].priority, TASK_PRIORITY_NONE);
        assert_eq!(projects[0].tasks[0].note, "");
    }
}
