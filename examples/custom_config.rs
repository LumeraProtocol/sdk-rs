use lumera_sdk_rs::chain::ChainConfig;
use lumera_sdk_rs::{CascadeConfig, CascadeSdk};

fn main() {
    // Example for apps that configure endpoints explicitly in code.
    let chain_cfg = ChainConfig::new(
        "lumera-testnet-2",
        "https://grpc.testnet.lumera.io",
        "https://rpc.testnet.lumera.io",
        "https://lcd.testnet.lumera.io",
        "0.025ulume",
    );

    let cfg = CascadeConfig::new(chain_cfg, "https://snapi.testnet.lumera.io");
    let _sdk = CascadeSdk::new(cfg);

    println!("Constructed CascadeSdk with explicit custom config");
}
