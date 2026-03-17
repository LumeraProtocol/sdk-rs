use std::{env, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{cascade::CascadeConfig, chain::ChainConfig, error::SdkError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkSettings {
    pub chain_id: String,
    pub grpc_endpoint: String,
    pub rpc_endpoint: String,
    pub rest_endpoint: String,
    pub gas_price: String,
    pub snapi_base: String,
}

impl Default for SdkSettings {
    fn default() -> Self {
        Self {
            chain_id: "lumera-devnet".into(),
            grpc_endpoint: "http://127.0.0.1:9090".into(),
            rpc_endpoint: "http://127.0.0.1:26657".into(),
            rest_endpoint: "http://127.0.0.1:1317".into(),
            gas_price: "0.025ulume".into(),
            snapi_base: "http://127.0.0.1:8080".into(),
        }
    }
}

impl SdkSettings {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        cfg.apply_env_overrides();
        cfg
    }

    pub fn from_env_file(path: impl AsRef<Path>) -> Result<Self, SdkError> {
        dotenvy::from_path(path)
            .map_err(|e| SdkError::InvalidInput(format!("load env file: {e}")))?;
        Ok(Self::from_env())
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SdkError> {
        let path = path.as_ref();
        let body = fs::read_to_string(path).map_err(|e| {
            SdkError::InvalidInput(format!("read config file {}: {e}", path.display()))
        })?;

        let mut cfg = match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
        {
            "toml" => toml::from_str::<SdkSettings>(&body).map_err(|e| {
                SdkError::Serialization(format!("parse toml {}: {e}", path.display()))
            })?,
            "json" => serde_json::from_str::<SdkSettings>(&body).map_err(|e| {
                SdkError::Serialization(format!("parse json {}: {e}", path.display()))
            })?,
            ext => {
                return Err(SdkError::InvalidInput(format!(
                    "unsupported config extension '{}', use .toml or .json",
                    ext
                )))
            }
        };

        // Allow env to override file values for deployment-time flexibility.
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    pub fn to_cascade_config(&self) -> CascadeConfig {
        CascadeConfig {
            chain: ChainConfig {
                chain_id: self.chain_id.clone(),
                grpc_endpoint: self.grpc_endpoint.clone(),
                rpc_endpoint: self.rpc_endpoint.clone(),
                rest_endpoint: self.rest_endpoint.clone(),
                gas_price: self.gas_price.clone(),
            },
            snapi_base: self.snapi_base.clone(),
        }
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = env::var("LUMERA_CHAIN_ID") {
            self.chain_id = v;
        }
        if let Ok(v) = env::var("LUMERA_GRPC") {
            self.grpc_endpoint = v;
        }
        if let Ok(v) = env::var("LUMERA_RPC") {
            self.rpc_endpoint = v;
        }
        if let Ok(v) = env::var("LUMERA_REST") {
            self.rest_endpoint = v;
        }
        if let Ok(v) = env::var("LUMERA_GAS_PRICE") {
            self.gas_price = v;
        }
        if let Ok(v) = env::var("SNAPI_BASE") {
            self.snapi_base = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    const ENV_KEYS: [&str; 6] = [
        "LUMERA_CHAIN_ID",
        "LUMERA_GRPC",
        "LUMERA_RPC",
        "LUMERA_REST",
        "LUMERA_GAS_PRICE",
        "SNAPI_BASE",
    ];

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        prev: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn clear_for_test() -> Self {
            let prev = ENV_KEYS
                .iter()
                .map(|k| (k.to_string(), std::env::var(k).ok()))
                .collect::<Vec<_>>();
            for k in ENV_KEYS {
                std::env::remove_var(k);
            }
            Self { prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.prev {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    #[test]
    fn tdd_default_settings_to_cascade_cfg() {
        let s = SdkSettings::default();
        let c = s.to_cascade_config();
        assert_eq!(c.chain.chain_id, "lumera-devnet");
        assert_eq!(c.snapi_base, "http://127.0.0.1:8080");
    }

    #[test]
    fn tdd_load_toml_file() {
        let _guard = env_lock().lock().unwrap();
        let _env_guard = EnvGuard::clear_for_test();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sdk.toml");
        fs::write(
            &p,
            r#"
chain_id = "lumera-testnet-2"
grpc_endpoint = "https://grpc.testnet.lumera.io"
rpc_endpoint = "https://rpc.testnet.lumera.io"
rest_endpoint = "https://lcd.testnet.lumera.io"
gas_price = "0.025ulume"
snapi_base = "https://snapi.testnet.example"
"#,
        )
        .unwrap();

        let got = SdkSettings::from_file(&p).unwrap();
        assert_eq!(got.chain_id, "lumera-testnet-2");
        assert_eq!(got.snapi_base, "https://snapi.testnet.example");
    }
}
