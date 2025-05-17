use std::str::FromStr;

use bech32::Hrp;
use bip32::{
    DerivationPath, Mnemonic, PrivateKey, XPrv,
    secp256k1::{ecdsa::SigningKey, elliptic_curve::rand_core::OsRng},
};
use cosmrs::{
    AccountId, Any, Tx,
    crypto::PublicKey,
    tx::{Body, BodyBuilder, Fee, ModeInfo, SignDoc, SignMode, SignerInfo, SignerPublicKey},
};
use eyre::{Context, bail};
use prost::{Message, Name};
use sha3::Digest;
use tracing::debug;

use crate::{
    chain::{Bech32Prefixes, ChainInfo},
    cmd::AccountArgs,
    cosmos_sdk_extra::ethermint::EthPubKey,
    cosmos_sdk_extra::injective::EthPubKey as InjectiveEthPubKey,
    ser::CosmosJsonSerializable,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum WalletKeyType {
    /// Standard Cosmos SDK secp256k1 key
    #[default]
    Secp256k1,
    /// eth_secp256k1, used by Ethermint/Evmos/etc.
    EthermintSecp256k1 {
        /// Injective has same structure as Ethermint's PubKey, yet it uses different package.
        injective: bool,
    },
}

impl WalletKeyType {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Secp256k1 => "secp256k1",
            Self::EthermintSecp256k1 { .. } => "eth_secp256k1",
        }
    }
}

impl FromStr for WalletKeyType {
    type Err = eyre::ErrReport;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "secp256k1" => Ok(Self::Secp256k1),
            "eth_secp256k1" => Ok(Self::EthermintSecp256k1 { injective: false }),
            s => bail!("Unsupported wallet key type '{s}'"),
        }
    }
}

impl<'a> TryFrom<&'a Any> for WalletKeyType {
    type Error = eyre::ErrReport;

    fn try_from(value: &'a Any) -> Result<Self, Self::Error> {
        match value.type_url.as_str() {
            "/cosmos.crypto.secp256k1.PubKey" => Ok(Self::Secp256k1),
            "/ethermint.crypto.v1.ethsecp256k1.PubKey" => {
                Ok(Self::EthermintSecp256k1 { injective: false })
            }
            "/injective.crypto.v1beta1.ethsecp256k1.PubKey" => {
                Ok(Self::EthermintSecp256k1 { injective: true })
            }
            type_url => bail!("unsupported public key type '{type_url}'"),
        }
    }
}

impl TryFrom<Any> for WalletKeyType {
    type Error = eyre::ErrReport;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

pub fn derive_key(mnemonic: &str, password: &str, coin_type: u64) -> eyre::Result<SigningKey> {
    let derivation_path: DerivationPath = format!("m/44'/{coin_type}'/0'/0/0")
        .parse()
        .wrap_err("failed to parse derivation path")?;

    let mnemonic =
        Mnemonic::new(mnemonic, Default::default()).wrap_err("failed to parse mnemonic")?;
    let seed = mnemonic.to_seed(password);

    let signing_key =
        XPrv::derive_from_path(seed, &derivation_path).wrap_err("failed to derive keypair")?;

    Ok(signing_key.into())
}

pub struct TxSigner {
    key: SigningKey,
    key_type: WalletKeyType,
    account_number: u64,
    sequence: u64,
}

impl TxSigner {
    pub fn new(key: SigningKey, key_type: WalletKeyType) -> Self {
        Self {
            key,
            key_type,
            account_number: 0,
            sequence: 0,
        }
    }

    pub fn random(key_type: WalletKeyType) -> Self {
        Self::new(SigningKey::random(&mut OsRng), key_type)
    }

    pub fn public_key(&self) -> PublicKey {
        self.key.public_key().into()
    }

    pub fn account_id(&self, hrp: &Hrp) -> eyre::Result<AccountId> {
        match self.key_type {
            WalletKeyType::Secp256k1 => Ok(self.public_key().account_id(hrp.as_str())?),
            WalletKeyType::EthermintSecp256k1 { .. } => {
                // Need uncompressed public key to derive the address
                let pubkey = self.key.public_key().to_encoded_point(false);

                let pubkey_bytes: [u8; 65] = pubkey
                    .as_bytes()
                    .try_into()
                    .expect("secp256k1 uncompressed public key should be 65 bytes");

                let mut hasher = sha3::Keccak256::default();
                sha3::Digest::update(&mut hasher, &pubkey_bytes[1..]);
                let hashed_bytes: [u8; 32] = sha3::Digest::finalize(hasher).into();

                AccountId::new(hrp.as_str(), &hashed_bytes[12..32])
            }
        }
    }

    pub fn signer_public_key(&self) -> SignerPublicKey {
        match self.key_type {
            WalletKeyType::Secp256k1 => SignerPublicKey::Single(self.public_key()),
            // Same bytes, but different type_url
            WalletKeyType::EthermintSecp256k1 { injective } => {
                let pub_key = EthPubKey {
                    key: self.public_key().to_bytes(),
                };

                if injective {
                    SignerPublicKey::Any(Any {
                        type_url: InjectiveEthPubKey::type_url(),
                        value: InjectiveEthPubKey::from(pub_key).encode_to_vec(),
                    })
                } else {
                    SignerPublicKey::Any(Any {
                        type_url: EthPubKey::type_url(),
                        value: pub_key.encode_to_vec(),
                    })
                }
            }
        }
    }

    pub fn with_numbers(mut self, account_number: u64, sequence: u64) -> Self {
        self.account_number = account_number;
        self.sequence = sequence;
        self
    }
}

#[derive(Clone, Debug)]
pub enum SigningAccountType {
    Controller {
        key_type: WalletKeyType,
        account_number: u64,
        sequence: u64,
    },
    Delegator {
        key_type: WalletKeyType,
        account_number: u64,
        sequence: u64,
    },
}

impl SigningAccountType {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Controller { .. } => "controller",
            Self::Delegator { .. } => "delegator",
        }
    }
}

pub fn setup_signer(
    account_args: &AccountArgs,
    bech32_prefixes: &Bech32Prefixes,
    signing_account_type: SigningAccountType,
    generate_only: bool,
) -> eyre::Result<TxSigner> {
    let (key_type, account_number, sequence) = match signing_account_type {
        SigningAccountType::Controller {
            key_type,
            account_number,
            sequence,
        } => (key_type, account_number, sequence),
        SigningAccountType::Delegator {
            key_type,
            account_number,
            sequence,
        } => (key_type, account_number, sequence),
    };

    if generate_only {
        return Ok(TxSigner::random(key_type).with_numbers(account_number, sequence));
    }

    let (expected_address, mnemonic, password, coin_type) = match signing_account_type {
        SigningAccountType::Controller { .. } => (
            &account_args.controller_address,
            account_args.controller_mnemonic.as_ref(),
            "",
            account_args.controller_mnemonic_coin_type,
        ),
        SigningAccountType::Delegator { .. } => (
            &account_args.delegator_address,
            account_args.delegator_mnemonic.as_ref(),
            "",
            account_args.delegator_mnemonic_coin_type,
        ),
    };

    let Some(mnemonic) = mnemonic else {
        bail!(
            "mnemonic not available for {}",
            signing_account_type.type_name()
        );
    };

    let signing_key = derive_key(mnemonic, password, coin_type)?;
    let signer = TxSigner::new(signing_key, key_type).with_numbers(account_number, sequence);

    let address = signer
        .account_id(&bech32_prefixes.account_prefix)
        .wrap_err("failed to derive address from signing key")?;

    debug!(
        ?address,
        account = signing_account_type.type_name(),
        key_type = key_type.type_name(),
        coin_type,
        "derived signer address"
    );
    if *expected_address != address {
        bail!(
            "expected {} address '{expected_address}', got '{address}'",
            signing_account_type.type_name()
        );
    }

    Ok(signer)
}

pub fn construct_transaction_body<'a, I: IntoIterator<Item = &'a CosmosJsonSerializable>>(
    memo: &str,
    msgs: I,
) -> eyre::Result<Body> {
    Ok(BodyBuilder::new()
        .memo(memo)
        .msgs(
            msgs.into_iter()
                .map(|msg| msg.to_any())
                .collect::<Result<Vec<_>, _>>()?,
        )
        .finish())
}

pub fn sign_transaction(
    chain_info: &ChainInfo,
    signer: &TxSigner,
    fee: Fee,
    body: Body,
) -> eyre::Result<Tx> {
    let signer_info = SignerInfo {
        public_key: Some(signer.signer_public_key()),
        mode_info: ModeInfo::single(SignMode::Direct),
        sequence: signer.sequence,
    };

    let auth_info = signer_info.auth_info(fee);
    let sign_doc = SignDoc::new(&body, &auth_info, &chain_info.id, signer.account_number)
        .wrap_err("failed to create SignDoc")?;

    let sign_doc_bytes = sign_doc
        .into_bytes()
        .wrap_err("failed to serialize SignDoc into bytes")?;

    let signature: Vec<u8> = match signer.key_type {
        WalletKeyType::Secp256k1 => signer
            .key
            .sign_recoverable(&sign_doc_bytes)
            .wrap_err("failed to sign SignDoc")?
            .0
            .to_vec(),
        WalletKeyType::EthermintSecp256k1 { .. } => {
            let hash = sha3::Keccak256::digest(&sign_doc_bytes);
            let (signature, recovery_id) = signer
                .key
                .sign_prehash_recoverable(&hash)
                .wrap_err("failed to sign SignDoc")?;

            let mut signature_bytes = signature.to_vec();
            signature_bytes.push(recovery_id.to_byte());
            signature_bytes
        }
    };

    Ok(Tx {
        body,
        auth_info,
        signatures: vec![signature.to_vec()],
    })
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use bech32::Hrp;
    use cosmrs::AccountId;
    use pretty_assertions::assert_eq;

    use super::{TxSigner, WalletKeyType, derive_key};

    #[test]
    fn test_eth_secp256k1_address() {
        let expected = AccountId::from_str("inj19lhpj24vqtglud7kd7e4n3zj8z4lxkl7ex3uv0").unwrap();

        // Don't worry, it's not a real wallet
        let key = derive_key("relief raise grow sketch turtle endless lens replace morning symptom short coin cousin hospital sauce foam stumble wife kind tortoise member heavy web render", "", 60).unwrap();
        let signer = TxSigner::new(key, WalletKeyType::EthermintSecp256k1 { injective: true });

        assert_eq!(
            expected,
            signer.account_id(&Hrp::parse_unchecked("inj")).unwrap(),
        );
    }
}
