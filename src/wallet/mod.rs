use std::str::FromStr;

use bip32::{DerivationPath, Mnemonic};
use cosmrs::{
    Any, Tx,
    crypto::{PublicKey, secp256k1::SigningKey},
    tx::{Body, BodyBuilder, Fee, ModeInfo, SignDoc, SignMode, SignerInfo, SignerPublicKey},
};
use eyre::{Context, bail};
use prost::{Message, Name};
use tracing::debug;

use crate::{
    chain::{Bech32Prefixes, ChainInfo},
    cmd::AccountArgs,
    cosmos_sdk_extra::ethermint::EthPubKey,
    ser::CosmosJsonSerializable,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum WalletKeyType {
    /// Standard Cosmos SDK secp256k1 key
    #[default]
    Secp256k1,
    /// eth_secp256k1, used by Ethermint/Evmos/etc.
    EthermintSecp256k1,
}

impl FromStr for WalletKeyType {
    type Err = eyre::ErrReport;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "secp256k1" => Ok(Self::Secp256k1),
            "eth_secp256k1" => Ok(Self::EthermintSecp256k1),
            s => bail!("Unsupported wallet key type '{s}'"),
        }
    }
}

pub fn derive_key(mnemonic: &str, password: &str, coin_type: u64) -> eyre::Result<SigningKey> {
    let derivation_path: DerivationPath = format!("m/44'/{coin_type}'/0'/0/0")
        .parse()
        .wrap_err("failed to parse derivation path")?;

    let mnemonic =
        Mnemonic::new(mnemonic, Default::default()).wrap_err("failed to parse mnemonic")?;
    let seed = mnemonic.to_seed(password);

    let signing_key = SigningKey::derive_from_path(seed, &derivation_path)
        .wrap_err("failed to derive keypair")?;

    Ok(signing_key)
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
        Self::new(SigningKey::random(), key_type)
    }

    pub fn public_key(&self) -> PublicKey {
        self.key.public_key()
    }

    pub fn signer_public_key(&self) -> SignerPublicKey {
        match self.key_type {
            WalletKeyType::Secp256k1 => self.key.public_key().into(),
            // Same bytes, but different type_url
            WalletKeyType::EthermintSecp256k1 => SignerPublicKey::Any(Any {
                type_url: EthPubKey::type_url(),
                value: EthPubKey {
                    key: self.key.public_key().to_bytes(),
                }
                .encode_to_vec(),
            }),
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
    Controller { account_number: u64, sequence: u64 },
    Delegator { account_number: u64, sequence: u64 },
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
            account_number,
            sequence,
        } => (
            account_args.controller_address_type,
            account_number,
            sequence,
        ),
        SigningAccountType::Delegator {
            account_number,
            sequence,
        } => (
            account_args.delegator_address_type,
            account_number,
            sequence,
        ),
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
        .public_key()
        .account_id(bech32_prefixes.account_prefix.as_str())
        .wrap_err("failed to derive address from signing key")?;

    debug!(
        ?address,
        account = signing_account_type.type_name(),
        coin_type,
        "derived signer address"
    );
    if *expected_address != address {
        bail!(
            "expected {} address '{expected_address}', got '{address}",
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

    let signature = signer
        .key
        .sign(&sign_doc_bytes)
        .wrap_err("failed to sign SignDoc")?;

    Ok(Tx {
        body,
        auth_info,
        signatures: vec![signature.to_vec()],
    })
}
