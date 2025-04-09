use std::path::{Path, PathBuf};

use anyhow::Context as _;
use tracing::warn;

use super::merge;

pub struct ConfigFileBuilder {
    names: Vec<String>,
    paths: Vec<PathBuf>,
    files: Vec<PathBuf>,
}

impl ConfigFileBuilder {
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            paths: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn default() -> Self {
        let mut name = "kaze".to_string();
        if let Ok(binpath) = std::env::current_exe() {
            if let Some(stem) = binpath.file_stem() {
                name = stem.to_string_lossy().to_string();
            }
        }
        Self::new()
            .add_homeconfig()
            .add_cwd()
            .add_binpath()
            .add_name(name + ".toml")
            .add_binname()
    }

    pub fn add_name(mut self, name: String) -> Self {
        let name =
            if let Some(idx) = self.names.iter().position(|n| n == &name) {
                self.names.remove(idx)
            } else {
                name
            };
        self.names.push(name);
        self
    }

    pub fn add_binname(self) -> Self {
        if let Ok(path) = std::env::current_exe() {
            if let Some(path) = path.file_name() {
                let name = path.to_string_lossy().to_string();
                return self.add_name(name);
            }
        }
        self
    }

    pub fn add_path(mut self, path: PathBuf) -> Self {
        if let Some(idx) = self.paths.iter().position(|p| p == &path) {
            let path = self.paths.remove(idx);
            self.paths.push(path);
        } else if path.is_dir() {
            self.paths.push(path);
        } else {
            warn!("Path is not a directory: {:?}", path)
        }
        self
    }

    pub fn add_file(mut self, file: PathBuf) -> Self {
        if file == PathBuf::from("") {
            return self;
        }
        if !file.is_file() {
            warn!("File not exists: {:?}", file);
            return self;
        }
        if let Some(idx) = self.files.iter().position(|p| p == &file) {
            self.files.remove(idx);
        }
        self.files.push(file);
        self
    }

    pub fn add_homeconfig(self) -> Self {
        if let Some(dir) = dirs::config_dir() {
            return self.add_path(dir);
        }
        self
    }

    pub fn add_cwd(self) -> Self {
        if let Ok(path) = std::env::current_dir() {
            return self.add_path(path);
        }
        self
    }

    pub fn add_binpath(self) -> Self {
        if let Ok(path) = std::env::current_exe() {
            let path = path.parent().unwrap_or(Path::new(""));
            return self.add_path(path.to_path_buf());
        }
        self
    }

    pub fn build(self) -> anyhow::Result<toml::Value> {
        let mut value = toml::Value::Table(toml::Table::new());
        for path in &self.paths {
            for name in &self.names {
                let path = path.join(name);
                if path.is_file() {
                    let content = std::fs::read_to_string(path)
                        .context("Failed to read file")?;
                    let current = toml::from_str(&content)
                        .context("Failed to parse file")?;
                    // TODO: record which config from which file
                    value = merge::merge(value, current)
                        .context("Failed to merge file")?;
                }
            }
        }
        for file in &self.files {
            let content = std::fs::read_to_string(file)
                .context("Failed to read file")?;
            let current =
                toml::from_str(&content).context("Failed to parse file")?;
            value = merge::merge(value, current)
                .context("Failed to merge file")?;
        }
        Ok(value)
    }
}
