use lumera_sdk_rs::SdkSettings;

#[test]
fn integration_load_json_config() {
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
