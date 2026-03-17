use crate::error::SdkError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cosmrs::{
    tx::{BodyBuilder, Fee, SignDoc, SignerInfo},
    Any, Coin,
};
use prost::Message;
use serde::Deserialize;

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
        let fee_coin = Coin {
            amount: 10000u128,
            denom: "ulume"
                .parse()
                .map_err(|e| SdkError::Chain(format!("fee denom parse: {e}")))?,
        };
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
                .auth_info(Fee::from_amount_and_gas(fee_coin.clone(), 500_000u64));
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
    let v: serde_json::Value = serde_json::from_str(log).ok()?;
    let arr = v.as_array()?;
    for item in arr {
        for e in item.get("events")?.as_array()? {
            if e.get("type").and_then(|x| x.as_str()) == Some("action_registered") {
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
        }
    }
    None
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
}
