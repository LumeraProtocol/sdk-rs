use lumera_sdk_rs::{CascadeSdk, SdkSettings};

fn main() -> anyhow::Result<()> {
    // Loads defaults, then overrides from env vars:
    // LUMERA_CHAIN_ID, LUMERA_GRPC, LUMERA_RPC, LUMERA_REST, LUMERA_GAS_PRICE, SNAPI_BASE
    let settings = SdkSettings::from_env();
    let sdk = CascadeSdk::new(settings.to_cascade_config());

    println!("SDK ready");
    println!("chain_id={}", settings.chain_id);
    println!("rpc={}", settings.rpc_endpoint);
    println!("rest={}", settings.rest_endpoint);
    println!("snapi={}", settings.snapi_base);

    // Example query call to confirm endpoint wiring.
    // This will require reachable REST endpoint.
    let rt = tokio::runtime::Runtime::new()?;
    let params = rt.block_on(async { sdk.chain.get_action_params().await })?;
    println!(
        "action params: max_raptor_q_symbols={} base_fee_denom={}",
        params.max_raptor_q_symbols, params.base_action_fee_denom
    );

    Ok(())
}
