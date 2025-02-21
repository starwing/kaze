use std::{
    any::{Any, TypeId},
    collections::HashMap,
    ffi::OsString,
};

use serde::{Deserialize, Serialize};

/// builder for ConfigMap
pub struct ConfigBuilder {
    content: toml::Value,
    map: HashMap<TypeId, Box<dyn Any>>,
    mergers: Vec<
        Box<
            dyn FnOnce(
                &mut clap::ArgMatches,
                &mut HashMap<TypeId, Box<dyn Any>>,
            ),
        >,
    >,
    cmd: clap::Command,
}

impl ConfigBuilder {
    /// create a new ConfigBuilder
    pub fn new(cmd: clap::Command, content: toml::Value) -> Self {
        Self {
            content,
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
        table_name: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // if the table is not present, use the default value
        let config = match self.content.get(table_name) {
            Some(table) => T::deserialize(table.clone())?,
            _ => default_from_clap(),
        };

        // box the config
        let config = Box::new(config);
        self.map.insert(TypeId::of::<T>(), config as Box<dyn Any>);
        self.mergers.push(Box::new(|matches, map| {
            if let Some(boxed) = map.get_mut(&TypeId::of::<T>()) {
                if let Some(config) = boxed.downcast_mut::<T>() {
                    config.update_from_arg_matches_mut(matches).unwrap();
                }
            }
        }));

        // update the args
        self.cmd = T::augment_args_for_update(self.cmd);

        Ok(self)
    }

    /// test the clap Args in builder valid.
    #[cfg(test)]
    pub fn debug_assert(self) -> Self {
        self.cmd.clone().debug_assert();
        self
    }

    /// build the ConfigMap
    pub fn build(self) -> ConfigMap {
        self.build_from(std::env::args_os())
    }

    /// build the ConfigMap from custom args
    pub fn build_from<I, T>(mut self, itr: I) -> ConfigMap
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        self.cmd.print_help().unwrap();
        let mut matches = self.cmd.get_matches_from(itr);

        // update the structs from the matches
        for merger in self.mergers.drain(..) {
            merger(&mut matches, &mut self.map);
        }

        // build the ConfigMap
        ConfigMap::new(self.map)
    }
}

fn default_from_clap<T: clap::Args>() -> T {
    let mut cmd = clap::Command::new("__dummy__");
    cmd = T::augment_args(cmd);
    let mut matches = cmd.get_matches_from(vec!["__dummy__"]);
    T::from_arg_matches_mut(&mut matches)
        .expect("Failed to get default config from clap")
}

/// ConfigMap stores the parsed config
pub struct ConfigMap {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl ConfigMap {
    fn new(map: HashMap<TypeId, Box<dyn Any>>) -> Self {
        Self { map }
    }

    /// get the config
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<T>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Serialize, clap::Args, Debug)]
    struct DatabaseConfig {
        #[arg(long, default_value = "localhost")]
        #[serde(default)]
        host: String,

        #[arg(long, short, default_value_t = 5432)]
        #[serde(default)]
        port: u16,
    }

    #[derive(Deserialize, Serialize, clap::Args, Debug)]
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
        let config_map = ConfigBuilder::new(clap::Command::new("test"), value)
            .add::<DatabaseConfig>("database")
            .unwrap()
            .add::<ServerConfig>("server")
            .unwrap()
            .debug_assert()
            .build_from(args);

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
