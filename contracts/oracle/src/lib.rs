#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};
use stellai_lib::{OracleData, ADMIN_KEY, PROVIDER_LIST_KEY};

#[contract]
pub struct Oracle;

#[contractimpl]
impl Oracle {
    pub fn init_contract(env: Env, admin: Address) {
        let admin_data: Option<Address> =
            env.storage().instance().get(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);

        let providers: Vec<Address> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PROVIDER_LIST_KEY), &providers);
    }

    fn verify_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .unwrap_or_else(|| panic!("Contract not initialized"));

        if caller != &admin {
            panic!("Caller is not admin");
        }
    }

    fn is_authorized_provider(env: &Env, provider: &Address) -> bool {
        let providers: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(env, PROVIDER_LIST_KEY))
            .unwrap_or_else(|| Vec::new(env));

        for p in providers.iter() {
            if &p == provider {
                return true;
            }
        }
        false
    }

    pub fn register_provider(env: Env, admin: Address, provider: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        let mut providers: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PROVIDER_LIST_KEY))
            .unwrap_or_else(|| Vec::new(&env));

        for p in providers.iter() {
            if p == provider {
                panic!("Provider already registered");
            }
        }

        providers.push_back(provider.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PROVIDER_LIST_KEY), &providers);

        env.events().publish(
            (Symbol::new(&env, "provider_registered"),),
            (admin, provider),
        );
    }

    pub fn submit_data(env: Env, provider: Address, key: Symbol, value: i128) {
        provider.require_auth();

        if !Self::is_authorized_provider(&env, &provider) {
            panic!("Unauthorized: provider not registered");
        }

        let timestamp = env.ledger().timestamp();

        let oracle_data = OracleData {
            key: key.clone(),
            value,
            timestamp,
            provider: provider.clone(),
            signature: None,
            source: None,
        };

        env.storage().instance().set(&key, &oracle_data);

        env.events().publish(
            (Symbol::new(&env, "data_submitted"),),
            (key, timestamp, provider),
        );
    }

    pub fn get_data(env: Env, key: Symbol) -> Option<OracleData> {
        env.storage().instance().get(&key)
    }

    pub fn deregister_provider(env: Env, admin: Address, provider: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        let providers: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PROVIDER_LIST_KEY))
            .unwrap_or_else(|| Vec::new(&env));

        let mut updated_providers = Vec::new(&env);
        let mut found = false;

        for p in providers.iter() {
            if p != provider {
                updated_providers.push_back(p.clone());
            } else {
                found = true;
            }
        }

        if !found {
            panic!("Provider not found");
        }

        env.storage()
            .instance()
            .set(&Symbol::new(&env, PROVIDER_LIST_KEY), &updated_providers);

        env.events().publish(
            (Symbol::new(&env, "provider_deregistered"),),
            (admin, provider),
        );
    }
}
