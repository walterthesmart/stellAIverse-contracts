use soroban_sdk::{contracterror, contracttype, Address, Bytes, String, Vec};

// ============================================================================
// Contract Error Enum
// ============================================================================
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    DuplicateAgentId = 3,
    AgentNotFound = 4,
    InvalidAgentId = 5,
    InvalidInput = 6,
    AgentLeased = 7,
    OverflowError = 8,
    SameAddressTransfer = 9,
    NotOwner = 10,
    InvalidAmount = 11,
    NotEnoughBalance = 12,
    AlreadyExists = 13,
    InvalidMetadata = 14,
    OracleError = 15,
    RateLimitExceeded = 16,
}
