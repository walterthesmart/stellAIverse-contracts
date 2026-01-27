#![cfg(test)]

use crate::Marketplace;
use soroban_sdk::{Address, Env, String, Vec};

struct TestSetup {
    env: Env,
    admin: Address,
    seller: Address,
    buyer: Address,
    agent_nft_contract: Address,
}

impl TestSetup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let agent_nft_contract = Address::generate(&env);

        Marketplace::init_contract(env.clone(), admin.clone());
        Marketplace::set_agent_nft_contract(env.clone(), admin.clone(), agent_nft_contract.clone());

        TestSetup {
            env,
            admin,
            seller,
            buyer,
            agent_nft_contract,
        }
    }

    fn create_mock_agent(&self, id: u64, owner: Address) -> stellai_lib::Agent {
        let agent = stellai_lib::Agent {
            id,
            owner: owner.clone(),
            name: String::from_str(&self.env, "TestAgent"),
            model_hash: String::from_str(&self.env, "hash_value"),
            capabilities: Vec::from_array(
                &self.env,
                [
                    String::from_str(&self.env, "learn"),
                    String::from_str(&self.env, "execute"),
                ],
            ),
            evolution_level: 0,
            created_at: self.env.ledger().timestamp(),
            updated_at: self.env.ledger().timestamp(),
            nonce: 0,
            escrow_locked: false,
            escrow_holder: None,
        };

        let agent_key = match id {
            1 => String::from_str(&self.env, "agent_1"),
            2 => String::from_str(&self.env, "agent_2"),
            3 => String::from_str(&self.env, "agent_3"),
            _ => String::from_str(&self.env, "agent_0"),
        };
        self.env.storage().instance().set(&agent_key, &agent);
        agent
    }
}

#[test]
fn test_full_flow_mint_list_buy_stake_evolve() {
    let setup = TestSetup::new();
    let env = &setup.env;

    // Stage 1: MINT - Create agent NFT
    let agent_id = 1u64;
    let _agent = setup.create_mock_agent(agent_id, setup.seller.clone());
    let agent_key = String::from_str(env, "agent_1");
    let stored_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(stored_agent.id, agent_id);
    assert_eq!(stored_agent.owner, setup.seller);
    assert_eq!(stored_agent.evolution_level, 0);
    assert!(!stored_agent.escrow_locked);
    assert_eq!(stored_agent.capabilities.len(), 2);

    // Stage 2: LIST - Create marketplace listing
    let price = 1000_i128;
    let listing_id =
        Marketplace::create_listing(env.clone(), agent_id, setup.seller.clone(), 0, price, None);
    assert_eq!(listing_id, 1);

    let listing_key = String::from_str(env, "listing_1");
    let listing: shared::Listing = env.storage().instance().get(&listing_key).unwrap();
    assert_eq!(listing.agent_id, agent_id);
    assert_eq!(listing.seller, setup.seller);
    assert_eq!(listing.price, price);
    assert!(listing.active);

    let locked_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert!(locked_agent.escrow_locked);
    assert_eq!(
        locked_agent.escrow_holder,
        Some(setup.agent_nft_contract.clone())
    );

    // Stage 3: BUY - Execute purchase
    let purchase_result =
        Marketplace::purchase_listing(env.clone(), listing_id, setup.buyer.clone());
    assert!(purchase_result);

    let purchased_listing: shared::Listing = env.storage().instance().get(&listing_key).unwrap();
    assert!(!purchased_listing.active);

    let buyer_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(buyer_agent.owner, setup.buyer);
    assert!(!buyer_agent.escrow_locked);

    // Stage 4: STAKE - Request evolution/upgrade
    let stake_amount = 500_i128;
    let evolution_request = shared::EvolutionRequest {
        request_id: 1,
        agent_id,
        owner: setup.buyer.clone(),
        stake_amount,
        status: shared::EvolutionStatus::Pending,
        created_at: env.ledger().timestamp(),
        completed_at: None,
    };

    let request_key = String::from_str(env, "request_1");
    env.storage()
        .instance()
        .set(&request_key, &evolution_request);

    let stored_request: shared::EvolutionRequest =
        env.storage().instance().get(&request_key).unwrap();
    assert_eq!(stored_request.status, shared::EvolutionStatus::Pending);
    assert_eq!(stored_request.stake_amount, stake_amount);

    // Stage 5: EVOLVE - Complete upgrade
    let oracle_data = shared::OracleData {
        key: String::from_str(env, "evolution_cost"),
        value: String::from_str(env, "300"),
        timestamp: env.ledger().timestamp(),
        source: String::from_str(env, "test_oracle"),
    };

    let oracle_key = String::from_str(env, "data_evolution_cost");
    env.storage().instance().set(&oracle_key, &oracle_data);

    let stored_oracle: shared::OracleData = env.storage().instance().get(&oracle_key).unwrap();
    assert_eq!(stored_oracle.key, String::from_str(env, "evolution_cost"));

    let mut completed_request = stored_request;
    completed_request.status = shared::EvolutionStatus::Completed;
    completed_request.completed_at = Some(env.ledger().timestamp());
    env.storage()
        .instance()
        .set(&request_key, &completed_request);

    let evolved_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    let mut final_agent = evolved_agent;
    final_agent.evolution_level = 1;
    final_agent.updated_at = env.ledger().timestamp();
    env.storage().instance().set(&agent_key, &final_agent);

    let final_stored_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(final_stored_agent.evolution_level, 1);
    assert_eq!(final_stored_agent.owner, setup.buyer);
    assert_eq!(final_stored_agent.id, agent_id);
    assert!(!final_stored_agent.escrow_locked);

    let final_request: shared::EvolutionRequest =
        env.storage().instance().get(&request_key).unwrap();
    assert_eq!(final_request.status, shared::EvolutionStatus::Completed);
    assert!(final_request.completed_at.is_some());
}

#[test]
fn test_multiple_agents_parallel_listings() {
    let setup = TestSetup::new();
    let env = &setup.env;

    setup.create_mock_agent(1, setup.seller.clone());
    setup.create_mock_agent(2, setup.seller.clone());
    setup.create_mock_agent(3, setup.seller.clone());

    let listing_1 =
        Marketplace::create_listing(env.clone(), 1, setup.seller.clone(), 0, 1000, None);
    let listing_2 =
        Marketplace::create_listing(env.clone(), 2, setup.seller.clone(), 0, 1500, None);
    let listing_3 =
        Marketplace::create_listing(env.clone(), 3, setup.seller.clone(), 0, 2000, None);

    assert_eq!(listing_1, 1);
    assert_eq!(listing_2, 2);
    assert_eq!(listing_3, 3);

    let result_1 = Marketplace::purchase_listing(env.clone(), listing_1, setup.buyer.clone());
    let result_2 = Marketplace::purchase_listing(env.clone(), listing_2, Address::generate(env));

    assert!(result_1);
    assert!(result_2);

    let listing_3_key = String::from_str(env, "listing_3");
    let active_listing: shared::Listing = env.storage().instance().get(&listing_3_key).unwrap();
    assert!(active_listing.active);

    let agent_1_key = String::from_str(env, "agent_1");
    let agent_1: stellai_lib::Agent = env.storage().instance().get(&agent_1_key).unwrap();
    assert_eq!(agent_1.owner, setup.buyer);
}

#[test]
fn test_escrow_security() {
    let setup = TestSetup::new();
    let env = &setup.env;

    let agent_id = 1u64;
    setup.create_mock_agent(agent_id, setup.seller.clone());

    let listing_id =
        Marketplace::create_listing(env.clone(), agent_id, setup.seller.clone(), 0, 1000, None);

    let agent_key = String::from_str(env, "agent_1");
    let locked_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();

    assert!(locked_agent.escrow_locked);
    assert_eq!(
        locked_agent.escrow_holder,
        Some(setup.agent_nft_contract.clone())
    );

    Marketplace::purchase_listing(env.clone(), listing_id, setup.buyer.clone());

    let unlocked_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert!(!unlocked_agent.escrow_locked);
    assert_eq!(unlocked_agent.escrow_holder, None);
}

#[test]
fn test_evolution_with_oracle_validation() {
    let setup = TestSetup::new();
    let env = &setup.env;

    let agent_id = 1u64;
    setup.create_mock_agent(agent_id, setup.seller.clone());

    let listing_id =
        Marketplace::create_listing(env.clone(), agent_id, setup.seller.clone(), 0, 1000, None);
    Marketplace::purchase_listing(env.clone(), listing_id, setup.buyer.clone());

    let stake_amount = 500_i128;
    let evolution_request = shared::EvolutionRequest {
        request_id: 1,
        agent_id,
        owner: setup.buyer.clone(),
        stake_amount,
        status: shared::EvolutionStatus::Pending,
        created_at: env.ledger().timestamp(),
        completed_at: None,
    };

    let request_key = String::from_str(env, "request_1");
    env.storage()
        .instance()
        .set(&request_key, &evolution_request);

    let oracle_data = shared::OracleData {
        key: String::from_str(env, "agent_evolution_price"),
        value: String::from_str(env, "250"),
        timestamp: env.ledger().timestamp(),
        source: String::from_str(env, "oracle_provider_1"),
    };

    let oracle_key = String::from_str(env, "data_agent_evolution_price");
    env.storage().instance().set(&oracle_key, &oracle_data);

    let stored_oracle: shared::OracleData = env.storage().instance().get(&oracle_key).unwrap();
    assert_eq!(
        stored_oracle.key,
        String::from_str(env, "agent_evolution_price")
    );

    let mut completed = evolution_request;
    completed.status = shared::EvolutionStatus::Completed;
    completed.completed_at = Some(env.ledger().timestamp());
    env.storage().instance().set(&request_key, &completed);

    let agent_key = String::from_str(env, "agent_1");
    let mut agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    agent.evolution_level = 1;
    agent.updated_at = env.ledger().timestamp();
    env.storage().instance().set(&agent_key, &agent);

    let final_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    let final_request: shared::EvolutionRequest =
        env.storage().instance().get(&request_key).unwrap();

    assert_eq!(final_agent.evolution_level, 1);
    assert_eq!(final_request.status, shared::EvolutionStatus::Completed);
}

#[test]
fn test_state_assertions() {
    let setup = TestSetup::new();
    let env = &setup.env;

    let agent_id = 1u64;
    let agent = setup.create_mock_agent(agent_id, setup.seller.clone());

    assert_eq!(agent.evolution_level, 0);
    assert!(!agent.escrow_locked);

    let listing_id =
        Marketplace::create_listing(env.clone(), agent_id, setup.seller.clone(), 0, 1000, None);
    let agent_key = String::from_str(env, "agent_1");
    let listed_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert!(listed_agent.escrow_locked);

    Marketplace::purchase_listing(env.clone(), listing_id, setup.buyer.clone());
    let purchased_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert!(!purchased_agent.escrow_locked);
    assert_eq!(purchased_agent.owner, setup.buyer);

    let evolution_request = shared::EvolutionRequest {
        request_id: 1,
        agent_id,
        owner: setup.buyer.clone(),
        stake_amount: 500,
        status: shared::EvolutionStatus::Completed,
        created_at: env.ledger().timestamp(),
        completed_at: Some(env.ledger().timestamp()),
    };

    let request_key = String::from_str(env, "request_1");
    env.storage()
        .instance()
        .set(&request_key, &evolution_request);

    let mut evolved_agent = purchased_agent;
    evolved_agent.evolution_level = 1;
    env.storage().instance().set(&agent_key, &evolved_agent);

    let final_agent: stellai_lib::Agent = env.storage().instance().get(&agent_key).unwrap();
    assert_eq!(final_agent.evolution_level, 1);
}
