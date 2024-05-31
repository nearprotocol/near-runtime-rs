use near_sdk::errors::InvalidHashLength;
use near_sdk::json_types::Base64VecU8;
use near_sdk::{ext_contract, near, require_or_err, BaseError};

pub const FT_METADATA_SPEC: &str = "ft-1.0.0";

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<Base64VecU8>,
    pub decimals: u8,
}

#[ext_contract(ext_ft_metadata)]
pub trait FungibleTokenMetadataProvider {
    fn ft_metadata(&self) -> FungibleTokenMetadata;
}

impl FungibleTokenMetadata {
    pub fn assert_valid(&self) -> Result<(), BaseError> {
        require_or_err!(self.spec == FT_METADATA_SPEC);
        require_or_err!(self.reference.is_some() == self.reference_hash.is_some());
        if let Some(reference_hash) = &self.reference_hash {
            require_or_err!(reference_hash.0.len() == 32, InvalidHashLength::new(32));
        }
        Ok(())
    }
}
