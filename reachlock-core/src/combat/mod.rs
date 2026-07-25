//! Space combat (S19, spec §22 "Space Combat — Star Fox 64").
//!
//! Everything gameplay-visible lives here as pure, fixed-point computation:
//! enemy behavior trees (`behavior`), the damage/shield/subsystem model
//! (`damage`), and seeded encounter generation (`encounter`). No LLM — the
//! spec is explicit that enemies are behavior trees, not crew. The client
//! renders Intents and routes collisions through `apply_hit`; it never
//! invents combat math of its own.

pub mod behavior;
pub mod damage;
pub mod encounter;
pub mod humanoid;
pub mod location;
pub mod melee;

pub use behavior::{enemy_step, BehaviorState, Intent, Senses};
pub use damage::{
    apply_hit, CombatVessel, DamageResult, SubsystemKind, SubsystemState, WeaponKind, WeaponStats,
};
pub use encounter::{generate_encounters, EncounterSpawn, EnemyClass};
pub use humanoid::{
    humanoid_step, AttackWindow, BlockWindow, DodgeWindow, HostileArchetype, HumanoidIntent,
    HumanoidSenses, HumanoidState,
};
pub use location::{HostileLocation, HostileProp, HostileRoom, HostileSpawn, Keycard};
pub use melee::{block_reduce, in_melee_arc, is_dodging};
