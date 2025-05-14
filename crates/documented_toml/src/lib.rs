//! A trait for converting structs to TOML values with documentation.
use std::path::PathBuf;

use toml_edit::{value, Array, ArrayOfTables, Item};

pub use documented_toml_derive::DocumentedToml;
pub use serde::ser;
pub use toml_edit::ser::ValueSerializer;
pub use toml_edit;

/// A trait for converting a struct to a TOML value with documentation.
///
/// This trait can be derived using `#[derive(DocumentedToml)]` for structs
/// that have doc comments on their fields.
pub trait DocumentedToml {
    /// Convert the implementing type to a TOML value.
    ///
    /// For structs, the resulting value will be a table that includes
    /// field documentation as comments in the TOML output.
    /// For primitive types, the result will be a direct value.
    fn as_toml(&self) -> toml_edit::Item;
}

// 为基础类型实现 DocumentedToml
macro_rules! impl_primitive {
    ($t:ty, $convert:expr) => {
        impl DocumentedToml for $t {
            fn as_toml(&self) -> toml_edit::Item {
                value($convert(*self))
            }
        }
    };
}

// 实现整数类型
impl_primitive!(i8, |v: i8| v as i64);
impl_primitive!(i16, |v: i16| v as i64);
impl_primitive!(i32, |v: i32| v as i64);
impl_primitive!(i64, |v: i64| v);
impl_primitive!(i128, |v: i128| v as i64);
impl_primitive!(isize, |v: isize| v as i64);

impl_primitive!(u8, |v: u8| v as i64);
impl_primitive!(u16, |v: u16| v as i64);
impl_primitive!(u32, |v: u32| v as i64);
impl_primitive!(u64, |v: u64| v as i64);
impl_primitive!(u128, |v: u128| v as i64);
impl_primitive!(usize, |v: usize| v as i64);

// 实现浮点类型
impl_primitive!(f32, |v: f32| v as f64);
impl_primitive!(f64, |v: f64| v);

// 实现布尔类型
impl_primitive!(bool, |v: bool| v);

// 实现字符类型
impl DocumentedToml for char {
    fn as_toml(&self) -> toml_edit::Item {
        value(self.to_string())
    }
}

impl DocumentedToml for String {
    fn as_toml(&self) -> toml_edit::Item {
        value(self.clone())
    }
}

impl DocumentedToml for PathBuf {
    fn as_toml(&self) -> toml_edit::Item {
        value(self.to_string_lossy().to_string())
    }
}

pub struct Wrap<T>(pub T);

impl<T: DocumentedToml> DocumentedToml for &&&Wrap<&T> {
    fn as_toml(&self) -> toml_edit::Item {
        self.0.as_toml()
    }
}

// 实现字符串类型
impl<T: ToString> DocumentedToml for &&Wrap<&T> {
    fn as_toml(&self) -> toml_edit::Item {
        value(self.0.to_string())
    }
}

impl<T: DocumentedToml> DocumentedToml for &Wrap<&Vec<T>> {
    fn as_toml(&self) -> toml_edit::Item {
        if self.0.is_empty() {
            return toml_edit::Item::None;
        }

        if self.0[0].as_toml().is_table() {
            let mut array_of_tables = ArrayOfTables::new();
            for item in self.0 {
                array_of_tables
                    .push(item.as_toml().as_table().unwrap().clone());
            }
            return Item::ArrayOfTables(array_of_tables);
        }

        let mut array = Array::new();
        for item in self.0 {
            array.push(item.as_toml().as_value().unwrap().clone());
        }
        value(array)
    }
}

impl<T: DocumentedToml> DocumentedToml for &Wrap<&Option<T>> {
    fn as_toml(&self) -> toml_edit::Item {
        match self.0 {
            Some(value) => value.as_toml(),
            None => toml_edit::Item::None,
        }
    }
}

impl<T: ToString> DocumentedToml for Wrap<&Vec<T>> {
    fn as_toml(&self) -> toml_edit::Item {
        if self.0.is_empty() {
            return toml_edit::Item::None;
        }

        let mut array = Array::new();
        for item in self.0 {
            array.push(value(&item.to_string()).into_value().unwrap());
        }
        value(array)
    }
}

impl<T: ToString> DocumentedToml for Wrap<&Option<T>> {
    fn as_toml(&self) -> toml_edit::Item {
        match self.0 {
            Some(value) => value.to_string().into(),
            None => toml_edit::Item::None,
        }
    }
}
