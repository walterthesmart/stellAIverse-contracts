#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Map};

const ADMIN_KEY: &str = "admin";
const CLAIM_COOLDOWN_KEY: &str = "claim_cooldown";
const MAX_CLAIMS_PER_PERIOD_KEY: &str = "max_claims_per_period";
const LAST_CLAIM_KEY_PREFIX: &str = "last_claim_";
const CLAIM_COUNT_KEY_PREFIX: &str = "claim_count_";
const TESTNET_FLAG_KEY: &str = "testnet_mode";

// Default parameters
const DEFAULT_COOLDOWN_SECONDS: u64 = 86400; // 24 hours
const DEFAULT_MAX_CLAIMS: u32 = 1;

#[contract]
pub struct Faucet;

#[contractimpl]
impl Faucet {
    /// Initialize faucet (admin only)
    pub fn init_faucet(env: Env, admin: Address, testnet_only: bool) {
        let admin_data = env.storage().instance().get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage().instance().set(&Symbol::new(&env, CLAIM_COOLDOWN_KEY), &DEFAULT_COOLDOWN_SECONDS);
        env.storage().instance().set(&Symbol::new(&env, MAX_CLAIMS_PER_PERIOD_KEY), &DEFAULT_MAX_CLAIMS);
        env.storage().instance().set(&Symbol::new(&env, TESTNET_FLAG_KEY), &testnet_only);
    }

    /// Verify caller is admin
    fn verify_admin(env: &Env, caller: &Address) {
        let admin: Address = env.storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .expect("Admin not set");
        
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    /// Check if testnet mode is enabled
    fn is_testnet_mode(env: &Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(env, TESTNET_FLAG_KEY))
            .unwrap_or(true)
    }

    /// Claim a test agent from the faucet with comprehensive security
    pub fn claim_test_agent(
        env: Env,
        claimer: Address,
    ) -> u64 {
        claimer.require_auth();

        // Security: Verify testnet mode
        if !Self::is_testnet_mode(&env) {
            panic!("Faucet is not available on mainnet");
        }

        // Check eligibility
        if !Self::check_eligibility(&env, &claimer) {
            panic!("Address is not eligible for faucet claim at this time");
        }

        // In production: Call agent-nft contract to mint test agent
        // For now, returning placeholder ID
        let agent_id = 1u64;

        // Update rate limiting state
        let now = env.ledger().timestamp();
        let last_claim_key = String::from_slice(&env, 
            &format!("{}{}", LAST_CLAIM_KEY_PREFIX, claimer).as_bytes()
        );
        let claim_count_key = String::from_slice(&env,
            &format!("{}{}", CLAIM_COUNT_KEY_PREFIX, claimer).as_bytes()
        );

        env.storage().instance().set(&last_claim_key, &now);
        env.storage().instance().set(&claim_count_key, &1u32);

        env.events().publish(
            (Symbol::new(&env, "agent_claimed"),),
            (agent_id, claimer)
        );

        agent_id
    }

    /// Check if an address is eligible for a faucet claim
    pub fn check_eligibility(env: Env, address: Address) -> bool {
        let cooldown: u64 = env.storage()
            .instance()
            .get(&Symbol::new(&env, CLAIM_COOLDOWN_KEY))
            .unwrap_or(DEFAULT_COOLDOWN_SECONDS);

        let max_claims: u32 = env.storage()
            .instance()
            .get(&Symbol::new(&env, MAX_CLAIMS_PER_PERIOD_KEY))
            .unwrap_or(DEFAULT_MAX_CLAIMS);

        let last_claim_key = String::from_slice(&env,
            &format!("{}{}", LAST_CLAIM_KEY_PREFIX, address).as_bytes()
        );
        let last_claim: Option<u64> = env.storage().instance().get(&last_claim_key);

        match last_claim {
            Some(timestamp) => {
                let now = env.ledger().timestamp();
                let elapsed = now.checked_sub(timestamp).unwrap_or(0);

                // If cooldown has passed, eligible again
                if elapsed >= cooldown {
                    return true;
                }

                // Check claim count within current period
                let claim_count_key = String::from_slice(&env,
                    &format!("{}{}", CLAIM_COUNT_KEY_PREFIX, address).as_bytes()
                );
                let claims: u32 = env.storage()
                    .instance()
                    .get(&claim_count_key)
                    .unwrap_or(0);

                claims < max_claims
            }
            None => true, // First claim ever
        }
    }

    /// Admin function: Set faucet parameters with validation
    pub fn set_parameters(
        env: Env,
        admin: Address,
        claim_cooldown_seconds: u64,
        max_claims_per_period: u32,
    ) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        // Validation: prevent unreasonable values
        if claim_cooldown_seconds == 0 {
            panic!("Cooldown must be greater than 0");
        }
        if claim_cooldown_seconds > 365 * 24 * 60 * 60 {
            panic!("Cooldown exceeds one year");
        }
        if max_claims_per_period == 0 || max_claims_per_period > 100 {
            panic!("Max claims must be between 1 and 100");
        }

        env.storage().instance().set(&Symbol::new(&env, CLAIM_COOLDOWN_KEY), &claim_cooldown_seconds);
        env.storage().instance().set(&Symbol::new(&env, MAX_CLAIMS_PER_PERIOD_KEY), &max_claims_per_period);

        env.events().publish(
            (Symbol::new(&env, "parameters_updated"),),
            (claim_cooldown_seconds, max_claims_per_period)
        );
    }

    /// Get current faucet parameters
    pub fn get_parameters(env: Env) -> (u64, u32) {
        let cooldown: u64 = env.storage()
            .instance()
            .get(&Symbol::new(&env, CLAIM_COOLDOWN_KEY))
            .unwrap_or(DEFAULT_COOLDOWN_SECONDS);

        let max_claims: u32 = env.storage()
            .instance()
            .get(&Symbol::new(&env, MAX_CLAIMS_PER_PERIOD_KEY))
            .unwrap_or(DEFAULT_MAX_CLAIMS);

        (cooldown, max_claims)
    }

    /// Get remaining cooldown time for an address
    pub fn get_remaining_cooldown(env: Env, address: Address) -> u64 {
        let cooldown: u64 = env.storage()
            .instance()
            .get(&Symbol::new(&env, CLAIM_COOLDOWN_KEY))
            .unwrap_or(DEFAULT_COOLDOWN_SECONDS);

        let last_claim_key = String::from_slice(&env,
            &format!("{}{}", LAST_CLAIM_KEY_PREFIX, address).as_bytes()
        );
        let last_claim: Option<u64> = env.storage().instance().get(&last_claim_key);

        match last_claim {
            Some(timestamp) => {
                let now = env.ledger().timestamp();
                let elapsed = now.checked_sub(timestamp).unwrap_or(0);

                if elapsed >= cooldown {
                    0 // Eligible now
                } else {
                    cooldown.checked_sub(elapsed).unwrap_or(0)
                }
            }
            None => 0, // Never claimed, eligible now
        }
    }

    /// Admin emergency pause function
    pub fn pause_faucet(env: Env, admin: Address, paused: bool) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        env.storage().instance().set(&Symbol::new(&env, TESTNET_FLAG_KEY), &paused);
        env.events().publish((Symbol::new(&env, "faucet_paused"),), (paused,));
    }
}

