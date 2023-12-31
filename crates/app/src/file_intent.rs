use crate::tvdb_cache::{EpisodeKey, TvdbCache};
use crate::file_descriptor::{get_descriptor, clean_episode_title, clean_series_name};
use enum_map;
use std::path::Path;
use serde;

#[derive(Debug, Eq, PartialEq, Copy, Clone, enum_map::Enum)]
pub enum Action {
    Rename,
    Complete,
    Ignore,
    Delete,
    Whitelist,
}

impl Action {
    pub fn iterator() -> std::slice::Iter<'static, Self> {
        static ACTIONS: [Action;5] = [
            Action::Rename,
            Action::Delete,
            Action::Ignore,
            Action::Whitelist,
            Action::Complete,
        ];
        ACTIONS.iter() 
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Action::Complete => "Complete",
            Action::Rename => "Rename",
            Action::Delete => "Delete",
            Action::Ignore => "Ignore",
            Action::Whitelist => "Whitelist",
        }
    }
}

#[derive(Debug)]
pub struct FileIntent {
    pub action: Action,
    pub dest: String,
    pub descriptor: Option<EpisodeKey>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct FilterRules {
    pub blacklist_extensions: Vec<String>,
    pub whitelist_folders: Vec<String>,
    pub whitelist_filenames: Vec<String>,
    pub whitelist_tags: Vec<String>,
}

pub fn get_file_intent(path_str: &str, rules: &FilterRules, cache: &TvdbCache) -> FileIntent {
    let mut intent = FileIntent {
        action: Action::Ignore,
        dest: "".to_string(),
        descriptor: None,
    };
    
    let path = Path::new(path_str);
    let extension = match path.extension() {
        Some(extension) => extension.to_string_lossy().to_string(),
        None => {
            intent.action = Action::Delete;
            return intent;
        },
    };
    let filename = match path.file_name() {
        Some(filename) => filename.to_string_lossy().to_string(),
        None => {
            intent.action = Action::Delete;
            return intent;
        },
    };
    
    if rules.blacklist_extensions.contains(&extension) {
        intent.action = Action::Delete;
        return intent;
    }

    for component in path.iter() {
        if let Some(folder) = component.to_str() {
            if rules.whitelist_folders.contains(&folder.to_string()) {
                intent.action = Action::Whitelist;
                return intent;
            }
        }
    }
    
    if rules.whitelist_filenames.contains(&filename) {
        intent.action = Action::Whitelist;
        return intent;
    }
    
    // get descriptor tag if possible
    let descriptor = match get_descriptor(filename.as_str()) {
        Some(descriptor) => descriptor,
        None => {
            intent.action = Action::Ignore;
            return intent;
        },
    };

    let episode_key = EpisodeKey { 
        season: descriptor.season, 
        episode: descriptor.episode,
    };
    intent.descriptor = Some(episode_key);

    // create new filename
    let new_episode_title = match cache.episode_cache.get(&episode_key) {
        None => "".to_string(),
        Some(index) => {
            let episode = &cache.episodes[*index];
            match &episode.name {
                None => "".to_string(),
                Some(name) => {
                    let clean_name = clean_episode_title(name.as_str());
                    if clean_name.is_empty() {
                        "".to_string()
                    } else {
                        format!("-{}", clean_name.as_str())
                    }
                },
            }
        },
    };
    let tags_string = descriptor.tags
        .iter()
        .filter(|tag| rules.whitelist_tags.contains(tag))
        .map(|tag| format!(".[{}]", tag.as_str()))
        .collect::<Vec<String>>()
        .join("");

    let new_filename = format!(
        "{}-S{:02}E{:02}{}{}.{}", 
        clean_series_name(cache.series.name.as_str()).as_str(), 
        descriptor.season, descriptor.episode, 
        new_episode_title.as_str(),
        tags_string.as_str(),
        extension.as_str(),
    );

    // check if new path is same as old path
    let new_folder = format!("Season {:02}", descriptor.season);
    let new_path = Path::new(new_folder.as_str()).join(new_filename.as_str());
    let new_path_str = new_path.to_string_lossy().to_string();
    let is_same_filepath = new_path == path;
    if is_same_filepath {
        intent.action = Action::Complete;
        return intent;
    }

    intent.action = Action::Rename;
    intent.dest = new_path_str;
    intent
}
