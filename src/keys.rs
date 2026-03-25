use std::str::FromStr;

use bip32::{DerivationPath, XPrv};
use bip39::Mnemonic;
use k256::ecdsa::SigningKey as K256SigningKey;

use crate::error::SdkError;

pub struct SigningIdentity {
    pub chain_signing_key: cosmrs::crypto::secp256k1::SigningKey,
    pub arbitrary_signing_key: K256SigningKey,
    pub address: String,
    pub hrp: String,
}

impl SigningIdentity {
    pub fn from_mnemonic(mnemonic: &str, hrp: &str, derivation_path: &str) -> Result<Self, SdkError> {
        let m = Mnemonic::parse(mnemonic)
            .map_err(|e| SdkError::Crypto(format!("invalid mnemonic: {e}")))?;
        let seed = m.to_seed("");
        let path = DerivationPath::from_str(derivation_path)
            .map_err(|e| SdkError::Crypto(format!("invalid derivation path: {e}")))?;
        let xprv = XPrv::derive_from_path(seed, &path)
            .map_err(|e| SdkError::Crypto(format!("key derivation failed: {e}")))?;

        let sk_bytes = xprv.private_key().to_bytes();
        let chain_signing_key = cosmrs::crypto::secp256k1::SigningKey::from_slice(&sk_bytes)
            .map_err(|e| SdkError::Crypto(format!("chain signing key creation failed: {e}")))?;
        let arbitrary_signing_key = K256SigningKey::from_slice(&sk_bytes)
            .map_err(|e| SdkError::Crypto(format!("message signing key creation failed: {e}")))?;

        let address = chain_signing_key
            .public_key()
            .account_id(hrp)
            .map_err(|e| SdkError::Crypto(format!("address derivation failed: {e}")))?
            .to_string();

        Ok(Self {
            chain_signing_key,
            arbitrary_signing_key,
            address,
            hrp: hrp.to_string(),
        })
    }

    pub fn validate_address(&self, expected_address: &str) -> Result<(), SdkError> {
        if self.address != expected_address {
            return Err(SdkError::InvalidInput(format!(
                "signing identity mismatch: expected address {} but derived {}",
                expected_address, self.address
            )));
        }
        Ok(())
    }

    pub fn validate_chain_prefix(expected_address: &str, expected_hrp: &str) -> Result<(), SdkError> {
        let (actual_hrp, _) = expected_address.split_once('1').ok_or_else(|| {
            SdkError::InvalidInput(format!("invalid bech32 address: {}", expected_address))
        })?;
        if actual_hrp != expected_hrp {
            return Err(SdkError::InvalidInput(format!(
                "address prefix mismatch: expected {} but got {}",
                expected_hrp, actual_hrp
            )));
        }
        Ok(())
    }
}

pub fn derive_signing_keys_from_mnemonic(
    mnemonic: &str,
) -> Result<(cosmrs::crypto::secp256k1::SigningKey, K256SigningKey), SdkError> {
    let id = SigningIdentity::from_mnemonic(mnemonic, "lumera", "m/44'/118'/0'/0/0")?;
    Ok((id.chain_signing_key, id.arbitrary_signing_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn tdd_signing_identity_derives_lumera_address() {
        let id = SigningIdentity::from_mnemonic(TEST_MNEMONIC, "lumera", "m/44'/118'/0'/0/0")
            .expect("derive signing identity");
        assert!(id.address.starts_with("lumera1"));
    }

    #[test]
    fn tdd_validate_chain_prefix() {
        assert!(SigningIdentity::validate_chain_prefix(
            "lumera1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqj9f3h",
            "lumera"
        )
        .is_ok());
        assert!(SigningIdentity::validate_chain_prefix(
            "cosmos1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqnrql8a",
            "lumera"
        )
        .is_err());
    }
}
