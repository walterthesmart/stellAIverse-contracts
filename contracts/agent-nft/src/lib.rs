#![no_std]

extern crate alloc;
use alloc::format;
use soroban_sdk::{contract, contractimpl, Symbol, Address, String, Env, Vec};

const ADMIN_KEY: &str = "admin";
const AGENT_COUNTER_KEY: &str = "agent_counter";
const AGENT_KEY_PREFIX: &str = "agent_";
const AGENT_LEASE_STATUS_PREFIX: &str = "agent_lease_";

// Maximum lengths for validation
const MAX_STRING_LENGTH: usize = 256;
const MAX_CAPABILITIES: usize = 10;

// Agent data structure
use soroban_sdk::contracttype;
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    pub id: u64,
    pub owner: Address,
    pub name: String,
    pub model_hash: String,
    pub capabilities: Vec<String>,
    pub evolution_level: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub nonce: u64,
}

#[contract]
pub struct AgentNFT;

#[contractimpl]
impl AgentNFT {
    /// Initialize contract with admin (one-time setup)
    pub fn init_contract(env: Env, admin: Address) {
        // Security: Verify this is first initialization
        let admin_data = env.storage().instance().get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage().instance().set(&Symbol::new(&env, AGENT_COUNTER_KEY), &0u64);
    }

    /// Helper to get storage key for an agent
    fn get_agent_key(env: &Env, agent_id: u64) -> String {
        String::from_str(env, &format!("{}{}", AGENT_KEY_PREFIX, agent_id))
    }

    /// Helper to get storage key for agent lease status
    fn get_agent_lease_key(env: &Env, agent_id: u64) -> String {
        String::from_str(env, &format!("{}{}", AGENT_LEASE_STATUS_PREFIX, agent_id))
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

    /// Safe addition with overflow checks
    fn safe_add(a: u64, b: u64) -> u64 {
        a.checked_add(b).expect("Arithmetic overflow in safe_add")
    }

    /// Check if agent is currently leased
    fn is_agent_leased(env: &Env, agent_id: u64) -> bool {
        let lease_key = Self::get_agent_lease_key(env, agent_id);
        env.storage()
            .instance()
            .get::<_, bool>(&lease_key)
            .unwrap_or(false)
    }

    /// Set agent lease status
    fn set_agent_lease_status(env: &Env, agent_id: u64, is_leased: bool) {
        let lease_key = Self::get_agent_lease_key(env, agent_id);
        env.storage().instance().set(&lease_key, &is_leased);
    }

    /// Mint a new agent NFT with comprehensive security checks
    pub fn mint_agent(
        env: Env,
        owner: Address,
        name: String,
        model_hash: String,
        capabilities: Vec<String>,
    ) -> u64 {
        owner.require_auth();
        
        // Input validation
        if name.len() > MAX_STRING_LENGTH.try_into().unwrap() {
            panic!("Agent name exceeds maximum length");
        }
        if model_hash.len() > MAX_STRING_LENGTH.try_into().unwrap() {
            panic!("Model hash exceeds maximum length");
        }
        if capabilities.len() > MAX_CAPABILITIES.try_into().unwrap() {
            panic!("Capabilities exceed maximum allowed");
        }

        // Validate each capability string
        for i in 0..capabilities.len() {
            if let Some(cap) = capabilities.get(i) {
                if cap.len() > MAX_STRING_LENGTH.try_into().unwrap() {
                    panic!("Individual capability exceeds maximum length");
                }
            }
        }

        // Increment agent counter safely
        let counter: u64 = env.storage()
            .instance()
            .get(&Symbol::new(&env, AGENT_COUNTER_KEY))
            .unwrap_or(0);
        
        let agent_id = Self::safe_add(counter, 1);
        
        // Create agent with nonce initialized to 0 (for replay protection)
        let agent = Agent {
            id: agent_id,
            owner: owner.clone(),
            name,
            model_hash,
            capabilities,
            evolution_level: 0,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            nonce: 0,
        };

        // Store agent safely
        let key = Self::get_agent_key(&env, agent_id);
        env.storage().instance().set(&key, &agent);
        
        // Initialize lease status to false (not leased)
        Self::set_agent_lease_status(&env, agent_id, false);
        
        // Update counter
        env.storage().instance().set(&Symbol::new(&env, AGENT_COUNTER_KEY), &agent_id);

        // Emit event
        env.events().publish((Symbol::new(&env, "mint_agent"),), (agent_id, owner.clone()));

        agent_id
    }

    /// Get agent metadata with bounds checking
    pub fn get_agent(env: Env, agent_id: u64) -> Option<Agent> {
        if agent_id == 0 {
            panic!("Invalid agent ID: must be greater than 0");
        }

        let key = Self::get_agent_key(&env, agent_id);
        env.storage().instance().get::<_, Agent>(&key)
    }

    /// Update agent metadata with authorization check
    pub fn update_agent(
        env: Env,
        agent_id: u64,
        owner: Address,
        name: Option<String>,
        capabilities: Option<Vec<String>>,
    ) {
        owner.require_auth();

        if agent_id == 0 {
            panic!("Invalid agent ID: must be greater than 0");
        }

        let key = Self::get_agent_key(&env, agent_id);
        let mut agent: Agent = env.storage()
            .instance()
            .get(&key)
            .expect("Agent not found");

        // Authorization check: only owner can update
        if agent.owner != owner {
            panic!("Unauthorized: only agent owner can update");
        }

        // Check if agent is leased
        if Self::is_agent_leased(&env, agent_id) {
            panic!("Cannot update agent while it is leased");
        }

        // Update fields with validation
        if let Some(new_name) = name {
            if new_name.len() > MAX_STRING_LENGTH.try_into().unwrap() {
                panic!("Agent name exceeds maximum length");
            }
            agent.name = new_name;
        }

        if let Some(new_capabilities) = capabilities {
            if new_capabilities.len() > MAX_CAPABILITIES.try_into().unwrap() {
                panic!("Capabilities exceed maximum allowed");
            }
            for i in 0..new_capabilities.len() {
                if let Some(cap) = new_capabilities.get(i) {
                    if cap.len() > MAX_STRING_LENGTH.try_into().unwrap() {
                        panic!("Individual capability exceeds maximum length");
                    }
                }
            }
            agent.capabilities = new_capabilities;
        }

        // Increment nonce for replay protection
        agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");
        agent.updated_at = env.ledger().timestamp();

        env.storage().instance().set(&key, &agent);
        env.events().publish((Symbol::new(&env, "update_agent"),), (agent_id, owner));
    }

    /// Get total agents minted
    pub fn total_agents(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, AGENT_COUNTER_KEY))
            .unwrap_or(0)
    }

    /// Get nonce for replay protection (used by other contracts)
    pub fn get_nonce(env: Env, agent_id: u64) -> u64 {
        if agent_id == 0 {
            panic!("Invalid agent ID: must be greater than 0");
        }

        let key = Self::get_agent_key(&env, agent_id);
        env.storage()
            .instance()
            .get::<_, Agent>(&key)
            .map(|agent| agent.nonce)
            .unwrap_or(0)
    }

    /// Transfer ownership of an Agent NFT to another address
    pub fn transfer_agent(
        env: Env,
        agent_id: u64,
        from: Address,
        to: Address,
    ) {
        // Validate input
        if agent_id == 0 {
            panic!("Invalid agent ID: must be greater than 0");
        }

        // Authentication: from address must authorize the transfer
        from.require_auth();

        // Prevent transferring to the same address
        if from == to {
            panic!("Cannot transfer agent to the same address");
        }

        // Get agent key
        let key = Self::get_agent_key(&env, agent_id);

        // Fetch agent from storage
        let mut agent: Agent = env.storage()
            .instance()
            .get(&key)
            .expect("Agent not found");

        // Verify current ownership
        if agent.owner != from {
            panic!("Unauthorized: caller is not the current owner");
        }

        // Check if agent is actively leased
        if Self::is_agent_leased(&env, agent_id) {
            panic!("Cannot transfer agent while it is leased");
        }

        // Update ownership
        let previous_owner = agent.owner.clone();
        agent.owner = to.clone();
        
        // Increment nonce for replay protection
        agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");
        agent.updated_at = env.ledger().timestamp();

        // Save updated agent
        env.storage().instance().set(&key, &agent);

        // Emit transfer event
        env.events().publish(
            (Symbol::new(&env, "agent_transferred"),),
            (agent_id, previous_owner, to.clone())
        );
    }

    /// Get current owner of an agent
    pub fn get_agent_owner(env: Env, agent_id: u64) -> Option<Address> {
        if agent_id == 0 {
            panic!("Invalid agent ID: must be greater than 0");
        }

        let key = Self::get_agent_key(&env, agent_id);
        env.storage()
            .instance()
            .get::<_, Agent>(&key)
            .map(|agent| agent.owner)
    }

    /// Check if agent can be transferred (not leased and exists)
    pub fn can_transfer_agent(env: Env, agent_id: u64, caller: Address) -> bool {
        if agent_id == 0 {
            return false;
        }

        // Get agent key
        let key = Self::get_agent_key(&env, agent_id);

        // Check if agent exists
        let agent = match env.storage().instance().get::<_, Agent>(&key) {
            Some(agent) => agent,
            None => return false,
        };

        // Check ownership
        if agent.owner != caller {
            return false;
        }

        // Check lease status
        !Self::is_agent_leased(&env, agent_id)
    }

    /// Start leasing an agent
    pub fn start_lease(env: Env, agent_id: u64) {
        Self::set_agent_lease_status(&env, agent_id, true);
        
        env.events().publish(
            (Symbol::new(&env, "lease_started"),),
            (agent_id, env.ledger().timestamp())
        );
    }

    /// End leasing an agent
    pub fn end_lease(env: Env, agent_id: u64) {
        Self::set_agent_lease_status(&env, agent_id, false);
        
        env.events().publish(
            (Symbol::new(&env, "lease_ended"),),
            (agent_id, env.ledger().timestamp())
        );
    }

    /// Check if agent is leased
    pub fn is_leased(env: Env, agent_id: u64) -> bool {
        if agent_id == 0 {
            panic!("Invalid agent ID: must be greater than 0");
        }
        Self::is_agent_leased(&env, agent_id)
    }
}