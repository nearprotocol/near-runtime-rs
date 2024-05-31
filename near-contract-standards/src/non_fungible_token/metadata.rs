use near_sdk::errors::{InvalidContractState, InvalidHashLength};
use near_sdk::json_types::Base64VecU8;
use near_sdk::{contract_error, near, require_or_err, BaseError};

/// This spec can be treated like a version of the standard.
pub const NFT_METADATA_SPEC: &str = "nft-1.0.0";

/// Metadata for the NFT contract itself.
#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers=[borsh, json])]
pub struct NFTContractMetadata {
    pub spec: String,              // required, essentially a version like "nft-1.0.0"
    pub name: String,              // required, ex. "Mosaics"
    pub symbol: String,            // required, ex. "MOSIAC"
    pub icon: Option<String>,      // Data URL
    pub base_uri: Option<String>, // Centralized gateway known to have reliable access to decentralized storage assets referenced by `reference` or `media` URLs
    pub reference: Option<String>, // URL to a JSON file with more info
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenMetadata {
    pub title: Option<String>, // ex. "Arch Nemesis: Mail Carrier" or "Parcel #5055"
    pub description: Option<String>, // free-form description
    pub media: Option<String>, // URL to associated media, preferably to decentralized, content-addressed storage
    pub media_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of content referenced by the `media` field. Required if `media` is included.
    pub copies: Option<u64>, // number of copies of this set of metadata in existence when token was minted.
    pub issued_at: Option<String>, // ISO 8601 datetime when token was issued or minted
    pub expires_at: Option<String>, // ISO 8601 datetime when token expires
    pub starts_at: Option<String>, // ISO 8601 datetime when token starts being valid
    pub updated_at: Option<String>, // ISO 8601 datetime when token was last updated
    pub extra: Option<String>, // anything extra the NFT wants to store on-chain. Can be stringified JSON.
    pub reference: Option<String>, // URL to an off-chain JSON file with more info.
    pub reference_hash: Option<Base64VecU8>, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
}

/// Offers details on the contract-level metadata.
pub trait NonFungibleTokenMetadataProvider {
    fn nft_metadata(&self) -> NFTContractMetadata;
}

impl NFTContractMetadata {
    pub fn assert_valid(&self) -> Result<(), BaseError> {
        require_or_err!(
            self.spec == NFT_METADATA_SPEC,
            InvalidSpec::new("Spec is not NFT metadata")
        );
        require_or_err!(
            self.reference.is_some() == self.reference_hash.is_some(),
            InvalidContractState::new("Reference and reference hash must be present")
        );
        if let Some(reference_hash) = &self.reference_hash {
            require_or_err!(reference_hash.0.len() == 32, InvalidHashLength::new(32));
        }
        Ok(())
    }
}

impl TokenMetadata {
    pub fn assert_valid(&self) -> Result<(), BaseError> {
        require_or_err!(self.media.is_some() == self.media_hash.is_some());
        if let Some(media_hash) = &self.media_hash {
            require_or_err!(media_hash.0.len() == 32, InvalidHashLength::new(32));
        }

        require_or_err!(self.reference.is_some() == self.reference_hash.is_some());
        if let Some(reference_hash) = &self.reference_hash {
            require_or_err!(reference_hash.0.len() == 32, InvalidHashLength::new(32));
        }
        Ok(())
    }
}

#[contract_error]
pub struct InvalidSpec {
    pub message: String,
}

impl InvalidSpec {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}
