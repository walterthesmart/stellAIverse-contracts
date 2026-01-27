#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, Env, String, Symbol, Vec,
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
    pub action: String,
    pub executor: Address,
    pub timestamp: u64,
    pub nonce: u64,
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

    // Execute action with validation and replay protection
    pub fn execute_action(
        env: Env,
        agent_id: u64,
        executor: Address,
        action: String,
        parameters: Bytes,
        nonce: u64,
    ) -> u64 {
        executor.require_auth();

        Self::validate_agent_id(agent_id);
        Self::validate_string_length(&action, "Action name");
        Self::validate_data_size(&parameters, "Parameters");

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
        Self::set_action_nonce(&env, agent_id, nonce);
        Self::record_action_in_history(&env, agent_id, execution_id, &action, &executor, nonce);

        let timestamp = env.ledger().timestamp();
        env.events().publish(
            (symbol_short!("act_exec"),),
            (execution_id, agent_id, action, executor, timestamp, nonce),
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

    // Helper: record action in history
    fn record_action_in_history(
        env: &Env,
        agent_id: u64,
        execution_id: u64,
        action: &String,
        executor: &Address,
        nonce: u64,
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
            action: action.clone(),
            executor: executor.clone(),
            timestamp,
            nonce,
        };

        history.push_back(record);
        env.storage().instance().set(&agent_key, &history);
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

        let exec_id_1 = client.execute_action(&1, &executor, &action, &params, &1);
        assert_eq!(exec_id_1, 1);
        assert_eq!(client.get_execution_counter(), 1);

        let exec_id_2 = client.execute_action(&1, &executor, &action, &params, &2);
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

        client.execute_action(&1, &executor, &action, &params, &1);
        client.execute_action(&1, &executor, &action, &params, &1);
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

        client.execute_action(&1, &executor, &action, &params, &1);
        client.execute_action(&1, &executor, &action, &params, &2);

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
            client.execute_action(&1, &executor, &action, &params, &i);
        }

        let result = client.execute_action(&1, &executor, &action, &params, &11);
        assert!(result > 0);
    }
}
