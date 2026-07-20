# Adding a New ECS System

1. Create `client/src/systems/my_system.rs` with pub fns
2. Add `pub mod my_system;` to `client/src/systems/mod.rs`
3. In main.rs: register with a run condition (in_spaceflight / in_any_interior / space_live / in_state)
4. If new component/resource: define in the system file or in core if it's shared
5. Tag scene entities with ModeScope if they should despawn on mode exit
