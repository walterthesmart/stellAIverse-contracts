#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, Env, String, Vec,
};
use stellai_lib::{
    ADMIN_KEY, DEFAULT_RATE_LIMIT_OPERATIONS, DEFAULT_RATE_LIMIT_WINDOW_SECONDS, EXEC_CTR_KEY,
    MAX_DATA_SIZE, MAX_HISTORY_QUERY_LIMIT, MAX_HISTORY_SIZE, MAX_STRING_LENGTH,
};

// Data structures
#[derive(Clone)]
#[contracttype]
pub struct RuleKey {
    pub agent_id: u64,
    pub rule_name: String,
}

#[derive(Clone)]
#[contracttype]
pub struct ActionRecord {
    pub execution_id: u64,
    pub agent_id: u64,
    pub action: String,
    pub executor: Address,
    pub timestamp: u64,
    pub nonce: u64,
    /// Cryptographic hash of execution data for off-chain verification (Issue #10)
    pub execution_hash: Bytes,
}

/// Immutable execution receipt for off-chain proof storage (Issue #10)
/// Receipts are stored separately and cannot be modified after creation
#[derive(Clone)]
#[contracttype]
pub struct ExecutionReceipt {
    pub execution_id: u64,
    pub agent_id: u64,
    pub action: String,
    pub executor: Address,
    pub timestamp: u64,
    pub execution_hash: Bytes,
    pub created_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct RateLimitData {
    pub last_reset: u64,
    pub count: u32,
}

#[contract]
pub struct ExecutionHub;

#[contractimpl]
impl ExecutionHub {
    // Initialize contract with admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&EXEC_CTR_KEY, &0u64);

        env.events().publish((symbol_short!("init"),), admin);
    }

    // Get current execution counter
    pub fn get_execution_counter(env: Env) -> u64 {
        env.storage().instance().get(&EXEC_CTR_KEY).unwrap_or(0u64)
    }

    // Increment execution ID
    fn next_execution_id(env: &Env) -> u64 {
        let current: u64 = env.storage().instance().get(&EXEC_CTR_KEY).unwrap_or(0u64);
        let next = current.checked_add(1).expect("Execution ID overflow");
        env.storage().instance().set(&EXEC_CTR_KEY, &next);
        next
    }

    // Register execution rule for agent
    pub fn register_rule(
        env: Env,
        agent_id: u64,
        owner: Address,
        rule_name: String,
        rule_data: Bytes,
    ) {
        owner.require_auth();

        Self::validate_agent_id(agent_id);
        Self::validate_string_length(&rule_name, "Rule name");
        Self::validate_data_size(&rule_data, "Rule data");

        let rule_key = RuleKey {
            agent_id,
            rule_name: rule_name.clone(),
        };
        let timestamp = env.ledger().timestamp();

        env.storage().instance().set(&rule_key, &rule_data);
        env.events().publish(
            (symbol_short!("rule_reg"),),
            (agent_id, rule_name, owner, timestamp),
        );
    }

    // Revoke existing rule
    pub fn revoke_rule(env: Env, agent_id: u64, owner: Address, rule_name: String) {
        owner.require_auth();
        Self::validate_agent_id(agent_id);

        let rule_key = RuleKey {
            agent_id,
            rule_name: rule_name.clone(),
        };
        env.storage().instance().remove(&rule_key);

        env.events()
            .publish((symbol_short!("rule_rev"),), (agent_id, rule_name, owner));
    }

    // Get rule data
    pub fn get_rule(env: Env, agent_id: u64, rule_name: String) -> Option<Bytes> {
        Self::validate_agent_id(agent_id);
        let rule_key = RuleKey {
            agent_id,
            rule_name,
        };
        env.storage().instance().get(&rule_key)
    }

    /// Execute action with validation, replay protection, and proof storage (Issue #10)
    /// 
    /// # Arguments
    /// * `agent_id` - The agent executing the action
    /// * `executor` - Address of the executor
    /// * `action` - Action name/type
    /// * `parameters` - Action parameters
    /// * `nonce` - Replay protection nonce
    /// * `execution_hash` - Cryptographic hash for off-chain verification
    ///
    /// # Returns
    /// The execution ID for this action
    pub fn execute_action(
        env: Env,
        agent_id: u64,
        executor: Address,
        action: String,
        parameters: Bytes,
        nonce: u64,
        execution_hash: Bytes,
    ) -> u64 {
        executor.require_auth();

        Self::validate_agent_id(agent_id);
        Self::validate_string_length(&action, "Action name");
        Self::validate_data_size(&parameters, "Parameters");
        Self::validate_data_size(&execution_hash, "Execution hash");

        // Replay protection
        let stored_nonce = Self::get_action_nonce(&env, agent_id);
        if nonce <= stored_nonce {
            panic!("Invalid nonce: replay protection triggered");
        }

        // Rate limiting
        Self::check_rate_limit(
            &env,
            agent_id,
            DEFAULT_RATE_LIMIT_OPERATIONS,
            DEFAULT_RATE_LIMIT_WINDOW_SECONDS,
        );

        let execution_id = Self::next_execution_id(&env);
        let timestamp = env.ledger().timestamp();
        
        Self::set_action_nonce(&env, agent_id, nonce);
        Self::record_action_in_history(&env, agent_id, execution_id, &action, &executor, nonce, &execution_hash);
        Self::store_execution_receipt(&env, execution_id, agent_id, &action, &executor, timestamp, &execution_hash);

        env.events().publish(
            (symbol_short!("act_exec"),),
            (execution_id, agent_id, action.clone(), executor.clone(), timestamp, nonce, execution_hash.clone()),
        );

        execution_id
    }

    // Get execution history
    pub fn get_history(env: Env, agent_id: u64, limit: u32) -> Vec<ActionRecord> {
        Self::validate_agent_id(agent_id);

        if limit > MAX_HISTORY_QUERY_LIMIT {
            panic!("Limit exceeds maximum allowed (500)");
        }

        let history_key = symbol_short!("hist");
        let agent_key = (history_key, agent_id);
        let history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&agent_key)
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        let start_idx = if history.len() > limit {
            history.len() - limit
        } else {
            0
        };

        for i in start_idx..history.len() {
            if let Some(item) = history.get(i) {
                result.push_back(item);
            }
        }

        result
    }

    // Get total action count
    pub fn get_action_count(env: Env, agent_id: u64) -> u32 {
        Self::validate_agent_id(agent_id);
        let history_key = symbol_short!("hist");
        let agent_key = (history_key, agent_id);
        let history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&agent_key)
            .unwrap_or_else(|| Vec::new(&env));
        history.len()
    }

    /// Get execution receipt by execution ID (Issue #10)
    /// Read-only getter for immutable execution proofs
    /// Returns None if the execution ID doesn't exist
    pub fn get_execution_receipt(env: Env, execution_id: u64) -> Option<ExecutionReceipt> {
        let receipt_key = symbol_short!("receipt");
        let exec_receipt_key = (receipt_key, execution_id);
        env.storage().instance().get(&exec_receipt_key)
    }

    /// Get agent ID for a given execution ID (Issue #10)
    /// Provides reverse lookup from execution to agent
    /// Returns None if the execution ID doesn't exist
    pub fn get_agent_for_execution(env: Env, execution_id: u64) -> Option<u64> {
        let exec_agent_key = symbol_short!("exagent");
        let exec_to_agent_key = (exec_agent_key, execution_id);
        env.storage().instance().get(&exec_to_agent_key)
    }

    /// Get all execution receipts for an agent (Issue #10)
    /// Returns a list of execution receipts for the given agent
    pub fn get_agent_receipts(env: Env, agent_id: u64, limit: u32) -> Vec<ExecutionReceipt> {
        Self::validate_agent_id(agent_id);
        
        if limit > MAX_HISTORY_QUERY_LIMIT {
            panic!("Limit exceeds maximum allowed (500)");
        }

        // Get action history and extract receipts
        let history_key = symbol_short!("hist");
        let agent_key = (history_key, agent_id);
        let history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&agent_key)
            .unwrap_or_else(|| Vec::new(&env));

        let mut receipts = Vec::new(&env);
        let start_idx = if history.len() > limit {
            history.len() - limit
        } else {
            0
        };

        for i in start_idx..history.len() {
            if let Some(record) = history.get(i) {
                if let Some(receipt) = Self::get_execution_receipt(env.clone(), record.execution_id) {
                    receipts.push_back(receipt);
                }
            }
        }

        receipts
    }

    // Get admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("Admin not set")
    }

    // Transfer admin rights
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::verify_admin(&env, &current_admin);

        env.storage().instance().set(&ADMIN_KEY, &new_admin);
        env.events()
            .publish((symbol_short!("adm_xfer"),), (current_admin, new_admin));
    }

    // Helper: verify admin
    fn verify_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("Admin not set");
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    // Helper: validate agent ID
    fn validate_agent_id(agent_id: u64) {
        if agent_id == 0 {
            panic!("Invalid agent ID: must be non-zero");
        }
    }

    // Helper: validate string length
    fn validate_string_length(s: &String, _field_name: &str) {
        if s.len() > MAX_STRING_LENGTH {
            panic!("String exceeds maximum length");
        }
    }

    // Helper: validate data size
    fn validate_data_size(data: &Bytes, _field_name: &str) {
        if data.len() > MAX_DATA_SIZE {
            panic!("Data exceeds maximum size");
        }
    }

    // Helper: get nonce
    fn get_action_nonce(env: &Env, agent_id: u64) -> u64 {
        let nonce_key = symbol_short!("nonce");
        let agent_nonce_key = (nonce_key, agent_id);
        env.storage().instance().get(&agent_nonce_key).unwrap_or(0)
    }

    // Helper: set nonce
    fn set_action_nonce(env: &Env, agent_id: u64, nonce: u64) {
        let nonce_key = symbol_short!("nonce");
        let agent_nonce_key = (nonce_key, agent_id);
        env.storage().instance().set(&agent_nonce_key, &nonce);
    }

    // Helper: record action in history with execution hash (Issue #10)
    fn record_action_in_history(
        env: &Env,
        agent_id: u64,
        execution_id: u64,
        action: &String,
        executor: &Address,
        nonce: u64,
        execution_hash: &Bytes,
    ) {
        let history_key = symbol_short!("hist");
        let agent_key = (history_key, agent_id);

        let mut history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&agent_key)
            .unwrap_or_else(|| Vec::new(env));

        if history.len() >= MAX_HISTORY_SIZE {
            panic!("Action history limit exceeded");
        }

        let timestamp = env.ledger().timestamp();
        let record = ActionRecord {
            execution_id,
            agent_id,
            action: action.clone(),
            executor: executor.clone(),
            timestamp,
            nonce,
            execution_hash: execution_hash.clone(),
        };

        history.push_back(record);
        env.storage().instance().set(&agent_key, &history);
    }

    /// Helper: store immutable execution receipt (Issue #10)
    /// Receipts are stored separately and cannot be modified after creation
    fn store_execution_receipt(
        env: &Env,
        execution_id: u64,
        agent_id: u64,
        action: &String,
        executor: &Address,
        timestamp: u64,
        execution_hash: &Bytes,
    ) {
        let receipt_key = symbol_short!("receipt");
        let exec_receipt_key = (receipt_key, execution_id);

        // Create immutable receipt
        let receipt = ExecutionReceipt {
            execution_id,
            agent_id,
            action: action.clone(),
            executor: executor.clone(),
            timestamp,
            execution_hash: execution_hash.clone(),
            created_at: env.ledger().timestamp(),
        };

        // Store receipt - immutable after creation
        env.storage().instance().set(&exec_receipt_key, &receipt);

        // Map execution ID to agent for reverse lookups
        let exec_agent_key = symbol_short!("exagent");
        let exec_to_agent_key = (exec_agent_key, execution_id);
        env.storage().instance().set(&exec_to_agent_key, &agent_id);
    }

    // Helper: check rate limit
    fn check_rate_limit(env: &Env, agent_id: u64, max_operations: u32, window_seconds: u64) {
        let now = env.ledger().timestamp();
        let limit_key = symbol_short!("ratelim");
        let agent_limit_key = (limit_key, agent_id);

        let rate_data: Option<RateLimitData> = env.storage().instance().get(&agent_limit_key);
        let (last_reset, count) = match rate_data {
            Some(data) => (data.last_reset, data.count),
            None => (now, 0),
        };

        let elapsed = now.saturating_sub(last_reset);

        let (new_reset, new_count) = if elapsed > window_seconds {
            (now, 1)
        } else if count < max_operations {
            (last_reset, count + 1)
        } else {
            panic!("Rate limit exceeded");
        };

        let new_rate_data = RateLimitData {
            last_reset: new_reset,
            count: new_count,
        };

        env.storage()
            .instance()
            .set(&agent_limit_key, &new_rate_data);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_execution_counter(), 0);
    }

    #[test]
    #[should_panic(expected = "Contract already initialized")]
    fn test_double_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);
        client.initialize(&admin);
    }

    #[test]
    fn test_execution_counter_increment() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "test_action");
        let params = Bytes::from_array(&env, &[1, 2, 3]);
        let exec_hash = Bytes::from_array(&env, &[0xab, 0xcd, 0xef]);

        let exec_id_1 = client.execute_action(&1, &executor, &action, &params, &1, &exec_hash);
        assert_eq!(exec_id_1, 1);
        assert_eq!(client.get_execution_counter(), 1);

        let exec_hash_2 = Bytes::from_array(&env, &[0x12, 0x34, 0x56]);
        let exec_id_2 = client.execute_action(&1, &executor, &action, &params, &2, &exec_hash_2);
        assert_eq!(exec_id_2, 2);
        assert_eq!(client.get_execution_counter(), 2);
    }

    #[test]
    fn test_register_and_get_rule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let rule_name = String::from_str(&env, "my_rule");
        let rule_data = Bytes::from_array(&env, &[10, 20, 30]);

        client.register_rule(&1, &owner, &rule_name, &rule_data);

        let retrieved = client.get_rule(&1, &rule_name);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), rule_data);
    }

    #[test]
    #[should_panic(expected = "Invalid nonce")]
    fn test_replay_protection() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "test");
        let params = Bytes::from_array(&env, &[1]);
        let exec_hash = Bytes::from_array(&env, &[0xaa, 0xbb]);

        client.execute_action(&1, &executor, &action, &params, &1, &exec_hash);
        client.execute_action(&1, &executor, &action, &params, &1, &exec_hash);
    }

    #[test]
    fn test_get_history() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "test_action");
        let params = Bytes::from_array(&env, &[1]);
        let exec_hash_1 = Bytes::from_array(&env, &[0x11, 0x22]);
        let exec_hash_2 = Bytes::from_array(&env, &[0x33, 0x44]);

        client.execute_action(&1, &executor, &action, &params, &1, &exec_hash_1);
        client.execute_action(&1, &executor, &action, &params, &2, &exec_hash_2);

        let history = client.get_history(&1, &10);
        assert_eq!(history.len(), 2);
        assert_eq!(client.get_action_count(&1), 2);
    }

    #[test]
    fn test_admin_transfer() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin1);

        client.transfer_admin(&admin1, &admin2);
        assert_eq!(client.get_admin(), admin2);
    }

    #[test]
    fn test_rate_limiting() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "test");
        let params = Bytes::from_array(&env, &[1]);

        for i in 1..=10 {
            let exec_hash = Bytes::from_array(&env, &[i as u8, (i * 2) as u8]);
            client.execute_action(&1, &executor, &action, &params, &i, &exec_hash);
        }

        let exec_hash_11 = Bytes::from_array(&env, &[11, 22]);
        let result = client.execute_action(&1, &executor, &action, &params, &11, &exec_hash_11);
        assert!(result > 0);
    }

    // Issue #10: Tests for execution receipt functionality
    #[test]
    fn test_execution_receipt_storage() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "transfer");
        let params = Bytes::from_array(&env, &[1, 2, 3]);
        let exec_hash = Bytes::from_array(&env, &[0xde, 0xad, 0xbe, 0xef]);

        let exec_id = client.execute_action(&1, &executor, &action, &params, &1, &exec_hash);

        // Verify receipt was stored
        let receipt = client.get_execution_receipt(&exec_id);
        assert!(receipt.is_some());
        
        let receipt = receipt.unwrap();
        assert_eq!(receipt.execution_id, exec_id);
        assert_eq!(receipt.agent_id, 1);
        assert_eq!(receipt.action, action);
        assert_eq!(receipt.executor, executor);
        assert_eq!(receipt.execution_hash, exec_hash);
    }

    #[test]
    fn test_get_agent_for_execution() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "action");
        let params = Bytes::from_array(&env, &[1]);
        let exec_hash = Bytes::from_array(&env, &[0xca, 0xfe]);

        let exec_id = client.execute_action(&42, &executor, &action, &params, &1, &exec_hash);

        // Verify reverse lookup works
        let agent_id = client.get_agent_for_execution(&exec_id);
        assert!(agent_id.is_some());
        assert_eq!(agent_id.unwrap(), 42);
    }

    #[test]
    fn test_get_agent_receipts() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "batch_action");
        let params = Bytes::from_array(&env, &[1]);

        // Execute multiple actions for the same agent
        for i in 1..=5u64 {
            let exec_hash = Bytes::from_array(&env, &[i as u8, (i * 10) as u8]);
            client.execute_action(&1, &executor, &action, &params, &i, &exec_hash);
        }

        // Get all receipts for agent
        let receipts = client.get_agent_receipts(&1, &10);
        assert_eq!(receipts.len(), 5);
    }

    #[test]
    fn test_receipt_immutability() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ExecutionHub);
        let client = ExecutionHubClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        let action = String::from_str(&env, "immutable_test");
        let params = Bytes::from_array(&env, &[1]);
        let exec_hash = Bytes::from_array(&env, &[0x11, 0x22, 0x33]);

        let exec_id = client.execute_action(&1, &executor, &action, &params, &1, &exec_hash);

        // Get receipt
        let receipt_1 = client.get_execution_receipt(&exec_id).unwrap();
        
        // Execute another action
        let exec_hash_2 = Bytes::from_array(&env, &[0x44, 0x55, 0x66]);
        client.execute_action(&1, &executor, &action, &params, &2, &exec_hash_2);

        // Original receipt should remain unchanged
        let receipt_2 = client.get_execution_receipt(&exec_id).unwrap();
        assert_eq!(receipt_1.execution_hash, receipt_2.execution_hash);
        assert_eq!(receipt_1.timestamp, receipt_2.timestamp);
    }
}
