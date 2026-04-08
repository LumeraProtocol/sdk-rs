use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lumera_sdk_rs::{
    keys::derive_signing_keys_from_mnemonic, CascadeConfig, CascadeSdk, RegisterTicketRequest,
};

fn extract_state(v: &serde_json::Value) -> String {
    for key in ["status", "state", "task_status"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            return s.to_ascii_lowercase();
        }
    }
    if let Some(task) = v.get("task") {
        for key in ["status", "state", "task_status"] {
            if let Some(s) = task.get(key).and_then(|x| x.as_str()) {
                return s.to_ascii_lowercase();
            }
        }
    }
    v.to_string().to_ascii_lowercase()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rest = std::env::var("LUMERA_REST").unwrap_or_else(|_| "http://127.0.0.1:1317".into());
    let rpc = std::env::var("LUMERA_RPC").unwrap_or_else(|_| "http://127.0.0.1:26657".into());
    let grpc = std::env::var("LUMERA_GRPC").unwrap_or_else(|_| "http://127.0.0.1:9090".into());
    let snapi = std::env::var("SNAPI_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let chain_id = std::env::var("LUMERA_CHAIN_ID").unwrap_or_else(|_| "lumera-devnet".into());
    let creator = std::env::var("LUMERA_CREATOR")
        .unwrap_or_else(|_| "lumera158ulqepc5wnlx04eqqs7hkhr9rs2een275qkpp".into());
    let mnemonic = std::env::var("LUMERA_MNEMONIC")?;

    let input_path =
        std::env::var("GOLDEN_INPUT").unwrap_or_else(|_| "/tmp/lumera-rs-golden-input.bin".into());
    let input_path = PathBuf::from(input_path);
    if !input_path.exists() {
        tokio::fs::write(&input_path, b"lumera sdk-rs golden test payload\n").await?;
    }

    let cfg = CascadeConfig {
        chain: lumera_sdk_rs::chain::ChainConfig {
            chain_id,
            grpc_endpoint: grpc,
            rpc_endpoint: rpc,
            rest_endpoint: rest,
            gas_price: "0.025ulume".into(),
        },
        snapi_base: snapi,
    };

    let sdk = CascadeSdk::new(cfg);
    let (chain_sk, arb_sk) =
        derive_signing_keys_from_mnemonic(&mnemonic).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let exp_secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + 172800;
    let expiration_time = exp_secs.to_string();

    let registered = sdk
        .register_ticket(
            &chain_sk,
            &arb_sk,
            &creator,
            &input_path,
            RegisterTicketRequest {
                file_name: input_path
                    .file_name()
                    .and_then(|x| x.to_str())
                    .unwrap_or("input.bin")
                    .to_string(),
                is_public: false,
                expiration_time,
            },
        )
        .await?;

    eprintln!("registered action_id={}", registered.action_id);

    let up_task = sdk
        .upload_via_snapi(
            &registered.action_id,
            &registered.auth_signature,
            &input_path,
        )
        .await?;
    eprintln!("upload task_id={}", up_task);

    for _ in 0..120 {
        let st = sdk.snapi.upload_status(&up_task).await?;
        let s = extract_state(&st);
        if s.contains("done") || s.contains("complete") || s.contains("success") {
            break;
        }
        if s.contains("fail") || s.contains("error") {
            return Err(anyhow::anyhow!("upload failed: {}", st));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    let down_task = sdk.request_download(&registered.action_id, &arb_sk).await?;
    eprintln!("download task_id={}", down_task);

    for _ in 0..120 {
        let st = sdk.snapi.download_status(&down_task).await?;
        let s = extract_state(&st);
        if s.contains("done") || s.contains("complete") || s.contains("success") {
            break;
        }
        if s.contains("fail") || s.contains("error") {
            return Err(anyhow::anyhow!("download failed: {}", st));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    let downloaded = sdk.snapi.download_file(&down_task).await?;
    let orig = tokio::fs::read(&input_path).await?;
    let h1 = blake3::hash(&orig);
    let h2 = blake3::hash(&downloaded);

    if h1 != h2 {
        return Err(anyhow::anyhow!(
            "hash mismatch: orig={} downloaded={}",
            h1.to_hex(),
            h2.to_hex()
        ));
    }

    println!(
        "GOLDEN_OK action_id={} upload_task={} download_task={} hash={}",
        registered.action_id,
        up_task,
        down_task,
        h1.to_hex()
    );
    Ok(())
}
