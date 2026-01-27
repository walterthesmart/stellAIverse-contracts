#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use stellai_lib::{Listing, ListingType, RoyaltyInfo, ADMIN_KEY, LISTING_COUNTER_KEY};

#[contract]
pub struct Marketplace;

#[contractimpl]
impl Marketplace {
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
            .set(&Symbol::new(&env, LISTING_COUNTER_KEY), &0u64);
    }

    /// Create a new listing
    pub fn create_listing(
        env: Env,
        agent_id: u64,
        seller: Address,
        listing_type: u32,
        price: i128,
    ) -> u64 {
        seller.require_auth();

        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if listing_type > 2 {
            panic!("Invalid listing type");
        }
        if price <= 0 {
            panic!("Price must be positive");
        }

        // Generate listing ID
        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, LISTING_COUNTER_KEY))
            .unwrap_or(0);
        let listing_id = counter + 1;

        let listing = Listing {
            listing_id,
            agent_id,
            seller: seller.clone(),
            price,
            listing_type: match listing_type {
                0 => ListingType::Sale,
                1 => ListingType::Lease,
                2 => ListingType::Auction,
                _ => panic!("Invalid listing type"),
            },
            active: true,
            created_at: env.ledger().timestamp(),
        };

        // Store listing using tuple key
        let listing_key = (Symbol::new(&env, "listing"), listing_id);
        env.storage().instance().set(&listing_key, &listing);

        // Update counter
        env.storage()
            .instance()
            .set(&Symbol::new(&env, LISTING_COUNTER_KEY), &listing_id);

        env.events().publish(
            (Symbol::new(&env, "listing_created"),),
            (listing_id, agent_id, seller, price),
        );

        listing_id
    }

    /// Purchase an agent
    pub fn buy_agent(env: Env, listing_id: u64, buyer: Address) {
        buyer.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }

        let listing_key = (Symbol::new(&env, "listing"), listing_id);
        let mut listing: Listing = env
            .storage()
            .instance()
            .get(&listing_key)
            .expect("Listing not found");

        if !listing.active {
            panic!("Listing is not active");
        }

        // Mark listing as inactive
        listing.active = false;
        env.storage().instance().set(&listing_key, &listing);

        env.events().publish(
            (Symbol::new(&env, "agent_sold"),),
            (listing_id, listing.agent_id, buyer),
        );
    }

    /// Cancel a listing
    pub fn cancel_listing(env: Env, listing_id: u64, seller: Address) {
        seller.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }

        let listing_key = (Symbol::new(&env, "listing"), listing_id);
        let mut listing: Listing = env
            .storage()
            .instance()
            .get(&listing_key)
            .expect("Listing not found");

        if listing.seller != seller {
            panic!("Unauthorized: only seller can cancel listing");
        }

        listing.active = false;
        env.storage().instance().set(&listing_key, &listing);

        env.events().publish(
            (Symbol::new(&env, "listing_cancelled"),),
            (listing_id, listing.agent_id, seller),
        );
    }

    /// Get a specific listing
    pub fn get_listing(env: Env, listing_id: u64) -> Option<Listing> {
        if listing_id == 0 {
            panic!("Invalid listing ID");
        }

        let listing_key = (Symbol::new(&env, "listing"), listing_id);
        env.storage().instance().get(&listing_key)
    }

    /// Set royalty info for an agent
    pub fn set_royalty(env: Env, agent_id: u64, creator: Address, recipient: Address, fee: u32) {
        creator.require_auth();

        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if fee > 10000 {
            // 100% in basis points
            panic!("Royalty fee exceeds maximum (100%)");
        }

        let royalty_info = RoyaltyInfo { recipient, fee };

        let royalty_key = (Symbol::new(&env, "royalty"), agent_id);
        env.storage().instance().set(&royalty_key, &royalty_info);

        env.events()
            .publish((Symbol::new(&env, "royalty_set"),), (agent_id, fee));
    }

    /// Get royalty info for an agent
    pub fn get_royalty(env: Env, agent_id: u64) -> Option<RoyaltyInfo> {
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }

        let royalty_key = (Symbol::new(&env, "royalty"), agent_id);
        env.storage().instance().get(&royalty_key)
    }
}
