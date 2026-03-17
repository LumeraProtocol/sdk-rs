use lumera_sdk_rs::SdkSettings;
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
fn integration_load_json_config() {
    let _guard = env_lock().lock().unwrap();
    let _env_guard = EnvGuard::clear_for_test();

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("sdk.json");
    std::fs::write(
        &p,
        r#"{
  "chain_id": "lumera-devnet",
  "grpc_endpoint": "http://127.0.0.1:9090",
  "rpc_endpoint": "http://127.0.0.1:26657",
  "rest_endpoint": "http://127.0.0.1:1317",
  "gas_price": "0.025ulume",
  "snapi_base": "http://127.0.0.1:8080"
}"#,
    )
    .unwrap();

    let cfg = SdkSettings::from_file(&p).unwrap();
    assert_eq!(cfg.chain_id, "lumera-devnet");
    assert_eq!(cfg.snapi_base, "http://127.0.0.1:8080");
}
