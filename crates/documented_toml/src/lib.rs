//! A trait for converting structs to TOML values with documentation.
use std::path::PathBuf;

use toml_edit::{value, Array, ArrayOfTables, Decor, Item, RawString};

pub use documented_toml_derive::DocumentedToml;
pub use toml_edit;
pub use toml_edit::ser::ValueSerializer;

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
            #[inline]
            fn as_toml(&self) -> toml_edit::Item {
                value($convert(*self))
            }
        }
    };
}

// implement DocumentedToml for primitive types
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

impl_primitive!(f32, |v: f32| v as f64);
impl_primitive!(f64, |v: f64| v);

impl_primitive!(bool, |v: bool| v);

impl DocumentedToml for char {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        value(self.to_string())
    }
}

// implement DocumentedToml for String and PathBuf
impl DocumentedToml for String {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        value(self.clone())
    }
}

impl DocumentedToml for PathBuf {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        value(self.to_string_lossy().to_string())
    }
}

// implement DocumentedToml for Vec<T> and Option<T>
impl<T: DocumentedToml> DocumentedToml for Vec<T> {
    fn as_toml(&self) -> toml_edit::Item {
        if self.is_empty() {
            return toml_edit::Item::None;
        }

        if self[0].as_toml().is_table() {
            let mut array_of_tables = ArrayOfTables::new();
            for item in self {
                array_of_tables
                    .push(item.as_toml().as_table().unwrap().clone());
            }
            return Item::ArrayOfTables(array_of_tables);
        }

        let mut array = Array::new();
        for item in self {
            array.push(item.as_toml().as_value().unwrap().clone());
        }
        value(array)
    }
}

impl<T: DocumentedToml> DocumentedToml for Option<T> {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        match self {
            Some(value) => value.as_toml(),
            None => toml_edit::Item::None,
        }
    }
}

// autoref specialization, see:
// https://lukaskalbertodt.github.io/2019/12/05/generalized-autoref-based-specialization.html
pub struct Wrap<T>(pub T);

// implement DocumentedToml for DocumentedToml
impl<T: DocumentedToml> DocumentedToml for &&&Wrap<&T> {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        self.0.as_toml()
    }
}

// implement DocumentedToml for ToString
impl<T: ToString> DocumentedToml for &&Wrap<&T> {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        value(self.0.to_string())
    }
}

impl<T: ToString> DocumentedToml for &Wrap<&Vec<T>> {
    #[inline]
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

impl<T: ToString> DocumentedToml for &Wrap<&Option<T>> {
    #[inline]
    fn as_toml(&self) -> toml_edit::Item {
        match self.0 {
            Some(value) => value.to_string().into(),
            None => toml_edit::Item::None,
        }
    }
}

#[inline]
pub fn format_docs(prefix: &str, doc: &str, old: &Decor) -> Decor {
    let mut new = Decor::default();
    format_docs_implace(prefix, doc, old, &mut new);
    new
}

pub fn format_docs_implace(
    prefix: &str,
    doc: &str,
    old: &Decor,
    new: &mut Decor,
) {
    let mut formatted = prefix.to_string();

    for line in doc.lines() {
        if line.trim().is_empty() {
            formatted.push_str("#\n");
            continue;
        }

        if let Some(indent) = old.prefix().and_then(RawString::as_str) {
            formatted.push_str(indent);
        }
        formatted.push_str("# ");
        formatted.push_str(line.trim());
        formatted.push('\n');
    }

    new.set_prefix(formatted);
}
