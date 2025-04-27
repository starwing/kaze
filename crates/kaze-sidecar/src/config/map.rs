use std::{
    any::{Any, TypeId},
    collections::HashMap,
    ffi::OsString,
};

use kaze_plugin::serde::{Deserialize, Serialize};

type MergeTable = Vec<
    Box<
        dyn FnOnce(
            &toml::Value,
            &mut clap::ArgMatches,
            &mut HashMap<TypeId, Box<dyn Any>>,
        ) -> anyhow::Result<()>,
    >,
>;

/// builder for ConfigMap
pub struct ConfigBuilder {
    map: HashMap<TypeId, Box<dyn Any>>,
    mergers: MergeTable,
    cmd: clap::Command,
}

impl ConfigBuilder {
    /// create a new ConfigBuilder
    pub fn new(cmd: clap::Command) -> Self {
        Self {
            cmd,
            mergers: Vec::new(),
            map: HashMap::new(),
        }
    }

    /// add a config table to the builder
    pub fn add<
        T: for<'a> Deserialize<'a> + Serialize + clap::Args + 'static,
    >(
        mut self,
        name: impl ToString,
    ) -> Self {
        // add flags from T to the command
        self.cmd = T::augment_args_for_update(self.cmd);

        // add a merger for this config table
        let table_name = name.to_string();
        self.mergers.push(Box::new(
            |content, matches, map| -> anyhow::Result<()> {
                // if the table is not present, use the default value
                let config = match content.get(table_name) {
                    Some(table) => T::deserialize(table.clone())?,
                    _ => default_from_clap(),
                };
                // box the config
                let config = Box::new(config);
                if let Some(boxed) = map.get_mut(&TypeId::of::<T>()) {
                    if let Some(config) = boxed.downcast_mut::<T>() {
                        config.update_from_arg_matches_mut(matches).unwrap();
                    }
                }
                map.insert(TypeId::of::<T>(), config as Box<dyn Any>);
                Ok(())
            },
        ));

        self
    }

    /// test the clap Args in builder valid.
    pub fn debug_assert(self) -> Self {
        self.cmd.clone().debug_assert();
        self
    }

    /// get the ConfigMerger for the current command
    pub fn get_matches(self) -> ConfigMerger {
        Self::get_matches_from(self, std::env::args_os())
    }

    /// get the ConfigMerger for the current command
    pub fn get_matches_from<I, T>(self, itr: I) -> ConfigMerger
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        ConfigMerger::new(
            self.map,
            self.mergers,
            self.cmd.get_matches_from(itr),
        )
    }
}

fn default_from_clap<T: clap::Args>() -> T {
    let mut cmd = clap::Command::new("__dummy__");
    cmd = T::augment_args(cmd);
    let matches = cmd.get_matches_from(vec!["__dummy__"]);
    T::from_arg_matches(&matches)
        .expect("Failed to get default config from clap")
}

/// ConfigMerger is a helper struct for building a ConfigMap from a clap::ArgMatches
pub struct ConfigMerger {
    map: HashMap<TypeId, Box<dyn Any>>,
    mergers: MergeTable,
    matches: clap::ArgMatches,
}

impl ConfigMerger {
    /// create a new ConfigMerger
    fn new(
        map: HashMap<TypeId, Box<dyn Any>>,
        mergers: MergeTable,
        matches: clap::ArgMatches,
    ) -> Self {
        Self {
            map,
            mergers,
            matches,
        }
    }

    /// get the arg matches
    pub fn arg_matches(&self) -> &clap::ArgMatches {
        &self.matches
    }

    /// build the ConfigMap from custom args
    pub fn build(mut self, content: toml::Value) -> anyhow::Result<ConfigMap> {
        // update the structs from the matches
        for merger in self.mergers.drain(..) {
            merger(&content, &mut self.matches, &mut self.map)?;
        }

        // build the ConfigMap
        Ok(ConfigMap::new(self.map))
    }
}

/// ConfigMap stores the parsed config
pub struct ConfigMap {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl ConfigMap {
    fn new(map: HashMap<TypeId, Box<dyn Any>>) -> Self {
        Self { map }
    }

    /// add new options to map
    pub fn insert<T: Any>(&mut self, config: T) {
        self.map.insert(TypeId::of::<T>(), Box::new(config));
    }

    /// get the config
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<T>())
    }

    /// take the config
    pub fn take<T: Any>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|x| x.downcast::<T>().ok())
            .map(|e| *e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Serialize, clap::Args, Debug)]
    #[serde(crate = "kaze_plugin::serde")]
    struct DatabaseConfig {
        #[arg(long, default_value = "localhost")]
        #[serde(default)]
        host: String,

        #[arg(long, short, default_value_t = 5432)]
        #[serde(default)]
        port: u16,
    }

    #[derive(Deserialize, Serialize, clap::Args, Debug)]
    #[serde(crate = "kaze_plugin::serde")]
    struct ServerConfig {
        #[arg(long, short, default_value = "0.0.0.0:8080")]
        #[serde(default)]
        address: String,

        #[arg(long, short, default_value_t = 10)]
        #[serde(default)]
        timeout: u32,
    }

    #[test]
    fn test_config() {
        let value = toml::from_str(
            r#"
            [database]
            #host = "localhost"
            #port = 5432

            [server]
            address = "0.0.0.0:8080"
            timeout = 10
        "#,
        )
        .unwrap();
        let args = vec!["test", "--timeout", "20"];
        let config_map = ConfigBuilder::new(clap::Command::new("test"))
            .add::<DatabaseConfig>("database")
            .add::<ServerConfig>("server")
            .debug_assert()
            .get_matches_from(args)
            .build(value)
            .unwrap();

        assert_eq!(
            config_map.get::<DatabaseConfig>().unwrap().host,
            "localhost"
        );
        assert_eq!(
            config_map.get::<ServerConfig>().unwrap().address,
            "0.0.0.0:8080"
        );
        assert_eq!(config_map.get::<ServerConfig>().unwrap().timeout, 20);
    }
}
