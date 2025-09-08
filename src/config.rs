use bitflags::bitflags;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::fs;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub main: MainConfig,
    pub influxdb: InfluxDbConfig,
    pub web: WebConfig,
    pub teloxide: TeloxideConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InfluxDbConfig {
    pub host: String,
    pub org: String,
    pub token: String,
    pub bucket: String,
    #[serde(default = "default_node_name")]
    pub node_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MainConfig {
    pub work_mode: WorkMode,
    pub subscription_url: String,
    pub test_url: String,
    pub timeout: u64,
    pub concurrent: usize,
    pub verbose: bool,
    pub probe_interval: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeloxideConfig {
    pub token: String,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WorkMode: u8 {
        const CLI = 1;
        const WEB = 2;
        const INFLUXDB = 4;
        const TELOXIDE = 8;
    }
}

impl WorkMode {
    pub fn validate(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err("At least one work mode must be specified".to_string());
        }
        Ok(())
    }
}

impl Serialize for WorkMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut modes = Vec::new();
        if self.contains(WorkMode::WEB) {
            modes.push("Web");
        }
        if self.contains(WorkMode::INFLUXDB) {
            modes.push("InfluxDB");
        }
        if self.contains(WorkMode::TELOXIDE) {
            modes.push("Teloxide");
        }
        modes.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WorkMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WorkModeVisitor;

        impl<'de> Visitor<'de> for WorkModeVisitor {
            type Value = WorkMode;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an array of mode strings")
            }

            fn visit_str<E>(self, value: &str) -> Result<WorkMode, E>
            where
                E: de::Error,
            {
                match value {
                    "Web" => Ok(WorkMode::WEB),
                    "InfluxDB" => Ok(WorkMode::INFLUXDB),
                    "Teloxide" => Ok(WorkMode::TELOXIDE),
                    _ => Err(de::Error::unknown_variant(
                        value,
                        &["Web", "InfluxDB", "Teloxide"],
                    )),
                }
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<WorkMode, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut mode = WorkMode::empty();
                while let Some(value) = seq.next_element::<String>()? {
                    match value.as_str() {
                        "Web" => mode |= WorkMode::WEB,
                        "InfluxDB" => mode |= WorkMode::INFLUXDB,
                        "Teloxide" => mode |= WorkMode::TELOXIDE,
                        _ => {
                            return Err(de::Error::unknown_variant(
                                &value,
                                &["Web", "InfluxDB", "Teloxide"],
                            ));
                        }
                    }
                }
                if mode.is_empty() {
                    return Err(de::Error::custom("Empty mode array not allowed"));
                }
                Ok(mode)
            }
        }

        deserializer.deserialize_any(WorkModeVisitor)
    }
}

impl Config {
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn generate_default() -> Self {
        Self::default()
    }

    pub fn generate_default_toml() -> String {
        toml::to_string_pretty(&Self::default()).expect("serialize default config")
    }
}

impl Default for WorkMode {
    fn default() -> Self {
        WorkMode::CLI
    }
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            work_mode: WorkMode::WEB,
            subscription_url: "http://your_clash_sub".into(),
            test_url: "http://www.gstatic.com/generate_204".into(),
            timeout: 5,
            concurrent: 10,
            verbose: false,
            probe_interval: 30,
        }
    }
}

impl Default for InfluxDbConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:8086".into(),
            org: "example-org".into(),
            token: "REPLACE_WITH_TOKEN".into(),
            bucket: "example-bucket".into(),
            node_name: default_node_name(),
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
        }
    }
}

impl Default for TeloxideConfig {
    fn default() -> Self {
        Self {
            token: "REPLACE_WITH_TOKEN".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            main: MainConfig::default(),
            influxdb: InfluxDbConfig::default(),
            web: WebConfig::default(),
            teloxide: TeloxideConfig::default(),
        }
    }
}

fn default_node_name() -> String {
    "default".to_string()
}
