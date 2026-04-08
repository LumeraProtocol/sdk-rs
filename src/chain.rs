use crate::error::SdkError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cosmrs::{
    tx::{BodyBuilder, Fee, SignDoc, SignerInfo},
    Any, Coin,
};
use prost::Message;
use serde::Deserialize;
use tendermint_rpc::Client;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub chain_id: String,
    pub grpc_endpoint: String,
    pub rpc_endpoint: String,
    pub rest_endpoint: String,
    pub gas_price: String,
}

impl ChainConfig {
    pub fn new(
        chain_id: impl Into<String>,
        grpc_endpoint: impl Into<String>,
        rpc_endpoint: impl Into<String>,
        rest_endpoint: impl Into<String>,
        gas_price: impl Into<String>,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            grpc_endpoint: grpc_endpoint.into(),
            rpc_endpoint: rpc_endpoint.into(),
            rest_endpoint: rest_endpoint.into(),
            gas_price: gas_price.into(),
        }
    }

    pub fn with_chain_id(mut self, chain_id: impl Into<String>) -> Self {
        self.chain_id = chain_id.into();
        self
    }

    pub fn with_grpc_endpoint(mut self, grpc_endpoint: impl Into<String>) -> Self {
        self.grpc_endpoint = grpc_endpoint.into();
        self
    }

    pub fn with_rpc_endpoint(mut self, rpc_endpoint: impl Into<String>) -> Self {
        self.rpc_endpoint = rpc_endpoint.into();
        self
    }

    pub fn with_rest_endpoint(mut self, rest_endpoint: impl Into<String>) -> Self {
        self.rest_endpoint = rest_endpoint.into();
        self
    }

    pub fn with_gas_price(mut self, gas_price: impl Into<String>) -> Self {
        self.gas_price = gas_price.into();
        self
    }
}

#[derive(Debug, Clone)]
pub struct ActionParams {
    pub max_raptor_q_symbols: u32,
    pub svc_challenge_count: u32,
    pub svc_min_chunks_for_challenge: u32,
    pub base_action_fee_denom: String,
}

#[derive(Debug, Clone)]
pub struct TxResult {
    pub tx_hash: String,
    pub action_id: String,
}

#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub address: String,
    pub account_number: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone)]
pub struct TxConfirmationStatus {
    pub tx_hash: String,
    pub height: i64,
    pub code: u32,
    pub raw_log: String,
}

#[derive(Debug, Clone, Copy)]
pub enum BroadcastMode {
    Async,
    Sync,
    Commit,
}

#[derive(Debug, Clone)]
pub struct BroadcastTxResult {
    pub tx_hash: String,
    pub check_tx_code: Option<u32>,
    pub deliver_tx_code: Option<u32>,
    pub log: String,
}

#[derive(Debug, Clone)]
pub struct RequestActionSubmitResult {
    pub tx_hash: String,
    pub action_id: String,
}

#[derive(Debug, Clone)]
pub struct EventAttribute {
    pub event_type: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct RequestActionTxInput {
    pub creator: String,
    pub action_type: String,
    pub metadata: String,
    pub price: String,
    pub expiration_time: String,
    pub file_size_kbs: String,
    pub app_pubkey: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgRequestActionProto {
    #[prost(string, tag = "1")]
    pub creator: String,
    #[prost(string, tag = "2")]
    pub action_type: String,
    #[prost(string, tag = "3")]
    pub metadata: String,
    #[prost(string, tag = "4")]
    pub price: String,
    #[prost(string, tag = "5")]
    pub expiration_time: String,
    #[prost(string, tag = "6")]
    pub file_size_kbs: String,
    #[prost(bytes, tag = "7")]
    pub app_pubkey: Vec<u8>,
}

pub struct ChainClient {
    cfg: ChainConfig,
    http: reqwest::Client,
}

impl ChainClient {
    pub fn new(cfg: ChainConfig) -> Self {
        Self {
            cfg,
            http: reqwest::Client::new(),
        }
    }

    pub async fn get_action_params(&self) -> Result<ActionParams, SdkError> {
        let url = format!(
            "{}/LumeraProtocol/lumera/action/v1/params",
            self.cfg.rest_endpoint.trim_end_matches('/')
        );
        let v: serde_json::Value = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| SdkError::Serialization(e.to_string()))?;
        let p = v
            .get("params")
            .ok_or_else(|| SdkError::Serialization("missing params".into()))?;
        fn to_u32(v: Option<&serde_json::Value>, default: u32) -> u32 {
            match v {
                Some(x) if x.is_string() => {
                    x.as_str().and_then(|s| s.parse().ok()).unwrap_or(default)
                }
                Some(x) if x.is_u64() => x.as_u64().map(|n| n as u32).unwrap_or(default),
                _ => default,
            }
        }
        Ok(ActionParams {
            max_raptor_q_symbols: to_u32(p.get("max_raptor_q_symbols"), 50),
            svc_challenge_count: to_u32(p.get("svc_challenge_count"), 8),
            svc_min_chunks_for_challenge: to_u32(p.get("svc_min_chunks_for_challenge"), 4),
            base_action_fee_denom: p
                .get("base_action_fee")
                .and_then(|x| x.get("denom"))
                .and_then(|x| x.as_str())
                .unwrap_or("ulume")
                .to_string(),
        })
    }

    pub async fn get_action_fee_amount(&self, file_size_kbs: u64) -> Result<String, SdkError> {
        let url = format!(
            "{}/LumeraProtocol/lumera/action/v1/get_action_fee/{}",
            self.cfg.rest_endpoint.trim_end_matches('/'),
            file_size_kbs
        );
        let v: serde_json::Value = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| SdkError::Serialization(e.to_string()))?;
        Ok(v.get("amount")
            .and_then(|x| x.as_str())
            .unwrap_or("0")
            .to_string())
    }

    pub async fn register_action(
        &self,
        signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        tx: RequestActionTxInput,
    ) -> Result<TxResult, SdkError> {
        self.validate_signer_matches_creator(signing_key, &tx.creator)?;
        let account = self.get_base_account(&tx.creator).await?;

        let msg = MsgRequestActionProto {
            creator: tx.creator.clone(),
            action_type: tx.action_type,
            metadata: tx.metadata,
            price: tx.price,
            expiration_time: tx.expiration_time,
            file_size_kbs: tx.file_size_kbs,
            app_pubkey: tx.app_pubkey,
        };
        let mut msg_bytes = Vec::new();
        msg.encode(&mut msg_bytes)
            .map_err(|e| SdkError::Serialization(e.to_string()))?;
        let any = Any {
            type_url: "/lumera.action.v1.MsgRequestAction".to_string(),
            value: msg_bytes,
        };

        let tx_body = BodyBuilder::new().msg(any).finish();
        let gas_limit = 500_000u64;
        let fee_coin = self.calculate_fee_amount(gas_limit)?;
        let chain_id = self
            .cfg
            .chain_id
            .parse()
            .map_err(|e| SdkError::Chain(format!("chain-id: {e}")))?;
        let rpc = tendermint_rpc::HttpClient::new(self.cfg.rpc_endpoint.as_str())
            .map_err(|e| SdkError::Http(e.to_string()))?;

        let mut seq = account.sequence;
        let mut account_number = account.account_number;
        for _attempt in 0..3 {
            let auth = SignerInfo::single_direct(Some(signing_key.public_key()), seq)
                .auth_info(Fee::from_amount_and_gas(fee_coin.clone(), gas_limit));
            let sign_doc = SignDoc::new(&tx_body, &auth, &chain_id, account_number)
                .map_err(|e| SdkError::Chain(e.to_string()))?;
            let tx_raw = sign_doc
                .sign(signing_key)
                .map_err(|e| SdkError::Chain(e.to_string()))?;

            let rsp = tx_raw
                .broadcast_commit(&rpc)
                .await
                .map_err(|e| SdkError::Chain(e.to_string()))?;
            if rsp.check_tx.code.is_err() {
                let log = rsp.check_tx.log.to_string();
                if let Some(expected) = parse_expected_sequence(&log) {
                    if expected != seq {
                        if let Ok(refreshed) = self.get_base_account(&tx.creator).await {
                            seq = refreshed.sequence;
                            account_number = refreshed.account_number;
                        } else {
                            seq = expected;
                        }
                        continue;
                    }
                }
                return Err(SdkError::Chain(format!("check_tx: {}", log)));
            }
            if rsp.tx_result.code.is_err() {
                let log = rsp.tx_result.log.to_string();
                if let Some(expected) = parse_expected_sequence(&log) {
                    if expected != seq {
                        if let Ok(refreshed) = self.get_base_account(&tx.creator).await {
                            seq = refreshed.sequence;
                            account_number = refreshed.account_number;
                        } else {
                            seq = expected;
                        }
                        continue;
                    }
                }
                return Err(SdkError::Chain(format!("deliver_tx: {}", log)));
            }

            let action_id = extract_action_id_from_log(&rsp.tx_result.log)
                .or_else(|| extract_action_id_from_events_json(&rsp.tx_result.events))
                .ok_or_else(|| {
                    SdkError::Chain(format!(
                        "unable to extract action_id from tx logs/events; log={}",
                        rsp.tx_result.log
                    ))
                })?;

            return Ok(TxResult {
                tx_hash: rsp.hash.to_string(),
                action_id,
            });
        }

        Err(SdkError::Chain("sequence retry exhausted".into()))
    }

    pub async fn build_signed_tx(
        &self,
        signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        creator: &str,
        msgs: Vec<Any>,
        memo: impl Into<String>,
        gas_limit: u64,
    ) -> Result<cosmrs::tx::Raw, SdkError> {
        self.validate_signer_matches_creator(signing_key, creator)?;
        let account = self.get_base_account(creator).await?;
        let fee_coin = self.calculate_fee_amount(gas_limit)?;
        let chain_id = self
            .cfg
            .chain_id
            .parse()
            .map_err(|e| SdkError::Chain(format!("chain-id: {e}")))?;

        let mut txb = BodyBuilder::new();
        txb.msgs(msgs).memo(memo.into());
        let tx_body = txb.finish();

        let auth = SignerInfo::single_direct(Some(signing_key.public_key()), account.sequence)
            .auth_info(Fee::from_amount_and_gas(fee_coin, gas_limit));
        let sign_doc = SignDoc::new(&tx_body, &auth, &chain_id, account.account_number)
            .map_err(|e| SdkError::Chain(e.to_string()))?;
        sign_doc
            .sign(signing_key)
            .map_err(|e| SdkError::Chain(e.to_string()))
    }

    pub async fn broadcast_signed_tx(
        &self,
        tx_raw: &cosmrs::tx::Raw,
        mode: BroadcastMode,
    ) -> Result<BroadcastTxResult, SdkError> {
        let rpc = tendermint_rpc::HttpClient::new(self.cfg.rpc_endpoint.as_str())
            .map_err(|e| SdkError::Http(e.to_string()))?;
        let tx_bytes = tx_raw
            .to_bytes()
            .map_err(|e| SdkError::Serialization(e.to_string()))?;

        match mode {
            BroadcastMode::Async => {
                let rsp = rpc
                    .broadcast_tx_async(tx_bytes)
                    .await
                    .map_err(|e| SdkError::Chain(e.to_string()))?;
                Ok(BroadcastTxResult {
                    tx_hash: rsp.hash.to_string(),
                    check_tx_code: None,
                    deliver_tx_code: None,
                    log: String::new(),
                })
            }
            BroadcastMode::Sync => {
                let rsp = rpc
                    .broadcast_tx_sync(tx_bytes)
                    .await
                    .map_err(|e| SdkError::Chain(e.to_string()))?;
                Ok(BroadcastTxResult {
                    tx_hash: rsp.hash.to_string(),
                    check_tx_code: Some(rsp.code.value()),
                    deliver_tx_code: None,
                    log: rsp.log.to_string(),
                })
            }
            BroadcastMode::Commit => {
                let rsp = rpc
                    .broadcast_tx_commit(tx_bytes)
                    .await
                    .map_err(|e| SdkError::Chain(e.to_string()))?;
                Ok(BroadcastTxResult {
                    tx_hash: rsp.hash.to_string(),
                    check_tx_code: Some(rsp.check_tx.code.value()),
                    deliver_tx_code: Some(rsp.tx_result.code.value()),
                    log: rsp.tx_result.log.to_string(),
                })
            }
        }
    }

    pub async fn send_any_msgs(
        &self,
        signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        creator: &str,
        msgs: Vec<Any>,
        memo: impl Into<String>,
        gas_limit: u64,
        mode: BroadcastMode,
    ) -> Result<BroadcastTxResult, SdkError> {
        let tx_raw = self
            .build_signed_tx(signing_key, creator, msgs, memo, gas_limit)
            .await?;
        self.broadcast_signed_tx(&tx_raw, mode).await
    }

    pub async fn simulate_gas_for_tx(&self, tx_raw: &cosmrs::tx::Raw) -> Result<u64, SdkError> {
        let tx_bytes = tx_raw
            .to_bytes()
            .map_err(|e| SdkError::Serialization(e.to_string()))?;
        let tx_bytes_b64 = STANDARD.encode(tx_bytes);

        let url = format!(
            "{}/cosmos/tx/v1beta1/simulate",
            self.cfg.rest_endpoint.trim_end_matches('/')
        );

        let v: serde_json::Value = self
            .http
            .post(url)
            .json(&serde_json::json!({"tx_bytes": tx_bytes_b64}))
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| SdkError::Serialization(e.to_string()))?;

        let gas_used = v
            .get("gas_info")
            .and_then(|g| g.get("gas_used"))
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| x.as_u64())
            })
            .ok_or_else(|| {
                SdkError::Serialization("missing gas_info.gas_used in simulate response".into())
            })?;

        Ok(gas_used)
    }

    pub async fn build_signed_tx_with_simulation(
        &self,
        signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        creator: &str,
        msgs: Vec<Any>,
        memo: impl Into<String>,
        fallback_gas_limit: u64,
        gas_adjustment: f64,
    ) -> Result<(cosmrs::tx::Raw, u64), SdkError> {
        let memo = memo.into();
        let first = self
            .build_signed_tx(
                signing_key,
                creator,
                msgs.clone(),
                memo.clone(),
                fallback_gas_limit,
            )
            .await?;

        let simulated = self
            .simulate_gas_for_tx(&first)
            .await
            .unwrap_or(fallback_gas_limit);
        let adjustment = if gas_adjustment <= 0.0 {
            1.3
        } else {
            gas_adjustment
        };
        let adjusted = ((simulated as f64) * adjustment).ceil() as u64;
        let gas_limit = adjusted.max(1);

        let final_tx = self
            .build_signed_tx(signing_key, creator, msgs, memo, gas_limit)
            .await?;
        Ok((final_tx, gas_limit))
    }

    pub async fn request_action_tx(
        &self,
        signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        tx: RequestActionTxInput,
        memo: impl Into<String>,
    ) -> Result<RequestActionSubmitResult, SdkError> {
        self.validate_signer_matches_creator(signing_key, &tx.creator)?;

        let mut msg_bytes = Vec::new();
        MsgRequestActionProto {
            creator: tx.creator.clone(),
            action_type: tx.action_type.clone(),
            metadata: tx.metadata.clone(),
            price: tx.price.clone(),
            expiration_time: tx.expiration_time.clone(),
            file_size_kbs: tx.file_size_kbs.clone(),
            app_pubkey: tx.app_pubkey.clone(),
        }
        .encode(&mut msg_bytes)
        .map_err(|e| SdkError::Serialization(e.to_string()))?;

        let any = Any {
            type_url: "/lumera.action.v1.MsgRequestAction".to_string(),
            value: msg_bytes,
        };

        let memo = memo.into();
        for _attempt in 0..3 {
            let (tx_raw, _gas) = self
                .build_signed_tx_with_simulation(
                    signing_key,
                    &tx.creator,
                    vec![any.clone()],
                    memo.clone(),
                    500_000,
                    1.3,
                )
                .await?;

            let broadcast = self
                .broadcast_signed_tx(&tx_raw, BroadcastMode::Commit)
                .await?;

            if broadcast.check_tx_code.unwrap_or_default() != 0 {
                if parse_expected_sequence(&broadcast.log).is_some() {
                    continue;
                }
                return Err(SdkError::Chain(format!(
                    "check_tx failed: {}",
                    broadcast.log
                )));
            }
            if broadcast.deliver_tx_code.unwrap_or_default() != 0 {
                if parse_expected_sequence(&broadcast.log).is_some() {
                    continue;
                }
                return Err(SdkError::Chain(format!(
                    "deliver_tx failed: {}",
                    broadcast.log
                )));
            }

            let action_id = extract_action_id_from_log(&broadcast.log).ok_or_else(|| {
                SdkError::Chain(format!(
                    "unable to extract action_id from commit log: {}",
                    broadcast.log
                ))
            })?;

            return Ok(RequestActionSubmitResult {
                tx_hash: broadcast.tx_hash,
                action_id,
            });
        }

        Err(SdkError::Chain("sequence retry exhausted".into()))
    }

    pub async fn get_account_info(&self, address: &str) -> Result<AccountInfo, SdkError> {
        let base = self.get_base_account(address).await?;
        Ok(AccountInfo {
            address: address.to_string(),
            account_number: base.account_number,
            sequence: base.sequence,
        })
    }

    pub fn calculate_fee_amount(&self, gas_limit: u64) -> Result<Coin, SdkError> {
        let gas_price = self.cfg.gas_price.trim();
        let split_at = gas_price
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .ok_or_else(|| {
                SdkError::InvalidInput(format!(
                    "invalid gas_price '{}': expected e.g. 0.025ulume",
                    gas_price
                ))
            })?;
        let (amount_str, denom_str) = gas_price.split_at(split_at);
        let amount = amount_str
            .parse::<f64>()
            .map_err(|e| SdkError::InvalidInput(format!("gas_price amount parse error: {e}")))?;
        let fee = (amount * gas_limit as f64).ceil() as u128;

        Ok(Coin {
            amount: fee,
            denom: denom_str
                .parse()
                .map_err(|e| SdkError::Chain(format!("fee denom parse: {e}")))?,
        })
    }

    pub async fn wait_for_tx_confirmation(
        &self,
        tx_hash: &str,
        timeout_secs: u64,
    ) -> Result<TxConfirmationStatus, SdkError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            if let Some(status) = self.get_tx(tx_hash).await? {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                return Err(SdkError::Chain(format!(
                    "timed out waiting for tx confirmation: {}",
                    tx_hash
                )));
            }
            sleep(Duration::from_secs(2)).await;
        }
    }

    pub async fn wait_for_event_attribute(
        &self,
        tx_hash: &str,
        event_type: &str,
        attr_key: &str,
        timeout_secs: u64,
    ) -> Result<String, SdkError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            if let Some(attrs) = self.get_tx_event_attributes(tx_hash).await? {
                if let Some(found) = attrs
                    .iter()
                    .find(|a| a.event_type == event_type && a.key == attr_key)
                {
                    return Ok(found.value.clone());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(SdkError::Chain(format!(
                    "timed out waiting for tx event attribute: tx_hash={}, event_type={}, key={}",
                    tx_hash, event_type, attr_key
                )));
            }
            sleep(Duration::from_secs(2)).await;
        }
    }

    pub async fn get_tx(&self, tx_hash: &str) -> Result<Option<TxConfirmationStatus>, SdkError> {
        let tx_resp = self.get_tx_response_json(tx_hash).await?;
        let Some(tx_resp) = tx_resp else {
            return Ok(None);
        };

        let height = tx_resp
            .get("height")
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .or_else(|| x.as_i64())
            })
            .unwrap_or_default();
        let code = tx_resp
            .get("code")
            .and_then(|x| x.as_u64())
            .unwrap_or_default() as u32;
        let raw_log = tx_resp
            .get("raw_log")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();

        Ok(Some(TxConfirmationStatus {
            tx_hash: tx_hash.to_string(),
            height,
            code,
            raw_log,
        }))
    }

    pub async fn get_tx_event_attributes(
        &self,
        tx_hash: &str,
    ) -> Result<Option<Vec<EventAttribute>>, SdkError> {
        let tx_resp = self.get_tx_response_json(tx_hash).await?;
        let Some(tx_resp) = tx_resp else {
            return Ok(None);
        };

        Ok(Some(extract_event_attributes_from_tx_response(&tx_resp)))
    }

    async fn get_tx_response_json(
        &self,
        tx_hash: &str,
    ) -> Result<Option<serde_json::Value>, SdkError> {
        let url = format!(
            "{}/cosmos/tx/v1beta1/txs/{}",
            self.cfg.rest_endpoint.trim_end_matches('/'),
            tx_hash
        );

        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SdkError::Serialization(e.to_string()))?;

        let tx_resp = v
            .get("tx_response")
            .ok_or_else(|| SdkError::Serialization("missing tx_response".into()))?
            .clone();

        Ok(Some(tx_resp))
    }

    fn validate_signer_matches_creator(
        &self,
        signing_key: &cosmrs::crypto::secp256k1::SigningKey,
        creator: &str,
    ) -> Result<(), SdkError> {
        let (hrp, _) = creator.rsplit_once('1').ok_or_else(|| {
            SdkError::InvalidInput(format!("invalid creator bech32 address: {}", creator))
        })?;

        let derived = signing_key
            .public_key()
            .account_id(hrp)
            .map_err(|e| SdkError::Crypto(format!("derive signer account_id: {e}")))?
            .to_string();

        if derived != creator {
            return Err(SdkError::InvalidInput(format!(
                "creator address does not match signing key: creator={}, signer={}",
                creator, derived
            )));
        }
        Ok(())
    }

    async fn get_base_account(&self, address: &str) -> Result<BaseAccount, SdkError> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.cfg.rest_endpoint.trim_end_matches('/'),
            address
        );
        let v: serde_json::Value = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?
            .json()
            .await
            .map_err(|e| SdkError::Serialization(e.to_string()))?;
        let acc = v
            .get("account")
            .ok_or_else(|| SdkError::Serialization("missing account".into()))?;
        let base = acc.get("base_account").unwrap_or(acc);

        let account_number = base
            .get("account_number")
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| x.as_u64())
            })
            .ok_or_else(|| SdkError::Serialization("missing account_number".into()))?;
        let sequence = base
            .get("sequence")
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| x.as_u64())
            })
            .ok_or_else(|| SdkError::Serialization("missing sequence".into()))?;

        Ok(BaseAccount {
            account_number,
            sequence,
        })
    }
}

#[derive(Debug, Deserialize)]
struct BaseAccount {
    pub account_number: u64,
    pub sequence: u64,
}

pub fn extract_action_id_from_log(log: &str) -> Option<String> {
    extract_event_attribute_from_log(log, "action_registered", "action_id")
}

pub fn extract_event_attribute_from_log(
    log: &str,
    event_type: &str,
    attr_key: &str,
) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(log).ok()?;
    let arr = v.as_array()?;
    for item in arr {
        for e in item.get("events")?.as_array()? {
            if e.get("type").and_then(|x| x.as_str()) != Some(event_type) {
                continue;
            }
            for attr in e.get("attributes")?.as_array()? {
                let key = attr.get("key")?.as_str()?;
                let val = attr.get("value")?.as_str()?;
                if key == attr_key {
                    return Some(val.to_string());
                }
                let kb = STANDARD
                    .decode(key)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
                let vb = STANDARD
                    .decode(val)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
                if kb.as_deref() == Some(attr_key) {
                    return vb;
                }
            }
        }
    }
    None
}

fn extract_event_attributes_from_tx_response(
    tx_response: &serde_json::Value,
) -> Vec<EventAttribute> {
    let mut out = Vec::new();

    // Preferred path for REST /cosmos/tx/v1beta1/txs: tx_response.logs[].events[].attributes[]
    if let Some(logs) = tx_response.get("logs").and_then(|x| x.as_array()) {
        for log in logs {
            if let Some(events) = log.get("events").and_then(|x| x.as_array()) {
                for e in events {
                    let event_type = e
                        .get("type")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if let Some(attrs) = e.get("attributes").and_then(|x| x.as_array()) {
                        for a in attrs {
                            let key = a
                                .get("key")
                                .and_then(|x| x.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let value = a
                                .get("value")
                                .and_then(|x| x.as_str())
                                .unwrap_or_default()
                                .to_string();
                            out.push(EventAttribute {
                                event_type: event_type.clone(),
                                key,
                                value,
                            });
                        }
                    }
                }
            }
        }
    }

    // Fallback to RPC-style events[].attributes[] shape if logs are unavailable.
    if out.is_empty() {
        if let Some(events) = tx_response.get("events").and_then(|x| x.as_array()) {
            for e in events {
                let event_type = e
                    .get("kind")
                    .or_else(|| e.get("type"))
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string();
                if let Some(attrs) = e.get("attributes").and_then(|x| x.as_array()) {
                    for a in attrs {
                        let key = a
                            .get("key")
                            .and_then(|x| x.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let value = a
                            .get("value")
                            .and_then(|x| x.as_str())
                            .unwrap_or_default()
                            .to_string();
                        out.push(EventAttribute {
                            event_type: event_type.clone(),
                            key,
                            value,
                        });
                    }
                }
            }
        }
    }

    out
}

fn parse_expected_sequence(log: &str) -> Option<u64> {
    // Typical chain error: "account sequence mismatch, expected 14, got 5"
    let marker = "expected ";
    let start = log.find(marker)? + marker.len();
    let rest = &log[start..];
    let end = rest.find(',').unwrap_or(rest.len());
    rest[..end].trim().parse::<u64>().ok()
}

fn extract_action_id_from_events_json(events: &impl serde::Serialize) -> Option<String> {
    let v = serde_json::to_value(events).ok()?;
    for e in v.as_array()? {
        let kind = e
            .get("kind")
            .or_else(|| e.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if kind != "action_registered" {
            continue;
        }
        for attr in e.get("attributes")?.as_array()? {
            let key = attr.get("key")?.as_str()?;
            let val = attr.get("value")?.as_str()?;
            if key == "action_id" {
                return Some(val.to_string());
            }
            let kb = STANDARD
                .decode(key)
                .ok()
                .and_then(|b| String::from_utf8(b).ok());
            let vb = STANDARD
                .decode(val)
                .ok()
                .and_then(|b| String::from_utf8(b).ok());
            if kb.as_deref() == Some("action_id") {
                return vb;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn tdd_params_parse() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/LumeraProtocol/lumera/action/v1/params"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"params":{"max_raptor_q_symbols":"64","svc_challenge_count":8,"svc_min_chunks_for_challenge":4,"base_action_fee":{"denom":"ulume"}}})))
            .mount(&server).await;
        let c = ChainClient::new(ChainConfig {
            chain_id: "lumera-devnet".into(),
            grpc_endpoint: "".into(),
            rpc_endpoint: "http://127.0.0.1:26657".into(),
            rest_endpoint: server.uri(),
            gas_price: "0.025ulume".into(),
        });
        let p = c.get_action_params().await.unwrap();
        assert_eq!(p.max_raptor_q_symbols, 64);
    }

    #[test]
    fn tdd_extract_action_id_log_json() {
        let log = r#"[{"events":[{"type":"action_registered","attributes":[{"key":"action_id","value":"A-1"}]}]}]"#;
        assert_eq!(extract_action_id_from_log(log).as_deref(), Some("A-1"));
    }

    #[test]
    fn tdd_parse_expected_sequence() {
        let log = "account sequence mismatch, expected 14, got 5: incorrect account sequence";
        assert_eq!(parse_expected_sequence(log), Some(14));
    }

    #[test]
    fn tdd_extract_event_attribute_from_log_json() {
        let log = r#"[{"events":[{"type":"action_registered","attributes":[{"key":"action_id","value":"A-9"},{"key":"creator","value":"lumera1abc"}]}]}]"#;
        assert_eq!(
            extract_event_attribute_from_log(log, "action_registered", "creator").as_deref(),
            Some("lumera1abc")
        );
    }

    #[test]
    fn tdd_extract_event_attributes_from_tx_response_logs() {
        let tx_response = serde_json::json!({
            "logs": [{
                "events": [{
                    "type": "action_registered",
                    "attributes": [
                        {"key": "action_id", "value": "A-42"},
                        {"key": "creator", "value": "lumera1xyz"}
                    ]
                }]
            }]
        });

        let attrs = extract_event_attributes_from_tx_response(&tx_response);
        assert!(attrs.iter().any(|a| a.event_type == "action_registered"
            && a.key == "action_id"
            && a.value == "A-42"));
    }
}
