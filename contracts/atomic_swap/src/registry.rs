//! Local asset-commitment registry for the `atomic_swap` contract.
//!
//! Every swappable asset is registered here under a hiding commitment
//! (`sha256(secret || blinding_factor)`) chosen by its owner. This used to
//! live in a separate `ip_registry` contract that `atomic_swap` cross-called
//! on every swap; folding it into this contract's own storage removes that
//! external dependency and the extra cross-contract round trip, at the cost
//! of the licensing/staking/dispute machinery the old registry also carried
//! (out of scope for a generic swap contract).
//!
//! Two guarantees are enforced from here:
//!
//! * `ensure_seller_owns_active_asset` — the caller initiating a swap still
//!   owns the asset and hasn't revoked it.
//! * `verify_commitment` — the secret/blinding factor revealed at swap
//!   completion actually opens the commitment made at registration time.

use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env};

use crate::{utils::panic_with_error, ContractError, DataKey, LEDGER_BUMP};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AssetCommitment {
    pub owner: Address,
    pub commitment_hash: BytesN<32>,
    pub revoked: bool,
}

fn load(env: &Env, asset_id: u64) -> AssetCommitment {
    env.storage()
        .persistent()
        .get(&DataKey::AssetCommitment(asset_id))
        .unwrap_or_else(|| panic_with_error(env, ContractError::AssetNotFound))
}

/// Registers a new asset commitment for `owner` and returns its `asset_id`.
/// `commitment_hash` must be `sha256(secret || blinding_factor)` for a
/// `secret`/`blinding_factor` pair only `owner` knows — that pair is what
/// `reveal_key` later checks against in `verify_commitment`.
pub fn register_asset(env: &Env, owner: &Address, commitment_hash: BytesN<32>) -> u64 {
    owner.require_auth();

    // Reject a commitment hash that's already registered — reusing one would
    // let a single secret/blinding-factor reveal open two unrelated assets.
    if env
        .storage()
        .persistent()
        .has(&DataKey::CommitmentUsed(commitment_hash.clone()))
    {
        panic_with_error(env, ContractError::DuplicateCommitment);
    }
    env.storage().persistent().set(
        &DataKey::CommitmentUsed(commitment_hash.clone()),
        &true,
    );

    let asset_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::NextAssetId)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::NextAssetId, &(asset_id + 1));

    env.storage().persistent().set(
        &DataKey::AssetCommitment(asset_id),
        &AssetCommitment {
            owner: owner.clone(),
            commitment_hash,
            revoked: false,
        },
    );
    env.storage().persistent().extend_ttl(
        &DataKey::AssetCommitment(asset_id),
        LEDGER_BUMP,
        LEDGER_BUMP,
    );

    asset_id
}

/// Revokes an asset commitment. Only its registered owner may revoke it;
/// once revoked, `ensure_seller_owns_active_asset` rejects new swaps against it.
pub fn revoke_asset(env: &Env, owner: &Address, asset_id: u64) {
    owner.require_auth();

    let mut record = load(env, asset_id);
    if record.owner != *owner {
        panic_with_error(env, ContractError::Unauthorized);
    }
    record.revoked = true;

    env.storage()
        .persistent()
        .set(&DataKey::AssetCommitment(asset_id), &record);
    env.storage().persistent().extend_ttl(
        &DataKey::AssetCommitment(asset_id),
        LEDGER_BUMP,
        LEDGER_BUMP,
    );
}

/// Returns the registered commitment record for `asset_id`, panicking with
/// `AssetNotFound` if it was never registered.
pub fn get_asset(env: &Env, asset_id: u64) -> AssetCommitment {
    load(env, asset_id)
}

pub fn ensure_seller_owns_active_asset(env: &Env, asset_id: u64, seller: &Address) {
    let record = load(env, asset_id);

    if record.owner != *seller {
        panic_with_error(env, ContractError::NotAssetOwner);
    }

    if record.revoked {
        panic_with_error(env, ContractError::AssetRevoked);
    }
}

pub fn verify_commitment(
    env: &Env,
    asset_id: u64,
    secret: &BytesN<32>,
    blinding_factor: &BytesN<32>,
) -> bool {
    let record = load(env, asset_id);

    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from(secret.clone()));
    preimage.append(&Bytes::from(blinding_factor.clone()));
    let computed_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    computed_hash == record.commitment_hash
}
