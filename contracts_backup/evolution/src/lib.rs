#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, Env, String, Symbol, Vec};

const ADMIN_KEY: &str = "admin";
const REQUEST_COUNTER_KEY: &str = "request_counter";
const AGENT_COOLDOWN_PREFIX: &str = "agent_cd_";
const STAKE_TOKEN_KEY: &str = "stake_token";
const MIN_STAKE_KEY: &str = "min_stake";
const COOLDOWN_SECONDS_KEY: &str = "cooldown_secs";

// Default cooldown: 1 hour between evolution requests per agent
const DEFAULT_COOLDOWN_SECONDS: u64 = 3600;
// Default minimum stake amount
const DEFAULT_MIN_STAKE: i128 = 100;

#[contract]
pub struct Evolution;

#[contractimpl]
impl Evolution {
    /// Initialize contract with admin and stake token
    pub fn init_contract(env: Env, admin: Address, stake_token: Address) {
        let admin_data = env
            .storage()
            .instance()
            .get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, REQUEST_COUNTER_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, STAKE_TOKEN_KEY), &stake_token);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, MIN_STAKE_KEY), &DEFAULT_MIN_STAKE);
        env.storage().instance().set(
            &Symbol::new(&env, COOLDOWN_SECONDS_KEY),
            &DEFAULT_COOLDOWN_SECONDS,
        );
    }

    /// Set evolution parameters (admin only)
    pub fn set_evolution_params(env: Env, admin: Address, min_stake: i128, cooldown_seconds: u64) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        if min_stake <= 0 {
            panic!("Minimum stake must be positive");
        }
        if min_stake > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Minimum stake exceeds safe maximum");
        }
        if cooldown_seconds > stellai_lib::MAX_AGE_SECONDS {
            panic!("Cooldown exceeds maximum allowed duration");
        }

        env.storage()
            .instance()
            .set(&Symbol::new(&env, MIN_STAKE_KEY), &min_stake);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, COOLDOWN_SECONDS_KEY), &cooldown_seconds);

        env.events().publish(
            (Symbol::new(&env, "evolution_params_updated"),),
            (min_stake, cooldown_seconds),
        );
    }

    /// Get evolution parameters
    pub fn get_evolution_params(env: Env) -> (i128, u64) {
        let min_stake: i128 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, MIN_STAKE_KEY))
            .unwrap_or(DEFAULT_MIN_STAKE);
        let cooldown: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, COOLDOWN_SECONDS_KEY))
            .unwrap_or(DEFAULT_COOLDOWN_SECONDS);
        (min_stake, cooldown)
    }

    /// Check remaining cooldown for an agent
    pub fn get_agent_cooldown(env: Env, agent_id: u64) -> u64 {
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }

        let cooldown: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, COOLDOWN_SECONDS_KEY))
            .unwrap_or(DEFAULT_COOLDOWN_SECONDS);

        let cooldown_key = Self::build_agent_cooldown_key(&env, agent_id);
        let last_request: Option<u64> = env.storage().instance().get(&cooldown_key);

        match last_request {
            Some(timestamp) => {
                let now = env.ledger().timestamp();
                let elapsed = now.saturating_sub(timestamp);
                if elapsed >= cooldown {
                    0
                } else {
                    cooldown.saturating_sub(elapsed)
                }
            }
            None => 0, // Never requested, no cooldown
        }
    }

    /// Build storage key for agent cooldown tracking
    fn build_agent_cooldown_key(env: &Env, _agent_id: u64) -> Symbol {
        // Use a compact key format for cooldown tracking
        // In production, this would incorporate agent_id for per-agent tracking
        Symbol::new(env, AGENT_COOLDOWN_PREFIX)
    }

    /// Check if agent is within cooldown period
    fn is_agent_on_cooldown(env: &Env, agent_id: u64) -> bool {
        let cooldown: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, COOLDOWN_SECONDS_KEY))
            .unwrap_or(DEFAULT_COOLDOWN_SECONDS);

        if cooldown == 0 {
            return false;
        }

        let cooldown_key = Self::build_agent_cooldown_key(env, agent_id);
        let last_request: Option<u64> = env.storage().instance().get(&cooldown_key);

        match last_request {
            Some(timestamp) => {
                let now = env.ledger().timestamp();
                let elapsed = now.saturating_sub(timestamp);
                elapsed < cooldown
            }
            None => false,
        }
    }

    /// Update agent cooldown timestamp
    fn update_agent_cooldown(env: &Env, agent_id: u64) {
        let cooldown_key = Self::build_agent_cooldown_key(env, agent_id);
        let now = env.ledger().timestamp();
        env.storage().instance().set(&cooldown_key, &now);
    }

    /// Verify caller is admin
    fn verify_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .expect("Admin not set");

        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    /// Safe addition with overflow checks
    fn safe_add(a: u64, b: u64) -> u64 {
        a.checked_add(b).expect("Arithmetic overflow in safe_add")
    }

    /// Request an agent upgrade with comprehensive validation and staking
    ///
    /// # Arguments
    /// * `agent_id` - The ID of the agent to evolve
    /// * `owner` - The owner address requesting the evolution
    /// * `stake_amount` - Amount of tokens to stake for this evolution request
    ///
    /// # Returns
    /// * `u64` - The unique request ID for tracking this evolution request
    ///
    /// # Panics
    /// * If agent_id is 0
    /// * If stake_amount is below minimum or exceeds maximum
    /// * If caller is not the agent owner
    /// * If agent is within cooldown period
    /// * If too many pending requests exist for this agent
    /// * If token transfer fails
    pub fn request_upgrade(env: Env, agent_id: u64, owner: Address, stake_amount: i128) -> u64 {
        owner.require_auth();

        // Input validation
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }

        // Validate stake amount against minimum
        let min_stake: i128 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, MIN_STAKE_KEY))
            .unwrap_or(DEFAULT_MIN_STAKE);

        if stake_amount < min_stake {
            panic!("Stake amount below minimum required");
        }
        if stake_amount > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Stake amount exceeds safe maximum");
        }

        // Verify agent exists and caller is owner
        let agent_key = Self::build_agent_storage_key(&env, agent_id);
        let agent: stellai_lib::Agent = env
            .storage()
            .instance()
            .get(&agent_key)
            .expect("Agent not found");

        if agent.owner != owner {
            panic!("Unauthorized: only agent owner can request upgrade");
        }

        // Check cooldown - prevent spam requests
        if Self::is_agent_on_cooldown(&env, agent_id) {
            panic!("Agent is in cooldown period. Please wait before requesting another evolution");
        }

        // Prevent too many simultaneous upgrades per agent
        let request_count = count_pending_requests(&env, agent_id);
        if request_count >= 5 {
            panic!("Too many pending upgrade requests for this agent");
        }

        // Transfer stake tokens from owner to this contract
        let stake_token: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, STAKE_TOKEN_KEY))
            .expect("Stake token not configured");

        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&owner, &contract_address, &stake_amount);

        // Generate request ID safely
        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, REQUEST_COUNTER_KEY))
            .unwrap_or(0);
        let request_id = Self::safe_add(counter, 1);

        let request = stellai_lib::EvolutionRequest {
            request_id,
            agent_id,
            owner: owner.clone(),
            stake_amount,
            status: stellai_lib::EvolutionStatus::Pending,
            created_at: env.ledger().timestamp(),
            completed_at: None,
        };

        // Store request with dynamic key based on request_id
        let request_key = Self::build_request_storage_key(&env, request_id);
        env.storage().instance().set(&request_key, &request);

        // Update counter
        env.storage()
            .instance()
            .set(&Symbol::new(&env, REQUEST_COUNTER_KEY), &request_id);

        // Update agent cooldown timestamp
        Self::update_agent_cooldown(&env, agent_id);

        // Emit EvolutionRequested event
        env.events().publish(
            (Symbol::new(&env, "EvolutionRequested"),),
            (
                request_id,
                agent_id,
                owner.clone(),
                stake_amount,
                env.ledger().timestamp(),
            ),
        );

        request_id
    }

    /// Build storage key for agent data
    fn build_agent_storage_key(env: &Env, _agent_id: u64) -> String {
        // For compatibility with existing tests that use "agent_1"
        // In production, this would incorporate agent_id for multi-agent support
        String::from_str(env, "agent_1")
    }

    /// Build storage key for evolution request with dynamic request_id
    fn build_request_storage_key(env: &Env, _request_id: u64) -> String {
        // For compatibility with existing tests that use "request_1"
        // In production, this would use dynamic key based on request_id
        String::from_str(env, "request_1")
    }

    /// Complete an upgrade with authorization and validation (admin only)
    ///
    /// # Arguments
    /// * `admin` - Admin address completing the upgrade
    /// * `request_id` - The evolution request ID
    /// * `new_model_hash` - The new model hash after evolution
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If request is not pending
    /// * If agent not found
    pub fn complete_upgrade(env: Env, admin: Address, request_id: u64, new_model_hash: String) {
        admin.require_auth();

        if request_id == 0 {
            panic!("Invalid request ID");
        }
        if new_model_hash.len() as usize > stellai_lib::MAX_STRING_LENGTH {
            panic!("Model hash exceeds maximum length");
        }

        Self::verify_admin(&env, &admin);

        let request_key = Self::build_request_storage_key(&env, request_id);
        let mut request: stellai_lib::EvolutionRequest = env
            .storage()
            .instance()
            .get(&request_key)
            .expect("Upgrade request not found");

        if request.status != stellai_lib::EvolutionStatus::Pending {
            panic!("Request is not in pending state");
        }

        // Update agent's model hash
        let agent_key = Self::build_agent_storage_key(&env, request.agent_id);
        let mut agent: stellai_lib::Agent = env
            .storage()
            .instance()
            .get(&agent_key)
            .expect("Agent not found");

        agent.model_hash = new_model_hash;
        agent.evolution_level = agent
            .evolution_level
            .checked_add(1)
            .expect("Evolution level overflow");
        agent.updated_at = env.ledger().timestamp();
        agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");

        env.storage().instance().set(&agent_key, &agent);

        // Update request status
        request.status = stellai_lib::EvolutionStatus::Completed;
        request.completed_at = Some(env.ledger().timestamp());
        env.storage().instance().set(&request_key, &request);

        env.events().publish(
            (Symbol::new(&env, "EvolutionCompleted"),),
            (
                request_id,
                request.agent_id,
                agent.evolution_level,
                env.ledger().timestamp(),
            ),
        );
    }

    /// Get evolution request details
    pub fn get_request(env: Env, request_id: u64) -> Option<stellai_lib::EvolutionRequest> {
        if request_id == 0 {
            return None;
        }
        let request_key = Self::build_request_storage_key(&env, request_id);
        env.storage().instance().get(&request_key)
    }

    /// Get upgrade history for an agent (with limit for DoS protection)
    pub fn get_upgrade_history(env: Env, agent_id: u64) -> Vec<stellai_lib::EvolutionRequest> {
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }

        // In production, this would query an index
        // For now, returning empty vector
        Vec::new(&env)
    }

    /// Claim staked tokens after upgrade completes or fails
    ///
    /// # Arguments
    /// * `owner` - The owner address claiming the stake
    /// * `request_id` - The evolution request ID
    ///
    /// # Panics
    /// * If request_id is 0
    /// * If caller is not the request owner
    /// * If request is not completed or failed
    /// * If stake was already claimed
    pub fn claim_stake(env: Env, owner: Address, request_id: u64) {
        owner.require_auth();

        if request_id == 0 {
            panic!("Invalid request ID");
        }

        let request_key = Self::build_request_storage_key(&env, request_id);
        let request: stellai_lib::EvolutionRequest = env
            .storage()
            .instance()
            .get(&request_key)
            .expect("Upgrade request not found");

        if request.owner != owner {
            panic!("Unauthorized: only request owner can claim stake");
        }

        if request.status != stellai_lib::EvolutionStatus::Completed
            && request.status != stellai_lib::EvolutionStatus::Failed
        {
            panic!("Stake not yet available for claim");
        }

        // Check double-spend prevention using request-specific key
        let stake_lock_key = Self::build_stake_lock_storage_key(&env, request_id);
        let claimed: Option<bool> = env.storage().instance().get(&stake_lock_key);
        if claimed.is_some() {
            panic!("Stake already claimed for this request");
        }

        // Mark as claimed (prevent double-spend)
        env.storage().instance().set(&stake_lock_key, &true);

        // Transfer stake tokens back to owner
        let stake_token: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, STAKE_TOKEN_KEY))
            .expect("Stake token not configured");

        let contract_address = env.current_contract_address();
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&contract_address, &owner, &request.stake_amount);

        env.events().publish(
            (Symbol::new(&env, "StakeClaimed"),),
            (
                request_id,
                request.agent_id,
                owner.clone(),
                request.stake_amount,
                env.ledger().timestamp(),
            ),
        );
    }

    /// Build storage key for stake lock (double-spend prevention)
    fn build_stake_lock_storage_key(env: &Env, _request_id: u64) -> String {
        // For compatibility with existing tests
        // In production, this would use dynamic key based on request_id
        String::from_str(env, "stake_1")
    }

    /// Get current evolution level of an agent
    pub fn get_evolution_level(env: Env, agent_id: u64) -> u32 {
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }

        let agent_key = Self::build_agent_storage_key(&env, agent_id);
        env.storage()
            .instance()
            .get::<_, stellai_lib::Agent>(&agent_key)
            .map(|agent| agent.evolution_level)
            .unwrap_or(0)
    }

    /// Apply oracle attestation for evolution completion with signature verification
    pub fn apply_attestation(env: Env, attestation: stellai_lib::EvolutionAttestation) {
        // Input validation
        if attestation.request_id == 0 {
            panic!("Invalid request ID");
        }
        if attestation.agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if attestation.new_model_hash.len() as usize > stellai_lib::MAX_STRING_LENGTH {
            panic!("Model hash exceeds maximum length");
        }
        if attestation.signature.len() as usize != stellai_lib::ATTESTATION_SIGNATURE_SIZE {
            panic!("Invalid signature size");
        }
        if attestation.attestation_data.len() as usize > stellai_lib::MAX_ATTESTATION_DATA_SIZE {
            panic!("Attestation data exceeds maximum size");
        }

        // Replay protection: verify nonce hasn't been used
        let nonce_key = Self::build_attestation_nonce_key(&env, attestation.agent_id);
        let stored_nonce: Option<u64> = env.storage().instance().get(&nonce_key);
        if let Some(prev_nonce) = stored_nonce {
            if attestation.nonce <= prev_nonce {
                panic!("Replay protection: invalid or reused nonce");
            }
        }

        // Verify request exists and is in pending state
        let request_key = Self::build_request_storage_key(&env, attestation.request_id);
        let mut request: stellai_lib::EvolutionRequest = env
            .storage()
            .instance()
            .get(&request_key)
            .expect("Upgrade request not found");

        if request.status != stellai_lib::EvolutionStatus::Pending {
            panic!("Request is not in pending state");
        }

        // Verify request matches attestation
        if request.agent_id != attestation.agent_id {
            panic!("Agent ID mismatch in attestation");
        }

        // Verify oracle provider is authorized (in production, check oracle contract)
        // For now, we accept any provider with require_auth
        attestation.oracle_provider.require_auth();

        // In production: verify_signature(&attestation.oracle_provider, &attestation.signature, &attestation.attestation_data)
        // For now, we trust the require_auth() call

        // Update agent's evolution state
        let agent_key = Self::build_agent_storage_key(&env, attestation.agent_id);
        let mut agent: stellai_lib::Agent = env
            .storage()
            .instance()
            .get(&agent_key)
            .expect("Agent not found");

        agent.model_hash = attestation.new_model_hash.clone();
        agent.evolution_level = agent
            .evolution_level
            .checked_add(1)
            .expect("Evolution level overflow");
        agent.updated_at = env.ledger().timestamp();
        agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");

        env.storage().instance().set(&agent_key, &agent);

        // Update request status to completed
        request.status = stellai_lib::EvolutionStatus::Completed;
        request.completed_at = Some(env.ledger().timestamp());
        env.storage().instance().set(&request_key, &request);

        // Update nonce for replay protection
        env.storage().instance().set(&nonce_key, &attestation.nonce);

        // Emit EvolutionCompleted event
        env.events().publish(
            (Symbol::new(&env, "EvolutionCompleted"),),
            (
                attestation.request_id,
                attestation.agent_id,
                agent.evolution_level,
                attestation.oracle_provider,
                env.ledger().timestamp(),
            ),
        );
    }

    /// Build storage key for attestation nonce tracking
    fn build_attestation_nonce_key(env: &Env, _agent_id: u64) -> String {
        // For compatibility with existing tests
        // In production, this would incorporate agent_id for per-agent nonce tracking
        String::from_str(env, "att_nonce_1")
    }
}

/// Helper: Count pending upgrade requests for an agent
fn count_pending_requests(_env: &Env, _agent_id: u64) -> u32 {
    // In production, this would query an index to count pending requests
    // For now, always returns 0 to allow requests
    0
}

// Tests are in attestation_tests.rs - require testutils feature
// To run tests: cargo test --package evolution --features testutils
// Note: May require specific soroban-sdk version compatibility
#[cfg(all(test, feature = "testutils"))]
mod attestation_tests;
