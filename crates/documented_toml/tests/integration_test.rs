use documented_toml::{DocumentedToml, ValueSerializer};
use serde::Serialize;

#[derive(DocumentedToml)]
struct Config {
    /// The name of the server
    name: String,

    /// The port to listen on
    port: u16,

    /// Advanced server settings
    advanced: AdvancedConfig,
}

#[derive(DocumentedToml)]
struct AdvancedConfig {
    /// Maximum number of connections
    max_connections: u32,

    /// Enable debug mode
    debug: bool,
}

#[test]
fn test_documented_toml() {
    let config = Config {
        name: "example".to_string(),
        port: 8080,
        advanced: AdvancedConfig {
            max_connections: 1000,
            debug: true,
        },
    };

    let table = config.as_toml();
    let doc: toml_edit::DocumentMut = table.as_table().unwrap().clone().into();
    let toml_str = doc.to_string();
    println!("toml:\n{}", toml_str);

    // Verify doc comments appear in output
    assert!(toml_str.contains("# The name of the server"));
    assert!(toml_str.contains("# The port to listen on"));

    // Verify nested structure
    assert!(toml_str.contains("[advanced]"));
    assert!(toml_str.contains("# Maximum number of connections"));
    assert!(toml_str.contains("# Enable debug mode"));

    // Verify values
    assert!(toml_str.contains("name = \"example\""));
    assert!(toml_str.contains("port = 8080"));
    assert!(toml_str.contains("max_connections = 1000"));
    assert!(toml_str.contains("debug = true"));
}

#[test]
fn test_primitive_types() {
    #[derive(DocumentedToml)]
    struct PrimitiveTypes {
        /// Integer 8-bit
        int8: i8,
        /// Integer 16-bit
        int16: i16,
        /// Integer 32-bit
        int32: i32,
        /// Integer 64-bit
        int64: i64,
        /// Unsigned 8-bit
        uint8: u8,
        /// Unsigned 16-bit
        uint16: u16,
        /// Unsigned 32-bit
        uint32: u32,
        /// Unsigned 64-bit
        uint64: u64,
        /// Float 32-bit
        float32: f32,
        /// Float 64-bit
        float64: f64,
        /// Boolean value
        boolean: bool,
        /// Character
        character: char,
    }

    let primitives = PrimitiveTypes {
        int8: -42,
        int16: -1000,
        int32: -100000,
        int64: -10000000000,
        uint8: 42,
        uint16: 1000,
        uint32: 100000,
        uint64: 10000000000,
        float32: 3.14,
        float64: 2.71828,
        boolean: true,
        character: 'A',
    };

    let table = primitives.as_toml();
    let doc: toml_edit::DocumentMut = table.as_table().unwrap().clone().into();
    let toml_str = doc.to_string();

    assert!(toml_str.contains("# Integer 8-bit"));
    assert!(toml_str.contains("int8 = -42"));
    assert!(toml_str.contains("# Float 32-bit"));
    assert!(toml_str.contains("float32 = 3.14"));
    assert!(toml_str.contains("# Character"));
    assert!(toml_str.contains("character = \"A\""));
}

#[test]
fn test_option_types() {
    #[derive(DocumentedToml)]
    struct OptionConfig {
        /// Optional string value (present)
        opt_string_some: Option<String>,
        /// Optional string value (absent)
        opt_string_none: Option<String>,
        /// Optional integer (present)
        opt_int_some: Option<i32>,
        /// Optional integer (absent)
        opt_int_none: Option<i32>,
    }

    let options = OptionConfig {
        opt_string_some: Some("hello".to_string()),
        opt_string_none: None,
        opt_int_some: Some(42),
        opt_int_none: None,
    };

    let table = options.as_toml();
    let doc: toml_edit::DocumentMut = table.as_table().unwrap().clone().into();
    let toml_str = doc.to_string();

    // Present values should appear in output
    assert!(toml_str.contains("# Optional string value (present)"));
    assert!(toml_str.contains("opt_string_some = \"hello\""));
    assert!(toml_str.contains("# Optional integer (present)"));
    assert!(toml_str.contains("opt_int_some = 42"));

    // None values should not appear in output
    assert!(!toml_str.contains("opt_string_none ="));
    assert!(!toml_str.contains("opt_int_none ="));
}

#[test]
fn test_vec_types() {
    #[derive(DocumentedToml)]
    struct VecConfig {
        /// List of strings
        string_list: Vec<String>,
        /// List of integers
        int_list: Vec<i32>,
        /// Empty list
        empty_list: Vec<String>,
    }

    let vectors = VecConfig {
        string_list: vec![
            "one".to_string(),
            "two".to_string(),
            "three".to_string(),
        ],
        int_list: vec![1, 2, 3, 4, 5],
        empty_list: vec![],
    };

    let table = vectors.as_toml();
    let doc: toml_edit::DocumentMut = table.as_table().unwrap().clone().into();
    let toml_str = doc.to_string();

    assert!(toml_str.contains("# List of strings"));
    assert!(toml_str.contains("string_list = [\"one\", \"two\", \"three\"]"));
    assert!(toml_str.contains("# List of integers"));
    assert!(toml_str.contains("int_list = [1, 2, 3, 4, 5]"));
    // Empty list shouldn't appear in output
    assert!(!toml_str.contains("empty_list ="));
}

#[test]
fn test_complex_nested_structure() {
    #[derive(DocumentedToml)]
    struct User {
        /// User identifier
        id: u64,
        /// User's full name
        name: String,
    }

    #[derive(DocumentedToml)]
    struct ComplexConfig {
        /// Application name
        name: String,
        /// List of supported features
        features: Vec<String>,
        /// Admin user details
        admin: User,
        /// List of regular users
        users: Vec<User>,
        /// Optional logging configuration
        logging: Option<LogConfig>,
    }

    #[derive(DocumentedToml)]
    struct LogConfig {
        /// Log level (debug, info, warn, error)
        level: String,
        /// Enable file logging
        file_enabled: bool,
        /// Log file path
        file_path: Option<String>,
    }

    let config = ComplexConfig {
        name: "TestApp".to_string(),
        features: vec![
            "auth".to_string(),
            "api".to_string(),
            "admin".to_string(),
        ],
        admin: User {
            id: 1,
            name: "Admin User".to_string(),
        },
        users: vec![
            User {
                id: 2,
                name: "User One".to_string(),
            },
            User {
                id: 3,
                name: "User Two".to_string(),
            },
        ],
        logging: Some(LogConfig {
            level: "info".to_string(),
            file_enabled: true,
            file_path: Some("/var/log/app.log".to_string()),
        }),
    };

    let table = config.as_toml();
    let doc: toml_edit::DocumentMut = table.as_table().unwrap().clone().into();
    let toml_str = doc.to_string();

    // Basic fields
    assert!(toml_str.contains("# Application name"));
    assert!(toml_str.contains("name = \"TestApp\""));

    // Array values
    assert!(toml_str.contains("features = [\"auth\", \"api\", \"admin\"]"));

    // Nested object
    assert!(toml_str.contains("[admin]"));
    assert!(toml_str.contains("# User identifier"));
    assert!(toml_str.contains("id = 1"));

    // Array of tables
    assert!(toml_str.contains("[[users]]"));
    assert!(toml_str.contains("id = 2"));
    assert!(toml_str.contains("id = 3"));

    // Optional nested object
    assert!(toml_str.contains("[logging]"));
    assert!(toml_str.contains("# Log level (debug, info, warn, error)"));
    assert!(toml_str.contains("level = \"info\""));
    assert!(toml_str.contains("file_path = \"/var/log/app.log\""));
}

// Module for custom serialization
mod custom_string_serializer {
    use serde::Serializer;
    use std::time::Duration;

    pub fn serialize<S>(
        duration: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}s", duration.as_secs()))
    }
}

#[test]
fn test_serde_with_attribute() {
    #[derive(Serialize, DocumentedToml)]
    struct ConfigWithCustomSerialization {
        /// Timeout duration
        #[serde(with = "custom_string_serializer")]
        timeout: std::time::Duration,
    }

    let config = ConfigWithCustomSerialization {
        timeout: std::time::Duration::from_secs(60),
    };

    let ser = ValueSerializer::new();
    let time: toml_edit::Value =
        custom_string_serializer::serialize(&config.timeout, ser).unwrap();
    println!("printed time: {}", time.to_string());

    let table = config.as_toml();
    let doc: toml_edit::DocumentMut = table.as_table().unwrap().clone().into();
    let toml_str = doc.to_string();
    println!("toml with serde(with):\n{}", toml_str);

    assert!(toml_str.contains("# Timeout duration"));
    assert!(toml_str.contains("timeout = \"60s\""));
}
