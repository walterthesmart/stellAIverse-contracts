#![cfg(test)]

use soroban_sdk::{Address, Env, String, Vec};
use crate::Marketplace;

#[test]
fn test_basic_functionality() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup addresses
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let marketplace_address = env.current_contract_address();

    // Initialize Marketplace contract
    Marketplace::init_contract(env.clone(), admin.clone());
    Marketplace::set_agent_nft_contract(env.clone(), admin.clone(), marketplace_address.clone());

    // Create a mock agent directly in storage for testing
    let agent_id = 1u64;
    let agent_key_str = String::from_str(&env, "agent_");
    let agent = shared::Agent {
        id: agent_id,
        owner: seller.clone(),
        name: String::from_str(&env, "Test Agent"),
        model_hash: String::from_str(&env, "hash123"),
        capabilities: Vec::from_array(&env, [
            String::from_str(&env, "capability1"),
            String::from_str(&env, "capability2")
        ]),
        evolution_level: 0,
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        nonce: 0,
        escrow_locked: false,
        escrow_holder: None,
    };
    env.storage().instance().set(&agent_key_str, &agent);

    // Verify agent is not locked initially
    assert!(!agent.escrow_locked);
    assert_eq!(agent.escrow_holder, None);

    // Create listing (should lock agent in escrow)
    let listing_id = Marketplace::create_listing(
        env.clone(),
        agent_id,
        seller.clone(),
        0, // Sale type
        1000, // Price
        None, // No duration for sale
    );

    // Verify listing was created
    assert!(listing_id > 0);

    // Verify agent is now locked in escrow
    let updated_agent: shared::Agent = env.storage()
        .instance()
        .get(&agent_key_str)
        .unwrap();
    assert!(updated_agent.escrow_locked);
    assert_eq!(updated_agent.escrow_holder, Some(marketplace_address.clone()));

    // Cancel listing (should release agent from escrow)
    Marketplace::cancel_listing(env.clone(), listing_id, seller.clone());

    // Verify agent is released from escrow
    let final_agent: shared::Agent = env.storage()
        .instance()
        .get(&agent_key_str)
        .unwrap();
    assert!(!final_agent.escrow_locked);
    assert_eq!(final_agent.escrow_holder, None);
}

#[test]
fn test_ownership_validation() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup addresses
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let marketplace_address = env.current_contract_address();

    // Initialize Marketplace contract
    Marketplace::init_contract(env.clone(), admin.clone());
    Marketplace::set_agent_nft_contract(env.clone(), admin.clone(), marketplace_address.clone());

    // Create a mock agent directly in storage
    let agent_id = 1u64;
    let agent_key_str = String::from_str(&env, "agent_");
    let agent = shared::Agent {
        id: agent_id,
        owner: owner.clone(),
        name: String::from_str(&env, "Test Agent"),
        model_hash: String::from_str(&env, "hash123"),
        capabilities: Vec::from_array(&env, [String::from_str(&env, "capability1")]),
        evolution_level: 0,
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        nonce: 0,
        escrow_locked: false,
        escrow_holder: None,
    };
    env.storage().instance().set(&agent_key_str, &agent);

    // Create listing as owner (should succeed)
    let listing_id = Marketplace::create_listing(
        env.clone(),
        agent_id,
        owner.clone(),
        0, // Sale type
        1000, // Price
        None, // No duration
    );

    assert!(listing_id > 0);
}

#[test]
fn test_royalty_functionality() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup addresses
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);
    let marketplace_address = env.current_contract_address();

    // Initialize Marketplace contract
    Marketplace::init_contract(env.clone(), admin.clone());
    Marketplace::set_agent_nft_contract(env.clone(), admin.clone(), marketplace_address.clone());

    // Create a mock agent directly in storage
    let agent_id = 1u64;
    let agent_key_str = String::from_str(&env, "agent_");
    let agent = shared::Agent {
        id: agent_id,
        owner: creator.clone(),
        name: String::from_str(&env, "Test Agent"),
        model_hash: String::from_str(&env, "hash123"),
        capabilities: Vec::from_array(&env, [String::from_str(&env, "capability1")]),
        evolution_level: 0,
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
        nonce: 0,
        escrow_locked: false,
        escrow_holder: None,
    };
    env.storage().instance().set(&agent_key_str, &agent);

    // Set royalty (should succeed)
    Marketplace::set_royalty(
        env.clone(),
        agent_id,
        creator.clone(),
        royalty_recipient.clone(),
        500, // 5% royalty
    );

    // Get royalty info
    let royalty_info = Marketplace::get_royalty(env.clone(), agent_id);
    assert!(royalty_info.is_some());
    let royalty = royalty_info.unwrap();
    assert_eq!(royalty.recipient, royalty_recipient);
    assert_eq!(royalty.percentage, 500);
}
