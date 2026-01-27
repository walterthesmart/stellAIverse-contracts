pub const MAX_STRING_LENGTH: usize = 1024;
pub const MAX_AGE_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days
pub const PRICE_UPPER_BOUND: i128 = 100_000_000_000;
pub const ATTESTATION_SIGNATURE_SIZE: usize = 64;
pub const MAX_ATTESTATION_DATA_SIZE: usize = 2048;
pub const DATA_KEY_PREFIX: &str = "data_";
pub const DATA_HISTORY_PREFIX: &str = "history_";