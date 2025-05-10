mod filefinder;
mod merge;

pub use filefinder::ConfigFileBuilder;

use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use clap::ArgMatches;
use kaze_plugin::serde::{Deserialize, Serialize};

/// builder for ConfigMap
pub struct ConfigBuilder {
    map: HashMap<TypeId, Box<dyn Any>>,
    table: toml::Table,
    mergers: Vec<Box<dyn Merger>>,
    cmd: clap::Command,
}

impl ConfigBuilder {
    /// create a new ConfigBuilder
    pub fn new(cmd: clap::Command) -> Self {
        Self {
            cmd,
            mergers: Vec::new(),
            map: HashMap::new(),
            table: toml::Table::new(),
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
        self.mergers
            .push(Box::new(MergerImpl::<T>::new(name.to_string())));

        self
    }

    /// get the command for the builder
    pub fn command(&self) -> &clap::Command {
        &self.cmd
    }

    /// build the ConfigMap from custom args
    pub fn build(
        mut self,
        matches: &mut ArgMatches,
        content: toml::Value,
    ) -> anyhow::Result<ConfigMap> {
        // update the structs from the matches
        for merger in self.mergers.drain(..) {
            merger.merge(&content, matches, &mut self.map, &mut self.table)?;
        }

        // build the ConfigMap
        Ok(ConfigMap::new(self.map, self.table))
    }
}

trait Merger {
    fn merge(
        &self,
        content: &toml::Value,
        matches: &mut clap::ArgMatches,
        map: &mut HashMap<TypeId, Box<dyn Any>>,
        table: &mut toml::Table,
    ) -> anyhow::Result<()>;
}

struct MergerImpl<T> {
    name: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> MergerImpl<T> {
    fn new(name: String) -> Self {
        Self {
            name,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Merger for MergerImpl<T>
where
    T: for<'a> Deserialize<'a> + Serialize + clap::Args + 'static,
{
    fn merge(
        &self,
        content: &toml::Value,
        matches: &mut clap::ArgMatches,
        map: &mut HashMap<TypeId, Box<dyn Any>>,
        table: &mut toml::Table,
    ) -> anyhow::Result<()> {
        // if the table is not present, use the default value
        let mut config = match content.get(&self.name) {
            Some(table) => Box::new(T::deserialize(table.clone())?),
            _ => map
                .remove(&TypeId::of::<T>())
                .map(|v| v.downcast().unwrap())
                .unwrap_or_else(|| Box::new(default_config())),
        };
        config.update_from_arg_matches_mut(matches)?;
        // update the value
        let v = toml::Value::try_from(config.as_ref())?;
        table.insert(self.name.clone(), v);
        // update from matches
        map.insert(TypeId::of::<T>(), config as Box<dyn Any>);
        Ok(())
    }
}

pub fn default_config<T: clap::Args>() -> T {
    let mut cmd = clap::Command::new("__dummy__");
    cmd = T::augment_args(cmd);
    let matches = cmd.get_matches_from(vec!["__dummy__"]);
    T::from_arg_matches(&matches)
        .expect("Failed to get default config from clap")
}

/// ConfigMap stores the parsed config
pub struct ConfigMap {
    map: HashMap<TypeId, Box<dyn Any>>,
    table: toml::Table,
}

impl ConfigMap {
    fn new(map: HashMap<TypeId, Box<dyn Any>>, table: toml::Table) -> Self {
        Self { map, table }
    }

    pub fn mock() -> Self {
        Self {
            map: HashMap::new(),
            table: toml::Table::new(),
        }
    }

    /// get the toml config result
    pub fn get_toml(&self) -> toml::Value {
        toml::Value::Table(self.table.clone())
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

    /// get the config mutably
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|x| x.downcast_mut::<T>())
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
        let itr = vec!["test", "--timeout", "20"];
        let cb = ConfigBuilder::new(clap::Command::new("test"))
            .add::<DatabaseConfig>("database")
            .add::<ServerConfig>("server");
        let cmd = cb.command();
        cmd.clone().debug_assert();
        let mut matches = cmd.clone().get_matches_from(itr);
        let config_map = cb.build(&mut matches, value).unwrap();

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
