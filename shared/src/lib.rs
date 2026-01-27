#![allow(unused_imports)]
use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

/// Represents an agent's metadata and state
#[derive(Clone)]
#[contracttype]
pub struct Agent {
    pub id: u64,
    pub owner: Address,
    pub name: String,
    pub model_hash: String,
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

/// Oracle data entry
#[derive(Clone)]
#[contracttype]
pub struct OracleData {
    pub key: String,
    pub value: String,
    pub timestamp: u64,
    pub source: String,
}

/// Royalty information for marketplace transactions
#[derive(Clone)]
#[contracttype]
pub struct RoyaltyInfo {
    pub recipient: Address,
    pub percentage: u32, // 0-10000 representing 0-100%
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
pub const MAX_STRING_LENGTH: usize = 256;
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

#[cfg(any(test, feature = "testutils"))]
pub mod testutils {
    use super::*;
    use soroban_sdk::{Address, Bytes, Env, String, Vec};

    pub fn create_oracle_data(env: &Env, key: &str, value: &str, source: &str) -> OracleData {
        OracleData {
            key: String::from_str(env, key),
            value: String::from_str(env, value),
            timestamp: env.ledger().timestamp(),
            source: String::from_str(env, source),
        }
    }

    pub fn create_evolution_attestation(
        env: &Env,
        request_id: u64,
        agent_id: u64,
        oracle_provider: Address,
        new_model_hash: &str,
        nonce: u64,
    ) -> EvolutionAttestation {
        EvolutionAttestation {
            request_id,
            agent_id,
            oracle_provider,
            new_model_hash: String::from_str(env, new_model_hash),
            attestation_data: Bytes::from_slice(env, b"mock_attestation_data"),
            signature: Bytes::from_slice(env, &[0u8; 64]),
            timestamp: env.ledger().timestamp(),
            nonce,
        }
    }
}
