use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use toml::Value;

/// The configuration table `package.metadata.grub-bootimage`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Extra arguments passed to QEMU in not testing mode.
    pub run_args: Option<Vec<String>>,
    /// Extra arguments passed to QEMU in testing mode.
    pub test_args: Option<Vec<String>>,
    /// The exit code considered a success in testing mode.
    pub test_success_exit_code: Option<i32>,
    /// The amount of time to wait before giving up on QEMU.
    pub test_timeout: u32,
}

impl Config {
    fn new() -> Config {
        Config {
            run_args: None,
            test_args: None,
            test_success_exit_code: None,
            test_timeout: 300,
        }
    }
}

pub fn read_config(cargo_toml: &PathBuf) -> Result<Config> {
    use std::{fs::File, io::Read};
    let content: Value = {
        let mut content = String::new();
        File::open(cargo_toml)
            .context("Failed to open Cargo.toml")?
            .read_to_string(&mut content)
            .context("Failed to read Cargo.toml")?;
        content
            .parse::<Value>()
            .context("Failed to parse Cargo.toml")?
    };

    let metadata = content
        .get("package")
        .and_then(|t| t.get("metadata"))
        .and_then(|t| t.get("grub-bootimage"));
    let metadata = match metadata {
        Some(metadata) => metadata
            .as_table()
            .ok_or_else(|| anyhow!("grub-bootimage: config invalid: {:?}", metadata))?,
        None => {
            return Ok(Config::new());
        }
    };

    let mut config = Config::new();

    for (key, value) in metadata {
        match (key.as_str(), value.clone()) {
            ("run-args", Value::Array(array)) => {
                config.run_args = Some(parse_config(array)?);
            }
            ("test-args", Value::Array(array)) => {
                config.test_args = Some(parse_config(array)?);
            }
            ("test-timeout", Value::Integer(timeout)) => {
                config.test_timeout = timeout as u32;
            }
            ("test-success-exit-code", Value::Integer(exit_code)) => {
                config.test_success_exit_code = Some(exit_code as i32);
            }
            (key, value) => {
                return Err(anyhow!(
                    "grub-bootimage: unexpected key `{}` with value `{}`",
                    key,
                    value
                ));
            }
        }
    }
    Ok(config)
}

fn parse_config(array: Vec<Value>) -> Result<Vec<String>> {
    let mut parsed = Vec::new();
    for val in array {
        match val {
            Value::String(s) => parsed.push(s),
            _ => return Err(anyhow!("config must be a list of strings")),
        }
    }
    Ok(parsed)
}
