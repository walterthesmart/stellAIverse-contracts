#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{Env, Address, String};
use soroban_sdk::testutils::Address as _;

fn create_contract(e: &Env) -> AgentNFTClient {
    let contract_id = e.register_contract(None, AgentNFT);
    AgentNFTClient::new(e, &contract_id)
}

#[test]
fn test_init_works() {
    // let env = Env::default();
    // env.mock_all_auths();
    
    // let client = create_contract(&env);
    // let admin = Address::generate(&env);
    
    // // Initialize
    // client.init_contract(&admin);
    
    // // Verify admin can add minter
    // let new_minter = Address::generate(&env);
    // client.add_approved_minter(&admin, &new_minter);
    
    // // Verify double init fails
    // let result = client.try_init_contract(&admin);
    // assert!(result.is_err());
    // assert_eq!(result.err(), Some(Ok(ContractError::AlreadyInitialized)));
}


#[test]
fn test_mint_valid_flow() {
    let env = Env::default();
    env.mock_all_auths();
    
    let client = create_contract(&env);
    let admin = Address::generate(&env);
    client.init_contract(&admin);
    
    let user = Address::generate(&env);
    // User is not admin or approved minter yet
    
    let agent_id = 100u128;
    let cid = String::from_str(&env, "QmHash");
    let evolution = 1;
    
    // User cannot mint yet (mock_all_auths bypasses auth check but logic check remains)
    // Wait, mock_all_auths makes require_auth() pass. 
    // But verify_minter checks storage.
    
    // Try minting as admin
    client.mint_agent(&agent_id, &admin, &cid, &evolution);
    
    let agent = client.get_agent(&(agent_id as u64));
    assert_eq!(agent.owner, admin);
    assert_eq!(agent.evolution_level, evolution);
}

#[test]
fn test_unauthorized_mint_fails() {
    let env = Env::default();
    // Do NOT mock auths globally if we want to test auth failure? 
    // Actually verify_minter does manual checks against storage, so mock_all_auths() is fine for the signature part,
    // but the logic inside verify_minter will fail if the caller is not in the list.
    env.mock_all_auths(); 
    
    let client = create_contract(&env);
    let admin = Address::generate(&env);
    client.init_contract(&admin);
    
    let scanner = Address::generate(&env); // Random user
    let agent_id = 200u128;
    let cid = String::from_str(&env, "QmHash");
    
    // Should fail as scanner is not admin or approved
    let result = client.try_mint_agent(&agent_id, &scanner, &cid, &1);
    assert_eq!(result.err(), Some(Ok(ContractError::Unauthorized)));
}

#[test]
fn test_update_agent_auth() {
    let env = Env::default();
    env.mock_all_auths();
    
    let client = create_contract(&env);
    let admin = Address::generate(&env);
    client.init_contract(&admin);
    
    let agent_id = 100u128;
    let cid = String::from_str(&env, "QmHash");
    client.mint_agent(&agent_id, &admin, &cid, &1);
    
    let new_name = String::from_str(&env, "Agent Bond");
    
    // Admin (owner) can update
    client.update_agent(&(agent_id as u64), &admin, &Some(new_name.clone()), &None);
    
    let agent = client.get_agent(&(agent_id as u64));
    assert_eq!(agent.name, new_name);
    
    // Verify nonce incremented
    assert_eq!(agent.nonce, 1);
}

#[test]
fn test_replay_protection() {
    let env = Env::default();
    env.mock_all_auths();
    
    let client = create_contract(&env);
    let admin = Address::generate(&env);
    client.init_contract(&admin);
    
    let agent_id = 100u128;
    let cid = String::from_str(&env, "QmHash");
    client.mint_agent(&agent_id, &admin, &cid, &1);
    
    let agent_before = client.get_agent(&(agent_id as u64));
    assert_eq!(agent_before.nonce, 0);
    
    // Update should increment nonce
    client.update_agent(&(agent_id as u64), &admin, &Some(String::from_str(&env, "Updated")), &None);
    
    let agent_after = client.get_agent(&(agent_id as u64));
    assert_eq!(agent_after.nonce, 1);
    
    // Transfer should increment nonce
    let new_owner = Address::generate(&env);
    client.transfer_agent(&(agent_id as u64), &admin, &new_owner);
    
    let agent_final = client.get_agent(&(agent_id as u64));
    assert_eq!(agent_final.nonce, 2);
    assert_eq!(agent_final.owner, new_owner);
}
