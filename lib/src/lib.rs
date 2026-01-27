#![no_std]
pub mod errors;

use soroban_sdk::{contracttype, symbol_short, Address, Bytes, String, Symbol, Vec};

/// Oracle data entry
#[derive(Clone, Debug)]
#[contracttype]
pub struct OracleData {
    pub key: Symbol,
    pub value: i128,
    pub timestamp: u64,
    pub provider: Address,
    pub signature: Option<String>,
    pub source: Option<String>,
}

/// Represents an agent's metadata and state
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracttype]
pub struct Agent {
    pub id: u64,
    pub owner: Address,
    pub name: String,
    pub model_hash: String,
    pub metadata_cid: String,
    pub capabilities: Vec<String>,
    pub evolution_level: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub nonce: u64,
    pub escrow_locked: bool,
    pub escrow_holder: Option<Address>,
}

/// Rate limiting window for security protection
#[derive(Clone, Copy)]
#[contracttype]
pub struct RateLimit {
    pub window_seconds: u64,
    pub max_operations: u32,
}

/// Represents a marketplace listing
#[derive(Clone)]
#[contracttype]
pub struct Listing {
    pub listing_id: u64,
    pub agent_id: u64,
    pub seller: Address,
    pub price: i128,
    pub listing_type: ListingType, // Sale, Lease, etc.
    pub active: bool,
    pub created_at: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum ListingType {
    Sale = 0,
    Lease = 1,
    Auction = 2,
}

/// Represents an evolution/upgrade request
#[derive(Clone)]
#[contracttype]
pub struct EvolutionRequest {
    pub request_id: u64,
    pub agent_id: u64,
    pub owner: Address,
    pub stake_amount: i128,
    pub status: EvolutionStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum EvolutionStatus {
    Pending = 0,
    InProgress = 1,
    Completed = 2,
    Failed = 3,
}

/// Royalty information for marketplace transactions
#[derive(Clone)]
#[contracttype]
pub struct RoyaltyInfo {
    pub recipient: Address,
    pub fee: u32, // 0-10000 representing 0-100%
}

/// Oracle attestation for evolution completion (signed by oracle provider)
#[derive(Clone)]
#[contracttype]
pub struct EvolutionAttestation {
    pub request_id: u64,
    pub agent_id: u64,
    pub oracle_provider: Address,
    pub new_model_hash: String,
    pub attestation_data: Bytes,
    pub signature: Bytes,
    pub timestamp: u64,
    pub nonce: u64,
}

/// Constants for security hardening
// Config
pub const ADMIN_KEY: &str = "admin";
pub const MAX_STRING_LENGTH: u32 = 256;
pub const MAX_ROYALTY_FEE: u32 = 10000;
pub const MAX_DATA_SIZE: u32 = 65536;
pub const MAX_HISTORY_SIZE: u32 = 1000;
pub const MAX_HISTORY_QUERY_LIMIT: u32 = 500;
pub const DEFAULT_RATE_LIMIT_OPERATIONS: u32 = 100;
pub const DEFAULT_RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_ROYALTY_PERCENTAGE: u32 = 10000; // 100%
pub const MIN_ROYALTY_PERCENTAGE: u32 = 0;
pub const SAFE_ARITHMETIC_CHECK_OVERFLOW: u128 = u128::MAX;
pub const PRICE_UPPER_BOUND: i128 = i128::MAX / 2; // Prevent overflow in calculations
pub const PRICE_LOWER_BOUND: i128 = 0; // Prevent negative prices
pub const MAX_DURATION_DAYS: u64 = 36500; // ~100 years max lease duration
pub const MAX_AGE_SECONDS: u64 = 365 * 24 * 60 * 60; // ~1 year max data age
pub const ATTESTATION_SIGNATURE_SIZE: usize = 64; // Ed25519 signature size
pub const MAX_ATTESTATION_DATA_SIZE: usize = 1024; // Max size for attestation data

// Storage keys
pub const EXEC_CTR_KEY: Symbol = symbol_short!("exec_ctr");
pub const REQUEST_COUNTER_KEY: &str = "request_counter";
pub const CLAIM_COOLDOWN_KEY: &str = "claim_cooldown";
pub const MAX_CLAIMS_PER_PERIOD_KEY: &str = "max_claims_per_period";
pub const TESTNET_FLAG_KEY: &str = "testnet_mode";
pub const DEFAULT_COOLDOWN_SECONDS: u64 = 86400; // 24 hours
pub const DEFAULT_MAX_CLAIMS: u32 = 1;
pub const LISTING_COUNTER_KEY: &str = "listing_counter";
pub const PROVIDER_LIST_KEY: &str = "providers";
pub const AGENT_COUNTER_KEY: &str = "agent_counter";
pub const AGENT_KEY_PREFIX: &str = "agent_";
pub const AGENT_LEASE_STATUS_PREFIX: &str = "agent_lease_";
pub const APPROVED_MINTERS_KEY: &str = "approved_minters";