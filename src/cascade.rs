use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    chain::{ChainClient, RequestActionTxInput},
    error::SdkError,
    snapi::SnApiClient,
};

#[derive(Debug, Clone)]
pub struct CascadeConfig {
    pub chain: crate::chain::ChainConfig,
    pub snapi_base: String,
}

#[derive(Debug, Clone)]
pub struct RegisterTicketRequest {
    pub file_name: String,
    pub is_public: bool,
    pub expiration_time: String,
}

#[derive(Debug, Clone)]
pub struct RegisteredTicket {
    pub action_id: String,
    pub auth_signature: String,
    pub data_hash_b64: String,
    pub metadata_json: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexFile {
    pub layout_ids: Vec<String>,
    pub layout_signature: String,
    pub version: i32,
}

pub struct CascadeSdk {
    pub chain: ChainClient,
    pub snapi: SnApiClient,
}

impl CascadeSdk {
    pub fn new(cfg: CascadeConfig) -> Self {
        Self {
            chain: ChainClient::new(cfg.chain),
            snapi: SnApiClient::new(cfg.snapi_base),
        }
    }

    pub fn compute_data_hash_b64(file_bytes: &[u8]) -> String {
        let h = blake3::hash(file_bytes);
        STANDARD.encode(h.as_bytes())
    }

    pub fn create_layout_b64(file_path: &Path) -> Result<String, SdkError> {
        let cfg = rq_library::ProcessorConfig {
            symbol_size: 65535,
            redundancy_factor: 6,
            max_memory_mb: 4096,
            concurrency_limit: 1,
        };
        let processor = rq_library::RaptorQProcessor::new(cfg);
        let result = processor
            .create_metadata(
                file_path
                    .to_str()
                    .ok_or_else(|| SdkError::InvalidInput("non-utf8 file path".into()))?,
                "",
                0,
            )
            .map_err(|e| SdkError::Serialization(format!("rq create_metadata: {e}")))?;
        let layout = result
            .layout_content
            .ok_or_else(|| SdkError::Serialization("missing layout content".into()))?;
        let compact = serde_json::to_vec(
            &serde_json::from_str::<serde_json::Value>(&layout)
                .map_err(|e| SdkError::Serialization(e.to_string()))?,
        )
        .map_err(|e| SdkError::Serialization(e.to_string()))?;
        Ok(STANDARD.encode(compact))
    }

    pub fn generate_ids(base: &str, ic: u32, max: u32) -> Result<Vec<String>, SdkError> {
        let mut out = Vec::with_capacity(max as usize);
        for i in 0..max {
            let payload = format!("{}.{}", base, ic + i);
            let compressed = zstd::stream::encode_all(payload.as_bytes(), 3)
                .map_err(|e| SdkError::Serialization(e.to_string()))?;
            let hash = blake3::hash(&compressed);
            out.push(bs58::encode(hash.as_bytes()).into_string());
        }
        Ok(out)
    }

    pub fn build_index_file(layout_ids: Vec<String>, layout_signature: String) -> IndexFile {
        IndexFile {
            layout_ids,
            layout_signature,
            version: 1,
        }
    }

    pub fn canonical_json_bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, SdkError> {
        serde_json::to_vec(v).map_err(|e| SdkError::Serialization(e.to_string()))
    }

    pub fn random_ic(max: u32) -> u32 {
        rand::thread_rng().gen_range(0..max)
    }

    pub async fn register_ticket(
        &self,
        chain_signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        arbitrary_signing_key: &k256::ecdsa::SigningKey,
        creator_addr: &str,
        file_path: &Path,
        req: RegisterTicketRequest,
    ) -> Result<RegisteredTicket, SdkError> {
        let params = self.chain.get_action_params().await?;
        let max = params.max_raptor_q_symbols;
        let ic = Self::random_ic(max);

        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| SdkError::InvalidInput(format!("read file: {e}")))?;

        let data_hash = Self::compute_data_hash_b64(&file_bytes);
        let layout_b64 = Self::create_layout_b64(file_path)?;

        let layout_sig = crate::crypto::sign_raw_message_b64(arbitrary_signing_key, &layout_b64);
        let layout_ids = Self::generate_ids(&format!("{}.{}", layout_b64, layout_sig), ic, max)?;

        let index = Self::build_index_file(layout_ids, layout_sig);
        let index_bytes = Self::canonical_json_bytes(&index)?;
        let index_b64 = STANDARD.encode(&index_bytes);
        let index_sig = crate::crypto::sign_raw_message_b64(arbitrary_signing_key, &index_b64);
        let signatures = format!("{}.{}", index_b64, index_sig);

        let metadata = serde_json::json!({
            "data_hash": data_hash,
            "file_name": req.file_name,
            "rq_ids_ic": ic,
            "signatures": signatures,
            "public": req.is_public
        });
        let metadata_json = serde_json::to_string(&metadata).map_err(|e| SdkError::Serialization(e.to_string()))?;

        let file_size_kbs = ((file_bytes.len() as u64) + 1023) / 1024;
        let fee_amount = self.chain.get_action_fee_amount(file_size_kbs).await?;
        let price = if fee_amount.chars().all(|c| c.is_ascii_digit()) {
            format!("{}{}", fee_amount, params.base_action_fee_denom)
        } else {
            fee_amount
        };

        let tx_out = self
            .chain
            .register_action(
                chain_signing_key,
                RequestActionTxInput {
                    creator: creator_addr.to_string(),
                    action_type: "CASCADE".into(),
                    metadata: metadata_json.clone(),
                    price,
                    expiration_time: req.expiration_time,
                    file_size_kbs: file_size_kbs.to_string(),
                    app_pubkey: vec![],
                },
            )
            .await?;

        let auth_signature = crate::crypto::sign_raw_message_b64(arbitrary_signing_key, &data_hash);

        Ok(RegisteredTicket {
            action_id: tx_out.action_id,
            auth_signature,
            data_hash_b64: data_hash,
            metadata_json,
        })
    }

    pub async fn upload_via_snapi(
        &self,
        action_id: &str,
        signature: &str,
        file_path: &Path,
    ) -> Result<String, SdkError> {
        self.snapi.start_cascade(action_id, signature, file_path).await
    }

    pub async fn request_download(
        &self,
        action_id: &str,
        signing_key: &k256::ecdsa::SigningKey,
    ) -> Result<String, SdkError> {
        let sig = crate::crypto::sign_raw_message_b64(signing_key, action_id);
        self.snapi.request_download(action_id, &sig).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tdd_data_hash_base64() {
        let h = CascadeSdk::compute_data_hash_b64(b"abc");
        assert!(!h.is_empty());
    }

    #[test]
    fn tdd_generate_ids_count() {
        let ids = CascadeSdk::generate_ids("layout.sig", 10, 5).unwrap();
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn tdd_index_shape() {
        let idx = CascadeSdk::build_index_file(vec!["a".into()], "sig".into());
        let js = String::from_utf8(CascadeSdk::canonical_json_bytes(&idx).unwrap()).unwrap();
        assert_eq!(
            js,
            "{\"layout_ids\":[\"a\"],\"layout_signature\":\"sig\",\"version\":1}"
        );
    }
}
