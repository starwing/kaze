#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
use std::{
    any::{Any, TypeId},
    collections::HashMap, ffi::OsString,
};
use serde::{Deserialize, Serialize};
/// builder for ConfigMap
pub struct ConfigBuilder {
    content: toml::Value,
    map: HashMap<TypeId, Box<dyn Any>>,
    mergers: Vec<
        Box<dyn FnOnce(&mut clap::ArgMatches, &mut HashMap<TypeId, Box<dyn Any>>)>,
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
    pub fn add<T: Default + for<'a> Deserialize<'a> + Serialize + clap::Args + 'static>(
        mut self,
        table_name: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let config = if let Some(table) = self.content.get(table_name) {
            T::deserialize(table.clone())?
        } else {
            T::default()
        };
        let config = Box::new(config);
        self.map.insert(TypeId::of::<T>(), config as Box<dyn Any>);
        self.mergers
            .push(
                Box::new(|matches, map| {
                    if let Some(boxed) = map.get_mut(&TypeId::of::<T>()) {
                        if let Some(config) = boxed.downcast_mut::<T>() {
                            config.update_from_arg_matches_mut(matches).unwrap();
                        }
                    }
                }),
            );
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
        for merger in self.mergers.drain(..) {
            merger(&mut matches, &mut self.map);
        }
        ConfigMap::new(self.map)
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
    /// get the config
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.map.get(&TypeId::of::<T>()).and_then(|x| x.downcast_ref::<T>())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct DatabaseConfig {
        #[arg(long, default_value = "localhost")]
        host: String,
        #[arg(long, short)]
        port: u16,
    }
    #[automatically_derived]
    impl ::core::default::Default for DatabaseConfig {
        #[inline]
        fn default() -> DatabaseConfig {
            DatabaseConfig {
                host: ::core::default::Default::default(),
                port: ::core::default::Default::default(),
            }
        }
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for DatabaseConfig {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                #[automatically_derived]
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "host" => _serde::__private::Ok(__Field::__field0),
                            "port" => _serde::__private::Ok(__Field::__field1),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"host" => _serde::__private::Ok(__Field::__field0),
                            b"port" => _serde::__private::Ok(__Field::__field1),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                #[automatically_derived]
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<DatabaseConfig>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                #[automatically_derived]
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = DatabaseConfig;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct DatabaseConfig",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct DatabaseConfig with 2 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            u16,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct DatabaseConfig with 2 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(DatabaseConfig {
                            host: __field0,
                            port: __field1,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<u16> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("host"),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field("port"),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u16>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("host")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("port")?
                            }
                        };
                        _serde::__private::Ok(DatabaseConfig {
                            host: __field0,
                            port: __field1,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &["host", "port"];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "DatabaseConfig",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<DatabaseConfig>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for DatabaseConfig {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "DatabaseConfig",
                    false as usize + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "host",
                    &self.host,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "port",
                    &self.port,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[allow(
        dead_code,
        unreachable_code,
        unused_variables,
        unused_braces,
        unused_qualifications,
    )]
    #[allow(
        clippy::style,
        clippy::complexity,
        clippy::pedantic,
        clippy::restriction,
        clippy::perf,
        clippy::deprecated,
        clippy::nursery,
        clippy::cargo,
        clippy::suspicious_else_formatting,
        clippy::almost_swapped,
        clippy::redundant_locals,
    )]
    #[automatically_derived]
    impl clap::FromArgMatches for DatabaseConfig {
        fn from_arg_matches(
            __clap_arg_matches: &clap::ArgMatches,
        ) -> ::std::result::Result<Self, clap::Error> {
            Self::from_arg_matches_mut(&mut __clap_arg_matches.clone())
        }
        fn from_arg_matches_mut(
            __clap_arg_matches: &mut clap::ArgMatches,
        ) -> ::std::result::Result<Self, clap::Error> {
            #![allow(deprecated)]
            let v = DatabaseConfig {
                host: __clap_arg_matches
                    .remove_one::<String>("host")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: host",
                    ))?,
                port: __clap_arg_matches
                    .remove_one::<u16>("port")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: port",
                    ))?,
            };
            ::std::result::Result::Ok(v)
        }
        fn update_from_arg_matches(
            &mut self,
            __clap_arg_matches: &clap::ArgMatches,
        ) -> ::std::result::Result<(), clap::Error> {
            self.update_from_arg_matches_mut(&mut __clap_arg_matches.clone())
        }
        fn update_from_arg_matches_mut(
            &mut self,
            __clap_arg_matches: &mut clap::ArgMatches,
        ) -> ::std::result::Result<(), clap::Error> {
            #![allow(deprecated)]
            if __clap_arg_matches.contains_id("host") {
                #[allow(non_snake_case)]
                let host = &mut self.host;
                *host = __clap_arg_matches
                    .remove_one::<String>("host")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: host",
                    ))?;
            }
            if __clap_arg_matches.contains_id("port") {
                #[allow(non_snake_case)]
                let port = &mut self.port;
                *port = __clap_arg_matches
                    .remove_one::<u16>("port")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: port",
                    ))?;
            }
            ::std::result::Result::Ok(())
        }
    }
    #[allow(
        dead_code,
        unreachable_code,
        unused_variables,
        unused_braces,
        unused_qualifications,
    )]
    #[allow(
        clippy::style,
        clippy::complexity,
        clippy::pedantic,
        clippy::restriction,
        clippy::perf,
        clippy::deprecated,
        clippy::nursery,
        clippy::cargo,
        clippy::suspicious_else_formatting,
        clippy::almost_swapped,
        clippy::redundant_locals,
    )]
    #[automatically_derived]
    impl clap::Args for DatabaseConfig {
        fn group_id() -> Option<clap::Id> {
            Some(clap::Id::from("DatabaseConfig"))
        }
        fn augment_args<'b>(__clap_app: clap::Command) -> clap::Command {
            {
                let __clap_app = __clap_app
                    .group(
                        clap::ArgGroup::new("DatabaseConfig")
                            .multiple(true)
                            .args({
                                let members: [clap::Id; 2usize] = [
                                    clap::Id::from("host"),
                                    clap::Id::from("port"),
                                ];
                                members
                            }),
                    );
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("host")
                            .value_name("HOST")
                            .required(false && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    String,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("host").default_value("localhost");
                        let arg = arg;
                        arg
                    });
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("port")
                            .value_name("PORT")
                            .required(true && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    u16,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("port").short('p');
                        let arg = arg;
                        arg
                    });
                __clap_app
            }
        }
        fn augment_args_for_update<'b>(__clap_app: clap::Command) -> clap::Command {
            {
                let __clap_app = __clap_app
                    .group(
                        clap::ArgGroup::new("DatabaseConfig")
                            .multiple(true)
                            .args({
                                let members: [clap::Id; 2usize] = [
                                    clap::Id::from("host"),
                                    clap::Id::from("port"),
                                ];
                                members
                            }),
                    );
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("host")
                            .value_name("HOST")
                            .required(false && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    String,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("host").default_value("localhost");
                        let arg = arg.required(false);
                        arg
                    });
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("port")
                            .value_name("PORT")
                            .required(true && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    u16,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("port").short('p');
                        let arg = arg.required(false);
                        arg
                    });
                __clap_app
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for DatabaseConfig {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field2_finish(
                f,
                "DatabaseConfig",
                "host",
                &self.host,
                "port",
                &&self.port,
            )
        }
    }
    struct ServerConfig {
        #[arg(long, short)]
        address: String,
        #[arg(long, short)]
        timeout: u32,
    }
    #[automatically_derived]
    impl ::core::default::Default for ServerConfig {
        #[inline]
        fn default() -> ServerConfig {
            ServerConfig {
                address: ::core::default::Default::default(),
                timeout: ::core::default::Default::default(),
            }
        }
    }
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for ServerConfig {
            fn deserialize<__D>(
                __deserializer: __D,
            ) -> _serde::__private::Result<Self, __D::Error>
            where
                __D: _serde::Deserializer<'de>,
            {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field {
                    __field0,
                    __field1,
                    __ignore,
                }
                #[doc(hidden)]
                struct __FieldVisitor;
                #[automatically_derived]
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "field identifier",
                        )
                    }
                    fn visit_u64<__E>(
                        self,
                        __value: u64,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            0u64 => _serde::__private::Ok(__Field::__field0),
                            1u64 => _serde::__private::Ok(__Field::__field1),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(
                        self,
                        __value: &str,
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            "address" => _serde::__private::Ok(__Field::__field0),
                            "timeout" => _serde::__private::Ok(__Field::__field1),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_bytes<__E>(
                        self,
                        __value: &[u8],
                    ) -> _serde::__private::Result<Self::Value, __E>
                    where
                        __E: _serde::de::Error,
                    {
                        match __value {
                            b"address" => _serde::__private::Ok(__Field::__field0),
                            b"timeout" => _serde::__private::Ok(__Field::__field1),
                            _ => _serde::__private::Ok(__Field::__ignore),
                        }
                    }
                }
                #[automatically_derived]
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(
                        __deserializer: __D,
                    ) -> _serde::__private::Result<Self, __D::Error>
                    where
                        __D: _serde::Deserializer<'de>,
                    {
                        _serde::Deserializer::deserialize_identifier(
                            __deserializer,
                            __FieldVisitor,
                        )
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private::PhantomData<ServerConfig>,
                    lifetime: _serde::__private::PhantomData<&'de ()>,
                }
                #[automatically_derived]
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = ServerConfig;
                    fn expecting(
                        &self,
                        __formatter: &mut _serde::__private::Formatter,
                    ) -> _serde::__private::fmt::Result {
                        _serde::__private::Formatter::write_str(
                            __formatter,
                            "struct ServerConfig",
                        )
                    }
                    #[inline]
                    fn visit_seq<__A>(
                        self,
                        mut __seq: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::SeqAccess<'de>,
                    {
                        let __field0 = match _serde::de::SeqAccess::next_element::<
                            String,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        0usize,
                                        &"struct ServerConfig with 2 elements",
                                    ),
                                );
                            }
                        };
                        let __field1 = match _serde::de::SeqAccess::next_element::<
                            u32,
                        >(&mut __seq)? {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(
                                    _serde::de::Error::invalid_length(
                                        1usize,
                                        &"struct ServerConfig with 2 elements",
                                    ),
                                );
                            }
                        };
                        _serde::__private::Ok(ServerConfig {
                            address: __field0,
                            timeout: __field1,
                        })
                    }
                    #[inline]
                    fn visit_map<__A>(
                        self,
                        mut __map: __A,
                    ) -> _serde::__private::Result<Self::Value, __A::Error>
                    where
                        __A: _serde::de::MapAccess<'de>,
                    {
                        let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
                        let mut __field1: _serde::__private::Option<u32> = _serde::__private::None;
                        while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<
                            __Field,
                        >(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private::Option::is_some(&__field0) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "address",
                                            ),
                                        );
                                    }
                                    __field0 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<String>(&mut __map)?,
                                    );
                                }
                                __Field::__field1 => {
                                    if _serde::__private::Option::is_some(&__field1) {
                                        return _serde::__private::Err(
                                            <__A::Error as _serde::de::Error>::duplicate_field(
                                                "timeout",
                                            ),
                                        );
                                    }
                                    __field1 = _serde::__private::Some(
                                        _serde::de::MapAccess::next_value::<u32>(&mut __map)?,
                                    );
                                }
                                _ => {
                                    let _ = _serde::de::MapAccess::next_value::<
                                        _serde::de::IgnoredAny,
                                    >(&mut __map)?;
                                }
                            }
                        }
                        let __field0 = match __field0 {
                            _serde::__private::Some(__field0) => __field0,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("address")?
                            }
                        };
                        let __field1 = match __field1 {
                            _serde::__private::Some(__field1) => __field1,
                            _serde::__private::None => {
                                _serde::__private::de::missing_field("timeout")?
                            }
                        };
                        _serde::__private::Ok(ServerConfig {
                            address: __field0,
                            timeout: __field1,
                        })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &["address", "timeout"];
                _serde::Deserializer::deserialize_struct(
                    __deserializer,
                    "ServerConfig",
                    FIELDS,
                    __Visitor {
                        marker: _serde::__private::PhantomData::<ServerConfig>,
                        lifetime: _serde::__private::PhantomData,
                    },
                )
            }
        }
    };
    #[doc(hidden)]
    #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
    const _: () = {
        #[allow(unused_extern_crates, clippy::useless_attribute)]
        extern crate serde as _serde;
        #[automatically_derived]
        impl _serde::Serialize for ServerConfig {
            fn serialize<__S>(
                &self,
                __serializer: __S,
            ) -> _serde::__private::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let mut __serde_state = _serde::Serializer::serialize_struct(
                    __serializer,
                    "ServerConfig",
                    false as usize + 1 + 1,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "address",
                    &self.address,
                )?;
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    "timeout",
                    &self.timeout,
                )?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
    #[allow(
        dead_code,
        unreachable_code,
        unused_variables,
        unused_braces,
        unused_qualifications,
    )]
    #[allow(
        clippy::style,
        clippy::complexity,
        clippy::pedantic,
        clippy::restriction,
        clippy::perf,
        clippy::deprecated,
        clippy::nursery,
        clippy::cargo,
        clippy::suspicious_else_formatting,
        clippy::almost_swapped,
        clippy::redundant_locals,
    )]
    #[automatically_derived]
    impl clap::FromArgMatches for ServerConfig {
        fn from_arg_matches(
            __clap_arg_matches: &clap::ArgMatches,
        ) -> ::std::result::Result<Self, clap::Error> {
            Self::from_arg_matches_mut(&mut __clap_arg_matches.clone())
        }
        fn from_arg_matches_mut(
            __clap_arg_matches: &mut clap::ArgMatches,
        ) -> ::std::result::Result<Self, clap::Error> {
            #![allow(deprecated)]
            let v = ServerConfig {
                address: __clap_arg_matches
                    .remove_one::<String>("address")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: address",
                    ))?,
                timeout: __clap_arg_matches
                    .remove_one::<u32>("timeout")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: timeout",
                    ))?,
            };
            ::std::result::Result::Ok(v)
        }
        fn update_from_arg_matches(
            &mut self,
            __clap_arg_matches: &clap::ArgMatches,
        ) -> ::std::result::Result<(), clap::Error> {
            self.update_from_arg_matches_mut(&mut __clap_arg_matches.clone())
        }
        fn update_from_arg_matches_mut(
            &mut self,
            __clap_arg_matches: &mut clap::ArgMatches,
        ) -> ::std::result::Result<(), clap::Error> {
            #![allow(deprecated)]
            if __clap_arg_matches.contains_id("address") {
                #[allow(non_snake_case)]
                let address = &mut self.address;
                *address = __clap_arg_matches
                    .remove_one::<String>("address")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: address",
                    ))?;
            }
            if __clap_arg_matches.contains_id("timeout") {
                #[allow(non_snake_case)]
                let timeout = &mut self.timeout;
                *timeout = __clap_arg_matches
                    .remove_one::<u32>("timeout")
                    .ok_or_else(|| clap::Error::raw(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "The following required argument was not provided: timeout",
                    ))?;
            }
            ::std::result::Result::Ok(())
        }
    }
    #[allow(
        dead_code,
        unreachable_code,
        unused_variables,
        unused_braces,
        unused_qualifications,
    )]
    #[allow(
        clippy::style,
        clippy::complexity,
        clippy::pedantic,
        clippy::restriction,
        clippy::perf,
        clippy::deprecated,
        clippy::nursery,
        clippy::cargo,
        clippy::suspicious_else_formatting,
        clippy::almost_swapped,
        clippy::redundant_locals,
    )]
    #[automatically_derived]
    impl clap::Args for ServerConfig {
        fn group_id() -> Option<clap::Id> {
            Some(clap::Id::from("ServerConfig"))
        }
        fn augment_args<'b>(__clap_app: clap::Command) -> clap::Command {
            {
                let __clap_app = __clap_app
                    .group(
                        clap::ArgGroup::new("ServerConfig")
                            .multiple(true)
                            .args({
                                let members: [clap::Id; 2usize] = [
                                    clap::Id::from("address"),
                                    clap::Id::from("timeout"),
                                ];
                                members
                            }),
                    );
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("address")
                            .value_name("ADDRESS")
                            .required(true && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    String,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("address").short('a');
                        let arg = arg;
                        arg
                    });
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("timeout")
                            .value_name("TIMEOUT")
                            .required(true && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    u32,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("timeout").short('t');
                        let arg = arg;
                        arg
                    });
                __clap_app
            }
        }
        fn augment_args_for_update<'b>(__clap_app: clap::Command) -> clap::Command {
            {
                let __clap_app = __clap_app
                    .group(
                        clap::ArgGroup::new("ServerConfig")
                            .multiple(true)
                            .args({
                                let members: [clap::Id; 2usize] = [
                                    clap::Id::from("address"),
                                    clap::Id::from("timeout"),
                                ];
                                members
                            }),
                    );
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("address")
                            .value_name("ADDRESS")
                            .required(true && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    String,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("address").short('a');
                        let arg = arg.required(false);
                        arg
                    });
                let __clap_app = __clap_app
                    .arg({
                        #[allow(deprecated)]
                        let arg = clap::Arg::new("timeout")
                            .value_name("TIMEOUT")
                            .required(true && clap::ArgAction::Set.takes_values())
                            .value_parser({
                                use ::clap_builder::builder::impl_prelude::*;
                                let auto = ::clap_builder::builder::_infer_ValueParser_for::<
                                    u32,
                                >::new();
                                (&&&&&&auto).value_parser()
                            })
                            .action(clap::ArgAction::Set);
                        let arg = arg.long("timeout").short('t');
                        let arg = arg.required(false);
                        arg
                    });
                __clap_app
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for ServerConfig {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field2_finish(
                f,
                "ServerConfig",
                "address",
                &self.address,
                "timeout",
                &&self.timeout,
            )
        }
    }
    extern crate test;
    #[cfg(test)]
    #[rustc_test_marker = "tests::test_config"]
    #[doc(hidden)]
    pub const test_config: test::TestDescAndFn = test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("tests::test_config"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "crates/kaze-config/src/lib.rs",
            start_line: 138usize,
            start_col: 8usize,
            end_line: 138usize,
            end_col: 19usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::UnitTest,
        },
        testfn: test::StaticTestFn(
            #[coverage(off)]
            || test::assert_test_result(test_config()),
        ),
    };
    fn test_config() {
        let value = toml::from_str(
                r#"
            [database]
            host = "localhost"
            port = 5432

            [server]
            address = "0.0.0.0:8080"
            timeout = 10
        "#,
            )
            .unwrap();
        let args = <[_]>::into_vec(
            #[rustc_box]
            ::alloc::boxed::Box::new(["test", "--timeout", "20"]),
        );
        let config_map = ConfigBuilder::new(clap::Command::new("test"), value)
            .add::<DatabaseConfig>("database")
            .unwrap()
            .add::<ServerConfig>("server")
            .unwrap()
            .debug_assert()
            .build_from(args);
        match (&config_map.get::<DatabaseConfig>().unwrap().host, &"localhost") {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        };
        match (&config_map.get::<ServerConfig>().unwrap().address, &"0.0.0.0:8080") {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        };
        match (&config_map.get::<ServerConfig>().unwrap().timeout, &20) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        };
    }
}
#[rustc_main]
#[coverage(off)]
#[doc(hidden)]
pub fn main() -> () {
    extern crate test;
    test::test_main_static(&[&test_config])
}
