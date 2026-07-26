pub mod career;
pub mod character_sprite;
pub mod charted_system;
pub mod contract;
pub mod crew_package;
pub mod dialogue;
pub mod dungeon;
pub mod economy;
pub mod ecosystem;
pub mod enemy;
pub mod event;
pub mod faction;
pub mod gate_network;
pub mod origin;
pub mod planet_culture;
// `hull` was a reference implementation for `HullConfiguration` editing,
// superseded in the registry by `hull_frame`. Removed from mod.rs (not the
// file system) because it added compile time for a module that was
// `#[allow(dead_code)]` and not registered in any dispatch path. The file
// remains on disk as a study reference.
pub mod hull_frame;
pub mod hull_mesh;
pub mod item;
pub mod item_browser;
pub mod location;
pub mod recipe;
pub mod room_templates;
pub mod scripted_encounter;
pub mod soul;
pub mod station;
pub mod storyline;
pub mod theme;
pub mod trope;
pub mod widgets;

pub fn register_all(registry: &mut super::app::EditorRegistry) {
    registry.register(super::app::ContentType::Career, career::create_editor);
    registry.register(super::app::ContentType::Ecosystem, ecosystem::create_editor);
    registry.register(super::app::ContentType::Theme, theme::create_editor);
    registry.register(super::app::ContentType::Trope, trope::create_editor);
    registry.register(
        super::app::ContentType::PlanetCulture,
        planet_culture::create_editor,
    );
    registry.register(
        super::app::ContentType::HullFrame,
        hull_frame::create_editor,
    );
    registry.register(super::app::ContentType::Station, station::create_editor);
    registry.register(super::app::ContentType::Location, location::create_editor);
    registry.register(super::app::ContentType::Soul, soul::create_editor);
    registry.register(super::app::ContentType::Contract, contract::create_editor);
    registry.register(super::app::ContentType::Faction, faction::create_editor);
    registry.register(
        super::app::ContentType::EconomyGoods,
        economy::create_editor,
    );
    registry.register(super::app::ContentType::Storyline, storyline::create_editor);
    registry.register(super::app::ContentType::Item, item::create_editor);
    registry.register(
        super::app::ContentType::EnemyArchetype,
        enemy::create_editor,
    );
    registry.register(
        super::app::ContentType::ChartedSystem,
        charted_system::create_editor,
    );
    registry.register(super::app::ContentType::HullMesh, hull_mesh::create_editor);
    registry.register(
        super::app::ContentType::RoomTemplates,
        room_templates::create_editor,
    );
    registry.register(
        super::app::ContentType::GateNetwork,
        gate_network::create_editor,
    );
    registry.register(
        super::app::ContentType::ItemBrowser,
        item_browser::create_editor,
    );
    registry.register(
        super::app::ContentType::SpriteViewer,
        character_sprite::create_editor,
    );
    registry.register(super::app::ContentType::Dungeon, dungeon::create_editor);
    registry.register(super::app::ContentType::Event, event::create_editor);
    registry.register(super::app::ContentType::Dialogue, dialogue::create_editor);
    registry.register(super::app::ContentType::Recipe, recipe::create_editor);
    registry.register(super::app::ContentType::Origin, origin::create_editor);
}

#[cfg(test)]
mod envelope_round_trip_tests {
    use super::super::app::{build_default_registry, ContentType, ROOT_LOCK};

    fn content_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("mods/reachlock")
    }

    /// Content types whose files on disk are `ContentFile` envelopes, paired
    /// with how many authored files each must have.
    const ENVELOPE_TABS: &[(ContentType, usize)] = &[
        (ContentType::Origin, 10),
        (ContentType::Soul, 15),
        (ContentType::Career, 10),
        (ContentType::Theme, 1),
        (ContentType::PlanetCulture, 1),
        (ContentType::Ecosystem, 1),
        (ContentType::Station, 1),
        (ContentType::CrewPackage, 1),
    ];

    fn ron_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ron"))
            .collect();
        paths.sort();
        paths
    }

    /// Every authored envelope file must open in the tab that edits it.
    ///
    /// Eight tabs read and wrote the bare payload while the files on disk were
    /// envelopes, so not one of these files could be opened in the editor. The
    /// content format is allowed to change; a tab silently losing the ability
    /// to read it is not.
    #[test]
    fn every_authored_envelope_file_opens_in_its_tab() {
        let _guard = ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reg = build_default_registry();
        let mut failures = Vec::new();
        let mut opened = 0usize;

        for (ct, least) in ENVELOPE_TABS {
            let dir = content_root().join(ct.directory());
            let paths = ron_files(&dir);
            assert!(
                paths.len() >= *least,
                "expected at least {least} authored file(s) in {}, found {}",
                dir.display(),
                paths.len()
            );
            for path in paths {
                let Some(mut editor) = reg.create(*ct) else {
                    failures.push(format!("{ct:?}: no editor registered"));
                    continue;
                };
                match editor.load(&path) {
                    Ok(()) => opened += 1,
                    Err(e) => failures.push(format!("{}: {e}", path.display())),
                }
            }
        }

        assert!(
            failures.is_empty(),
            "authored content the editor cannot open:\n  {}",
            failures.join("\n  ")
        );
        assert!(opened >= 38, "expected to open 38+ files, opened {opened}");
    }

    /// `storylines/` holds one storyline arc and one soul-mutation set, so it
    /// cannot go in the table above — the tab must open its own file and
    /// refuse the other one rather than silently loading an empty document.
    #[test]
    fn the_storyline_tab_opens_arcs_and_refuses_mutation_sets() {
        let _guard = ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reg = build_default_registry();
        let dir = content_root().join(ContentType::Storyline.directory());

        let mut editor = reg.create(ContentType::Storyline).expect("registered");
        editor
            .load(&dir.join("compact_arc.ron"))
            .expect("the storyline tab must open the authored arc");

        let mut editor = reg.create(ContentType::Storyline).expect("registered");
        let err = editor
            .load(&dir.join("loup_garou_souls.ron"))
            .expect_err("a soul-mutation set is not a storyline");
        assert!(
            err.contains("SoulMutations"),
            "the error should name the type actually found, got: {err}"
        );
    }

    /// Load → save → reload must come back identical, envelope included. A
    /// save that dropped `seed`, `universe` or `priority` would rewrite the
    /// author's decisions on every edit.
    #[test]
    fn saving_preserves_the_envelope_byte_for_byte() {
        let _guard = ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reg = build_default_registry();
        let tmp = std::env::temp_dir().join("reachlock_editor_envelope_round_trip");
        let _ = std::fs::create_dir_all(&tmp);

        for (ct, _) in ENVELOPE_TABS {
            let dir = content_root().join(ct.directory());
            for path in ron_files(&dir) {
                let mut editor = reg.create(*ct).expect("editor registered");
                editor.load(&path).expect("load");

                let dst = tmp.join(path.file_name().expect("file name"));
                editor.save(&dst).expect("save");

                // The re-read envelope must match the original's metadata.
                let before: reachlock_core::content::ContentFile =
                    crate::io::read_ron(&path).expect("read original");
                let after: reachlock_core::content::ContentFile =
                    crate::io::read_ron(&dst).expect("read saved");
                assert_eq!(
                    (
                        &before.id,
                        before.seed,
                        &before.universe,
                        before.priority,
                        before.asset_type
                    ),
                    (
                        &after.id,
                        after.seed,
                        &after.universe,
                        after.priority,
                        after.asset_type
                    ),
                    "envelope metadata changed when saving {}",
                    path.display()
                );
                assert_eq!(
                    before.payload,
                    after.payload,
                    "payload changed when saving {}",
                    path.display()
                );
                let _ = std::fs::remove_file(&dst);
            }
        }
        let _ = std::fs::remove_dir(&tmp);
    }
}
