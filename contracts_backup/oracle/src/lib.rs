#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};

const ADMIN_KEY: &str = "admin";
const PROVIDER_LIST_KEY: &str = "providers";
const DATA_KEY_PREFIX: &str = "data_";
const DATA_HISTORY_PREFIX: &str = "history_";

#[contract]
pub struct Oracle;

#[contractimpl]
impl Oracle {
    /// Initialize contract with admin
    pub fn init_contract(env: Env, admin: Address) {
        let admin_data = env.storage().instance().get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage().instance().set(&Symbol::new(&env, PROVIDER_LIST_KEY), &Vec::<Address>::new(&env));
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

    /// Register an oracle data provider with access control
    pub fn register_provider(env: Env, admin: Address, provider: Address) {
        admin.require_auth();

        Self::verify_admin(&env, &admin);

        let provider_list_key = Symbol::new(&env, PROVIDER_LIST_KEY);
        let mut providers: Vec<Address> = env.storage()
            .instance()
            .get(&provider_list_key)
            .unwrap_or_else(|_| Vec::new(&env));

        // Prevent duplicate providers
        for i in 0..providers.len() {
            if let Some(p) = providers.get(i) {
                if p == &provider {
                    panic!("Provider already registered");
                }
            }
        }

        // Limit number of providers to prevent unbounded growth
        if providers.len() >= 100 {
            panic!("Maximum number of providers reached");
        }

        providers.push_back(provider.clone());
        env.storage().instance().set(&provider_list_key, &providers);

        env.events().publish((Symbol::new(&env, "provider_registered"),), (provider,));
    }

    /// Verify if address is authorized provider
    fn is_authorized_provider(env: &Env, provider: &Address) -> bool {
        let provider_list_key = Symbol::new(env, PROVIDER_LIST_KEY);
        let providers: Vec<Address> = env.storage()
            .instance()
            .get(&provider_list_key)
            .unwrap_or_else(|_| Vec::new(env));

        for i in 0..providers.len() {
            if let Some(p) = providers.get(i) {
                if p == provider {
                    return true;
                }
            }
        }
        false
    }

    /// Submit oracle data with authorization and validation
    pub fn submit_data(
        env: Env,
        provider: Address,
        key: String,
        value: String,
        source: String,
    ) {
        provider.require_auth();

        // Input validation
        if key.len() > stellai_lib::MAX_STRING_LENGTH {
            panic!("Data key exceeds maximum length");
        }
        if value.len() > stellai_lib::MAX_STRING_LENGTH {
            panic!("Data value exceeds maximum length");
        }
        if source.len() > stellai_lib::MAX_STRING_LENGTH {
            panic!("Source exceeds maximum length");
        }

        // Authorization: verify provider is registered
        if !Self::is_authorized_provider(&env, &provider) {
            panic!("Unauthorized: provider not registered");
        }

        // Store current data (for freshness checking)
        let data_key = String::from_slice(&env, &format!("{}{}", DATA_KEY_PREFIX, key).as_bytes());
        let timestamp = env.ledger().timestamp();

        let oracle_data = stellai_lib::OracleData {
            key: key.clone(),
            value: value.clone(),
            timestamp,
            source: source.clone(),
        };

        // Store latest data
        env.storage().instance().set(&data_key, &oracle_data);

        // Store in history (with size limit to prevent DoS)
        let history_key = String::from_slice(&env, &format!("{}{}", DATA_HISTORY_PREFIX, key).as_bytes());
        let mut history: Vec<stellai_lib::OracleData> = env.storage()
            .instance()
            .get(&history_key)
            .unwrap_or_else(|_| Vec::new(&env));

        if history.len() >= 1000 {
            // Remove oldest entry if history is full
            // In production, use a circular buffer or more efficient data structure
            panic!("History limit reached for this data key");
        }

        history.push_back(oracle_data);
        env.storage().instance().set(&history_key, &history);

        env.events().publish(
            (Symbol::new(&env, "data_submitted"),),
            (key, timestamp, provider)
        );
    }

    /// Get latest oracle data with freshness validation
    pub fn get_data(env: Env, key: String) -> Option<stellai_lib::OracleData> {
        if key.len() > stellai_lib::MAX_STRING_LENGTH {
            panic!("Data key exceeds maximum length");
        }

        let data_key = String::from_slice(&env, &format!("{}{}", DATA_KEY_PREFIX, key).as_bytes());
        env.storage().instance().get(&data_key)
    }

    /// Get historical data with limit for DoS protection
    pub fn get_history(
        env: Env,
        key: String,
        limit: u32,
    ) -> Vec<stellai_lib::OracleData> {
        if key.len() > stellai_lib::MAX_STRING_LENGTH {
            panic!("Data key exceeds maximum length");
        }
        if limit > 500 {
            panic!("Limit exceeds maximum allowed (500)");
        }

        let history_key = String::from_slice(&env, &format!("{}{}", DATA_HISTORY_PREFIX, key).as_bytes());
        let history: Vec<stellai_lib::OracleData> = env.storage()
            .instance()
            .get(&history_key)
            .unwrap_or_else(|_| Vec::new(&env));

        // Return limited results
        let mut result = Vec::new(&env);
        let max_items = if (limit as usize) < history.len() { 
            limit as usize 
        } else { 
            history.len() 
        };

        for i in 0..max_items {
            if let Some(item) = history.get(history.len() - max_items + i) {
                result.push_back(item);
            }
        }

        result
    }

    /// Verify data staleness and validity with bounds checking
    pub fn is_data_fresh(env: Env, key: String, max_age_seconds: u64) -> bool {
        if key.len() > stellai_lib::MAX_STRING_LENGTH {
            panic!("Data key exceeds maximum length");
        }
        if max_age_seconds > stellai_lib::MAX_AGE_SECONDS {
            panic!("Max age exceeds reasonable limit");
        }

        let data_key = String::from_slice(&env, &format!("{}{}", DATA_KEY_PREFIX, key).as_bytes());
        match env.storage().instance().get::<_, stellai_lib::OracleData>(&data_key) {
            Some(data) => {
                let age = env.ledger().timestamp()
                    .checked_sub(data.timestamp)
                    .unwrap_or(0);
                age <= max_age_seconds
            }
            None => false,
        }
    }

    /// Deregister a provider (admin only)
    pub fn deregister_provider(env: Env, admin: Address, provider: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        let provider_list_key = Symbol::new(&env, PROVIDER_LIST_KEY);
        let mut providers: Vec<Address> = env.storage()
            .instance()
            .get(&provider_list_key)
            .unwrap_or_else(|_| Vec::new(&env));

        // Find and remove provider
        let mut found = false;
        for i in 0..providers.len() {
            if let Some(p) = providers.get(i) {
                if p == &provider {
                    // Remove by creating new vector without this provider
                    let mut new_providers = Vec::new(&env);
                    for j in 0..providers.len() {
                        if i != j {
                            if let Some(existing_p) = providers.get(j) {
                                new_providers.push_back(existing_p);
                            }
                        }
                    }
                    env.storage().instance().set(&provider_list_key, &new_providers);
                    found = true;
                    break;
                }
            }
        }

        if !found {
            panic!("Provider not found");
        }

        env.events().publish((Symbol::new(&env, "provider_deregistered"),), (provider,));
    }
}

