use base64::{engine::general_purpose::STANDARD, Engine as _};
use k256::ecdsa::{signature::Signer, Signature, SigningKey};

use crate::error::SdkError;

pub fn sign_raw_message_b64(sk: &SigningKey, message: &str) -> String {
    let sig: Signature = sk.sign(message.as_bytes());
    STANDARD.encode(sig.to_bytes())
}

pub fn make_adr36_sign_bytes(signer: &str, message: &str) -> Result<Vec<u8>, SdkError> {
    let data_b64 = STANDARD.encode(message.as_bytes());
    let doc = serde_json::json!({
      "account_number":"0",
      "chain_id":"",
      "fee":{"amount":[],"gas":"0"},
      "memo":"",
      "msgs":[{
        "type":"sign/MsgSignData",
        "value":{"signer":signer,"data":data_b64}
      }],
      "sequence":"0"
    });
    serde_json::to_vec(&doc).map_err(|e| SdkError::Serialization(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tdd_adr36_shape_stable() {
        let out = make_adr36_sign_bytes("lumera1abc", "hello").unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"type\":\"sign/MsgSignData\""));
        assert!(s.contains("\"signer\":\"lumera1abc\""));
    }

    #[test]
    fn tdd_sign_raw_returns_b64() {
        let sk = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let sig = sign_raw_message_b64(&sk, "hello");
        assert!(!sig.is_empty());
    }
}
