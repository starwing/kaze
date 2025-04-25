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
        let env = "KAZE_ENV".to_string();
        if let Ok(binpath) = std::env::current_exe() {
            if let Some(stem) = binpath.file_stem() {
                name = stem.to_string_lossy().to_string();
            }
        }
        Self::new()
            .add_homeconfig()
            .add_cwd()
            .add_binpath()
            .add_name(name.clone())
            .add_binname()
            .add_envname(name, env)
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

    pub fn add_envname(self, name: String, env: String) -> Self {
        if let Ok(env) = std::env::var(env) {
            return self.add_name(format!("{name}.{env}"));
        }
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
                let path = path.join(format!("{name}.toml"));
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

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs;
    use std::io::Write;

    use super::*;

    #[test]
    fn test_empty_builder() {
        let builder = ConfigFileBuilder::new();
        let result = builder.build();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), toml::Value::Table(toml::Table::new()));
    }

    #[test]
    fn test_add_file_nonexistent() {
        let file = PathBuf::from("/nonexistent/file.toml");
        let builder = ConfigFileBuilder::new().add_file(file);
        let result = builder.build();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), toml::Value::Table(toml::Table::new()));
    }

    #[test]
    fn test_add_file_empty_path() {
        let builder = ConfigFileBuilder::new().add_file(PathBuf::from(""));
        let result = builder.build();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), toml::Value::Table(toml::Table::new()));
    }

    #[test]
    fn test_add_file_with_content() -> anyhow::Result<()> {
        let temp_dir = temp_dir();
        let file_path = temp_dir.join("config.toml");
        let mut file = fs::File::create(&file_path)?;
        writeln!(file, "key = \"value\"")?;

        let builder = ConfigFileBuilder::new().add_file(file_path);
        let result = builder.build()?;

        let mut expected = toml::Table::new();
        expected.insert(
            "key".to_string(),
            toml::Value::String("value".to_string()),
        );

        assert_eq!(result, toml::Value::Table(expected));
        Ok(())
    }

    #[test]
    fn test_add_path_with_config_file() -> anyhow::Result<()> {
        let temp_dir = temp_dir();
        let config_path = temp_dir.join("test.toml");
        let mut file = fs::File::create(config_path)?;
        writeln!(file, "path_key = \"path_value\"")?;

        let builder = ConfigFileBuilder::new()
            .add_path(temp_dir.clone())
            .add_name("test".to_string());

        let result = builder.build()?;

        let mut expected = toml::Table::new();
        expected.insert(
            "path_key".to_string(),
            toml::Value::String("path_value".to_string()),
        );

        assert_eq!(result, toml::Value::Table(expected));
        Ok(())
    }

    #[test]
    fn test_config_merge() -> anyhow::Result<()> {
        let temp_dir = temp_dir();

        // Create first config file
        let config1_path = temp_dir.join("config1.toml");
        let mut file1 = fs::File::create(&config1_path)?;
        writeln!(file1, "key1 = \"value1\"\nshared = \"first\"")?;

        // Create second config file
        let config2_path = temp_dir.join("config2.toml");
        let mut file2 = fs::File::create(&config2_path)?;
        writeln!(file2, "key2 = \"value2\"\nshared = \"second\"")?;

        let builder = ConfigFileBuilder::new()
            .add_file(config1_path)
            .add_file(config2_path);

        let result = builder.build()?;

        let mut expected = toml::Table::new();
        expected.insert(
            "key1".to_string(),
            toml::Value::String("value1".to_string()),
        );
        expected.insert(
            "key2".to_string(),
            toml::Value::String("value2".to_string()),
        );
        expected.insert(
            "shared".to_string(),
            toml::Value::String("second".to_string()),
        ); // Second file overwrites

        assert_eq!(result, toml::Value::Table(expected));
        Ok(())
    }
}
