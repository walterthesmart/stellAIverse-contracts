#![cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, String, Bytes, Symbol};
use crate::Evolution;

struct TestSetup {
    env: Env,
    admin: Address,
    owner: Address,
    oracle_provider: Address,
    stake_token: Address,
}

impl TestSetup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let oracle_provider = Address::generate(&env);
        let stake_token = Address::generate(&env);

        Evolution::init_contract(env.clone(), admin.clone(), stake_token.clone());

        TestSetup {
            env,
            admin,
            owner,
            oracle_provider,
            stake_token,
        }
    }

    fn create_mock_agent(&self, id: u64) -> stellai_lib::Agent {
        let agent = stellai_lib::Agent {
            id,
            owner: self.owner.clone(),
            name: String::from_str(&self.env, "TestAgent"),
            model_hash: String::from_str(&self.env, "original_hash"),
            capabilities: soroban_sdk::Vec::from_array(&self.env, [
                String::from_str(&self.env, "execute"),
            ]),
            evolution_level: 0,
            created_at: self.env.ledger().timestamp(),
            updated_at: self.env.ledger().timestamp(),
            nonce: 0,
            escrow_locked: false,
            escrow_holder: None,
        };

        let agent_key = String::from_str(&self.env, "agent_1");
        self.env.storage().instance().set(&agent_key, &agent);
        agent
    }

    fn create_evolution_request(&self, request_id: u64, agent_id: u64) -> shared::EvolutionRequest {
        self.create_evolution_request_with_stake(request_id, agent_id, 1000)
    }

    fn create_evolution_request_with_stake(&self, request_id: u64, agent_id: u64, stake_amount: i128) -> shared::EvolutionRequest {
        let request = shared::EvolutionRequest {
            request_id,
            agent_id,
            owner: self.owner.clone(),
            stake_amount,
            status: shared::EvolutionStatus::Pending,
            created_at: self.env.ledger().timestamp(),
            completed_at: None,
        };

        let key = String::from_str(&self.env, "request_1");
        self.env.storage().instance().set(&key, &request);
        request
    }

    fn create_attestation(&self, request_id: u64, agent_id: u64, nonce: u64) -> shared::EvolutionAttestation {
        shared::EvolutionAttestation {
            request_id,
            agent_id,
            oracle_provider: self.oracle_provider.clone(),
            new_model_hash: String::from_str(&self.env, "upgraded_hash_v1"),
            attestation_data: Bytes::from_slice(&self.env, b"training_data_hash"),
            signature: Bytes::from_slice(&self.env, &[0u8; 64]),
            timestamp: self.env.ledger().timestamp(),
            nonce,
        }
    }

    /// Set agent cooldown timestamp for testing
    fn set_agent_cooldown(&self, _agent_id: u64, timestamp: u64) {
        let cooldown_key = Symbol::new(&self.env, "agent_cd_");
        self.env.storage().instance().set(&cooldown_key, &timestamp);
    }

    /// Clear agent cooldown for testing
    fn clear_agent_cooldown(&self, _agent_id: u64) {
        let cooldown_key = Symbol::new(&self.env, "agent_cd_");
        self.env.storage().instance().remove(&cooldown_key);
    }
}

// ============================================
// Evolution Staking Tests
// ============================================

#[test]
fn test_get_evolution_params_returns_defaults() {
    let setup = TestSetup::new();

    let (min_stake, cooldown) = Evolution::get_evolution_params(setup.env.clone());

    // Default values
    assert_eq!(min_stake, 100); // DEFAULT_MIN_STAKE
    assert_eq!(cooldown, 3600); // DEFAULT_COOLDOWN_SECONDS (1 hour)
}

#[test]
fn test_set_evolution_params_updates_values() {
    let setup = TestSetup::new();

    // Set new parameters
    Evolution::set_evolution_params(
        setup.env.clone(),
        setup.admin.clone(),
        500,  // new min stake
        7200, // new cooldown (2 hours)
    );

    let (min_stake, cooldown) = Evolution::get_evolution_params(setup.env.clone());
    assert_eq!(min_stake, 500);
    assert_eq!(cooldown, 7200);
}

#[test]
#[should_panic(expected = "Minimum stake must be positive")]
fn test_set_evolution_params_rejects_zero_min_stake() {
    let setup = TestSetup::new();

    Evolution::set_evolution_params(
        setup.env.clone(),
        setup.admin.clone(),
        0,    // Invalid: zero min stake
        3600,
    );
}

#[test]
#[should_panic(expected = "Minimum stake must be positive")]
fn test_set_evolution_params_rejects_negative_min_stake() {
    let setup = TestSetup::new();

    Evolution::set_evolution_params(
        setup.env.clone(),
        setup.admin.clone(),
        -100, // Invalid: negative min stake
        3600,
    );
}

#[test]
fn test_get_agent_cooldown_returns_zero_for_new_agent() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    let remaining = Evolution::get_agent_cooldown(setup.env.clone(), 1);
    assert_eq!(remaining, 0);
}

#[test]
#[should_panic(expected = "Invalid agent ID")]
fn test_get_agent_cooldown_rejects_invalid_agent_id() {
    let setup = TestSetup::new();

    Evolution::get_agent_cooldown(setup.env.clone(), 0);
}

#[test]
fn test_get_request_returns_none_for_invalid_id() {
    let setup = TestSetup::new();

    let request = Evolution::get_request(setup.env.clone(), 0);
    assert!(request.is_none());
}

#[test]
fn test_get_request_returns_none_for_nonexistent_request() {
    let setup = TestSetup::new();

    let request = Evolution::get_request(setup.env.clone(), 999);
    assert!(request.is_none());
}

#[test]
fn test_get_request_returns_existing_request() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    let request = Evolution::get_request(setup.env.clone(), 1);
    assert!(request.is_some());

    let req = request.unwrap();
    assert_eq!(req.request_id, 1);
    assert_eq!(req.agent_id, 1);
    assert_eq!(req.stake_amount, 1000);
    assert_eq!(req.status, shared::EvolutionStatus::Pending);
}

#[test]
fn test_complete_upgrade_updates_agent_and_request() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    let new_hash = String::from_str(&setup.env, "new_model_v2");
    Evolution::complete_upgrade(
        setup.env.clone(),
        setup.admin.clone(),
        1,
        new_hash.clone(),
    );

    // Verify agent was updated
    let agent_key = String::from_str(&setup.env, "agent_1");
    let agent: stellai_lib::Agent = setup.env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent.evolution_level, 1);
    assert_eq!(agent.model_hash, new_hash);

    // Verify request was completed
    let request = Evolution::get_request(setup.env.clone(), 1).unwrap();
    assert_eq!(request.status, shared::EvolutionStatus::Completed);
    assert!(request.completed_at.is_some());
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_complete_upgrade_rejects_non_admin() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    let non_admin = Address::generate(&setup.env);
    let new_hash = String::from_str(&setup.env, "new_model_v2");

    Evolution::complete_upgrade(
        setup.env.clone(),
        non_admin,
        1,
        new_hash,
    );
}

#[test]
#[should_panic(expected = "Request is not in pending state")]
fn test_complete_upgrade_rejects_already_completed() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    // Create a completed request
    let mut request = setup.create_evolution_request(1, 1);
    request.status = shared::EvolutionStatus::Completed;
    let key = String::from_str(&setup.env, "request_1");
    setup.env.storage().instance().set(&key, &request);

    let new_hash = String::from_str(&setup.env, "new_model_v2");

    Evolution::complete_upgrade(
        setup.env.clone(),
        setup.admin.clone(),
        1,
        new_hash,
    );
}

// ============================================
// Attestation Tests
// ============================================

#[test]
fn test_valid_attestation_updates_agent() {
    let setup = TestSetup::new();
    let env = &setup.env;

    // Setup: Create agent and request
    let agent_id = 1u64;
    let request_id = 1u64;
    setup.create_mock_agent(agent_id);
    setup.create_evolution_request(request_id, agent_id);

    // Get initial state
    let agent_key = String::from_str(env, "agent_1");
    let initial_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(initial_agent.evolution_level, 0);
    assert_eq!(initial_agent.model_hash, String::from_str(env, "original_hash"));

    // Apply valid attestation
    let attestation = setup.create_attestation(request_id, agent_id, 1);
    Evolution::apply_attestation(env.clone(), attestation.clone());

    // Verify agent was updated
    let updated_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(updated_agent.evolution_level, 1);
    assert_eq!(updated_agent.model_hash, String::from_str(env, "upgraded_hash_v1"));
    assert_eq!(updated_agent.nonce, 1);

    // Verify request status changed
    let request_key = String::from_str(env, "request_1");
    let updated_request: shared::EvolutionRequest = env.storage().instance().get(&request_key).unwrap();
    assert_eq!(updated_request.status, shared::EvolutionStatus::Completed);
    assert!(updated_request.completed_at.is_some());
}

#[test]
#[should_panic(expected = "Invalid signature size")]
fn test_attestation_invalid_signature_size_rejected() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Create attestation with invalid signature size
    let mut attestation = setup.create_attestation(1, 1, 1);
    attestation.signature = Bytes::from_slice(env, &[0u8; 32]); // Wrong size

    Evolution::apply_attestation(env.clone(), attestation);
}

#[test]
fn test_replay_protection_prevents_reuse() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Apply attestation with nonce 1
    let attestation1 = setup.create_attestation(1, 1, 1);
    Evolution::apply_attestation(env.clone(), attestation1);

    let agent_key = String::from_str(env, "agent_1");
    let agent_after_first: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent_after_first.evolution_level, 1);

    // Reset request for second attempt
    let request_key = String::from_str(env, "request_1");
    let mut request: shared::EvolutionRequest = env.storage().instance().get(&request_key).unwrap();
    request.status = shared::EvolutionStatus::Pending;
    request.completed_at = None;
    env.storage().instance().set(&request_key, &request);

    // Try to apply with same nonce (should fail)
    let attestation2 = setup.create_attestation(1, 1, 1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Evolution::apply_attestation(env.clone(), attestation2);
    }));

    assert!(result.is_err()); // Should panic due to replay protection

    // Verify agent wasn't updated again
    let agent_after_replay: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent_after_replay.evolution_level, 1); // Still 1, not 2
}

#[test]
fn test_replay_protection_with_higher_nonce_allowed() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Apply first attestation
    let attestation1 = setup.create_attestation(1, 1, 1);
    Evolution::apply_attestation(env.clone(), attestation1);

    let agent_key = String::from_str(env, "agent_1");
    let agent_after_first: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent_after_first.evolution_level, 1);

    // Reset request for second attestation
    let request_key = String::from_str(env, "request_1");
    let mut request: shared::EvolutionRequest = env.storage().instance().get(&request_key).unwrap();
    request.status = shared::EvolutionStatus::Pending;
    request.completed_at = None;
    env.storage().instance().set(&request_key, &request);

    // Apply with higher nonce (should succeed)
    let attestation2 = setup.create_attestation(1, 1, 2);
    Evolution::apply_attestation(env.clone(), attestation2);

    let agent_after_second: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent_after_second.evolution_level, 2);
}

#[test]
#[should_panic(expected = "Upgrade request not found")]
fn test_attestation_invalid_request_rejected() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    // Don't create request - try to apply attestation for non-existent request

    let attestation = setup.create_attestation(999, 1, 1); // Non-existent request

    Evolution::apply_attestation(env.clone(), attestation);
}

#[test]
#[should_panic(expected = "Agent ID mismatch in attestation")]
fn test_attestation_agent_mismatch_rejected() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Create attestation with mismatched agent ID
    let mut attestation = setup.create_attestation(1, 1, 1);
    attestation.agent_id = 999; // Different from request

    Evolution::apply_attestation(env.clone(), attestation);
}

#[test]
#[should_panic(expected = "Request is not in pending state")]
fn test_attestation_non_pending_request_rejected() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Mark request as already completed
    let request_key = String::from_str(env, "request_1");
    let mut request: shared::EvolutionRequest = env.storage().instance().get(&request_key).unwrap();
    request.status = shared::EvolutionStatus::Completed;
    env.storage().instance().set(&request_key, &request);

    let attestation = setup.create_attestation(1, 1, 1);
    Evolution::apply_attestation(env.clone(), attestation);
}

#[test]
#[should_panic(expected = "Attestation data exceeds maximum size")]
fn test_attestation_oversized_data_rejected() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Create attestation with oversized data
    let mut attestation = setup.create_attestation(1, 1, 1);
    let oversized_data: std::vec::Vec<u8> = std::vec![0u8; shared::MAX_ATTESTATION_DATA_SIZE + 1];
    attestation.attestation_data = Bytes::from_slice(env, &oversized_data);

    Evolution::apply_attestation(env.clone(), attestation);
}

#[test]
fn test_attestation_updates_nonce_tracking() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Apply attestation with nonce 5
    let attestation = setup.create_attestation(1, 1, 5);
    Evolution::apply_attestation(env.clone(), attestation);

    // Reset request
    let request_key = String::from_str(env, "request_1");
    let mut request: shared::EvolutionRequest = env.storage().instance().get(&request_key).unwrap();
    request.status = shared::EvolutionStatus::Pending;
    request.completed_at = None;
    env.storage().instance().set(&request_key, &request);

    // Attempt with nonce 3 (lower than stored 5) should fail
    let attestation_low = setup.create_attestation(1, 1, 3);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Evolution::apply_attestation(env.clone(), attestation_low);
    }));

    assert!(result.is_err());
}

#[test]
fn test_multiple_attestations_sequential() {
    let setup = TestSetup::new();
    let env = &setup.env;

    // Create first agent and request
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Apply first attestation
    let att1 = setup.create_attestation(1, 1, 1);
    Evolution::apply_attestation(env.clone(), att1);

    let agent_key = String::from_str(env, "agent_1");
    let agent1: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent1.evolution_level, 1);

    // Reset for second evolution
    let request_key = String::from_str(env, "request_1");
    let mut request: shared::EvolutionRequest = env.storage().instance().get(&request_key).unwrap();
    request.status = shared::EvolutionStatus::Pending;
    request.completed_at = None;
    env.storage().instance().set(&request_key, &request);

    // Apply second attestation with higher nonce
    let mut att2 = setup.create_attestation(1, 1, 2);
    att2.new_model_hash = String::from_str(env, "upgraded_hash_v2");
    Evolution::apply_attestation(env.clone(), att2);

    let agent2: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent2.evolution_level, 2);
    assert_eq!(agent2.model_hash, String::from_str(env, "upgraded_hash_v2"));
}

// ============================================
// Stake Claiming Tests
// ============================================

#[test]
#[should_panic(expected = "Invalid request ID")]
fn test_claim_stake_rejects_invalid_request_id() {
    let setup = TestSetup::new();

    Evolution::claim_stake(setup.env.clone(), setup.owner.clone(), 0);
}

#[test]
#[should_panic(expected = "Upgrade request not found")]
fn test_claim_stake_rejects_nonexistent_request() {
    let setup = TestSetup::new();

    Evolution::claim_stake(setup.env.clone(), setup.owner.clone(), 999);
}

#[test]
#[should_panic(expected = "Unauthorized: only request owner can claim stake")]
fn test_claim_stake_rejects_non_owner() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    // Create completed request
    let mut request = setup.create_evolution_request(1, 1);
    request.status = shared::EvolutionStatus::Completed;
    let key = String::from_str(&setup.env, "request_1");
    setup.env.storage().instance().set(&key, &request);

    let non_owner = Address::generate(&setup.env);

    Evolution::claim_stake(setup.env.clone(), non_owner, 1);
}

#[test]
#[should_panic(expected = "Stake not yet available for claim")]
fn test_claim_stake_rejects_pending_request() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1); // Creates pending request

    Evolution::claim_stake(setup.env.clone(), setup.owner.clone(), 1);
}

#[test]
#[should_panic(expected = "Stake not yet available for claim")]
fn test_claim_stake_rejects_in_progress_request() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    let mut request = setup.create_evolution_request(1, 1);
    request.status = shared::EvolutionStatus::InProgress;
    let key = String::from_str(&setup.env, "request_1");
    setup.env.storage().instance().set(&key, &request);

    Evolution::claim_stake(setup.env.clone(), setup.owner.clone(), 1);
}

#[test]
#[should_panic(expected = "Stake already claimed for this request")]
fn test_claim_stake_prevents_double_claim() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    // Create completed request
    let mut request = setup.create_evolution_request(1, 1);
    request.status = shared::EvolutionStatus::Completed;
    let key = String::from_str(&setup.env, "request_1");
    setup.env.storage().instance().set(&key, &request);

    // Mark as already claimed
    let stake_lock = String::from_str(&setup.env, "stake_1");
    setup.env.storage().instance().set(&stake_lock, &true);

    Evolution::claim_stake(setup.env.clone(), setup.owner.clone(), 1);
}

// ============================================
// Evolution Level Tests
// ============================================

#[test]
fn test_get_evolution_level_returns_zero_for_new_agent() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    let level = Evolution::get_evolution_level(setup.env.clone(), 1);
    assert_eq!(level, 0);
}

#[test]
fn test_get_evolution_level_returns_correct_level_after_evolution() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Apply attestation to evolve agent
    let attestation = setup.create_attestation(1, 1, 1);
    Evolution::apply_attestation(setup.env.clone(), attestation);

    let level = Evolution::get_evolution_level(setup.env.clone(), 1);
    assert_eq!(level, 1);
}

#[test]
#[should_panic(expected = "Invalid agent ID")]
fn test_get_evolution_level_rejects_invalid_agent_id() {
    let setup = TestSetup::new();

    Evolution::get_evolution_level(setup.env.clone(), 0);
}

// ============================================
// Upgrade History Tests
// ============================================

#[test]
#[should_panic(expected = "Invalid agent ID")]
fn test_get_upgrade_history_rejects_invalid_agent_id() {
    let setup = TestSetup::new();

    Evolution::get_upgrade_history(setup.env.clone(), 0);
}

#[test]
fn test_get_upgrade_history_returns_empty_for_new_agent() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);

    let history = Evolution::get_upgrade_history(setup.env.clone(), 1);
    assert!(history.is_empty());
}

// ============================================
// Contract Initialization Tests
// ============================================

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_init_contract_rejects_double_initialization() {
    let setup = TestSetup::new();
    let new_admin = Address::generate(&setup.env);
    let new_token = Address::generate(&setup.env);

    Evolution::init_contract(setup.env.clone(), new_admin, new_token);
}

// ============================================
// Edge Cases and Security Tests
// ============================================

#[test]
fn test_attestation_with_max_model_hash_length_succeeds() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Create model hash at max length
    let long_hash_str: std::string::String = "x".repeat(shared::MAX_STRING_LENGTH);
    let long_hash = String::from_str(&setup.env, &long_hash_str);
    let mut attestation = setup.create_attestation(1, 1, 1);
    attestation.new_model_hash = long_hash;

    // Should succeed
    Evolution::apply_attestation(setup.env.clone(), attestation);

    let agent_key = String::from_str(&setup.env, "agent_1");
    let agent: stellai_lib::Agent = setup.env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(agent.evolution_level, 1);
}

#[test]
#[should_panic(expected = "Model hash exceeds maximum length")]
fn test_attestation_with_oversized_model_hash_rejected() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    // Create model hash exceeding max length
    let oversized_hash_str: std::string::String = "x".repeat(shared::MAX_STRING_LENGTH + 1);
    let oversized_hash = String::from_str(&setup.env, &oversized_hash_str);
    let mut attestation = setup.create_attestation(1, 1, 1);
    attestation.new_model_hash = oversized_hash;

    Evolution::apply_attestation(setup.env.clone(), attestation);
}

#[test]
fn test_evolution_increments_agent_nonce() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    let agent_key = String::from_str(&setup.env, "agent_1");
    let initial_agent: stellai_lib::Agent = setup.env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(initial_agent.nonce, 0);

    let attestation = setup.create_attestation(1, 1, 1);
    Evolution::apply_attestation(setup.env.clone(), attestation);

    let updated_agent: stellai_lib::Agent = setup.env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(updated_agent.nonce, 1);
}

#[test]
fn test_complete_upgrade_increments_agent_nonce() {
    let setup = TestSetup::new();
    setup.create_mock_agent(1);
    setup.create_evolution_request(1, 1);

    let agent_key = String::from_str(&setup.env, "agent_1");
    let initial_agent: stellai_lib::Agent = setup.env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(initial_agent.nonce, 0);

    let new_hash = String::from_str(&setup.env, "upgraded_hash");
    Evolution::complete_upgrade(setup.env.clone(), setup.admin.clone(), 1, new_hash);

    let updated_agent: stellai_lib::Agent = setup.env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(updated_agent.nonce, 1);
}
