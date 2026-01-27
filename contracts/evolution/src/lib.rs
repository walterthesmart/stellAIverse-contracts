#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use stellai_lib::{EvolutionRequest, EvolutionStatus, ADMIN_KEY, REQUEST_COUNTER_KEY};

#[contract]
pub struct Evolution;

#[contractimpl]
impl Evolution {
    /// Initialize contract with admin
    pub fn init_contract(env: Env, admin: Address) {
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
    }

    /// Create an evolution request
    pub fn create_request(env: Env, agent_id: u64, owner: Address, stake_amount: i128) -> u64 {
        owner.require_auth();

        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if stake_amount <= 0 {
            panic!("Stake amount must be positive");
        }

        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, REQUEST_COUNTER_KEY))
            .unwrap_or(0);
        let request_id = counter + 1;

        let request = EvolutionRequest {
            request_id,
            agent_id,
            owner: owner.clone(),
            stake_amount,
            status: EvolutionStatus::Pending,
            created_at: env.ledger().timestamp(),
            completed_at: None,
        };

        // Use tuple as key (prefix, request_id)
        let request_key = (Symbol::new(&env, "request"), request_id);
        env.storage().instance().set(&request_key, &request);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, REQUEST_COUNTER_KEY), &request_id);

        env.events().publish(
            (Symbol::new(&env, "request_created"),),
            (request_id, agent_id, owner, stake_amount),
        );

        request_id
    }

    /// Get an evolution request
    pub fn get_request(env: Env, request_id: u64) -> Option<EvolutionRequest> {
        if request_id == 0 {
            panic!("Invalid request ID");
        }

        let request_key = (Symbol::new(&env, "request"), request_id);
        env.storage().instance().get(&request_key)
    }
}
