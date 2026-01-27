use soroban_sdk::{contracttype, Address, Bytes, BytesN, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleData {
    pub key: String,
    pub value: i128,
    pub timestamp: u64,
    pub provider: Address,
    pub signature: Option<BytesN<64>>,
    pub source: Option<String>,
}

// Add other types that might be needed
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Listing {
    pub id: u128,
    pub agent_id: u128,
    pub seller: Address,
    pub price: i128,
    pub duration: u64,
    pub listing_type: ListingType,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ListingType {
    Sale,
    Rental,
    Service,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvolutionRequest {
    pub request_id: u64,
    pub agent_id: u64,
    pub new_model_hash: String,
    pub min_stake: i128,
    pub cooldown_seconds: u64,
    pub status: EvolutionStatus,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvolutionStatus {
    Pending,
    Completed,
    Failed,
}