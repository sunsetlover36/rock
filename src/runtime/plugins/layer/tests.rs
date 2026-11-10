use std::{collections::HashMap, rc::Rc};

use mlua::Lua;

use super::*;
use crate::runtime::{
    EventBus, app_data,
    plugins::{EntityPlugin, OnPlugin, on::event_descriptors::GLOBAL_EVENT_DESCRIPTORS},
};

fn lua() -> Lua {
    let lua = Lua::new();
    lua.set_app_data(app_data::LayerRegistry::new());
    lua.set_app_data(app_data::ActiveLayers(Vec::new()));
    lua.set_app_data(app_data::World(hecs::World::new()));
    lua.set_app_data(app_data::EntityCustoms(HashMap::new()));
    lua.set_app_data(app_data::BlueprintRegistry::new());
    lua.set_app_data(crate::runtime::network_replicator::FieldRegistry::new(&lua).unwrap());
    lua.set_app_data(app_data::EventListeners(HashMap::new()));
    lua.set_app_data(app_data::EventBus(Rc::new(EventBus::new())));
    lua.set_app_data(app_data::ExecutionContext::Global);
    for plugin in [
        Box::new(LayerPlugin {}) as Box<dyn GameModePlugin>,
        Box::new(EntityPlugin {}),
        Box::new(OnPlugin {
            descriptors: GLOBAL_EVENT_DESCRIPTORS,
        }),
    ] {
        lua.globals()
            .set(
                plugin.name().to_string(),
                plugin.create_api(&lua).unwrap().unwrap(),
            )
            .unwrap();
    }
    lua
}

fn assert_empty(lua: &Lua) {
    let registry = lua.app_data_ref::<app_data::LayerRegistry>().unwrap();
    assert!(registry.layers.is_empty());
    assert!(registry.aliases.is_empty());
    assert!(
        lua.app_data_ref::<app_data::ActiveLayers>()
            .unwrap()
            .0
            .is_empty()
    );
    assert!(lua.app_data_ref::<app_data::World>().unwrap().0.is_empty());
    assert!(
        lua.app_data_ref::<app_data::EntityCustoms>()
            .unwrap()
            .0
            .is_empty()
    );
    assert!(
        lua.app_data_ref::<app_data::EventListeners>()
            .unwrap()
            .0
            .values()
            .all(Vec::is_empty)
    );
}

#[test]
fn clears_resources_and_cleanup_once() {
    let lua = lua();
    lua.load(
        r#"
        cleaned = 0
        local world = layer.create():name("world"):with(function()
            entity.blueprint():spawn()
            on.world.awake():each(function() end)
            return function() cleaned = cleaned + 1 end
        end):commit()
        world:clear()
        world:clear()
        assert(cleaned == 1)
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn initialization_failure_rolls_back_resources_and_name() {
    let lua = lua();
    let err = lua
        .load(
            r#"
        cleaned = false
        layer.create():name("world")
          :with(function()
              entity.blueprint():spawn()
              on.world.awake():each(function() end)
              return function() cleaned = true end
          end)
          :with(function() error("initialization failed") end)
          :commit()
    "#,
        )
        .exec()
        .unwrap_err();
    assert!(err.to_string().contains("initialization failed"));
    assert!(lua.globals().get::<bool>("cleaned").unwrap());
    assert_empty(&lua);
    lua.load("layer.create():name('world'):commit():clear()")
        .exec()
        .unwrap();
    assert_empty(&lua);
}

#[test]
fn invalid_cleanup_return_also_rolls_back() {
    let lua = lua();
    assert!(
        lua.load(
            r#"
        layer.create():with(function()
            entity.blueprint():spawn()
            on.world.awake():each(function() end)
            return 123
        end):commit()
    "#
        )
        .exec()
        .is_err()
    );
    assert_empty(&lua);
}

#[test]
fn cleanup_errors_do_not_interrupt_remaining_resources_or_callbacks() {
    let lua = lua();
    lua.load(
        r#"
        order = {}
        local world = layer.create():name("world")
          :with(function()
              entity.blueprint():spawn()
              on.world.awake():each(function() end)
              return function() table.insert(order, 1) end
          end)
          :with(function()
              return function()
                  table.insert(order, 2)
                  error("second cleanup failed")
              end
          end)
          :with(function()
              return function()
                  table.insert(order, 3)
                  error("third cleanup failed")
              end
          end):commit()
        local ok, err = pcall(function() world:clear() end)
        assert(not ok)
        assert(tostring(err):find("second cleanup failed", 1, true))
        assert(tostring(err):find("third cleanup failed", 1, true))
        assert(table.concat(order, ",") == "3,2,1")
        world:clear()
        layer.create():name("world"):commit():clear()
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn rollback_reports_original_and_cleanup_errors() {
    let lua = lua();
    let err = lua
        .load(
            r#"
        layer.create()
          :with(function()
              entity.blueprint():spawn()
              return function() error("cleanup failed") end
          end)
          :with(function() error("setup failed") end)
          :commit()
    "#,
        )
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains("setup failed"));
    assert!(err.contains("cleanup failed"));
    assert_empty(&lua);
}

#[test]
fn nested_layers_clear_children_before_parent() {
    let lua = lua();
    lua.load(
        r#"
        order = {}
        local world = layer.create():name("world"):with(function()
            entity.blueprint():spawn()
            child = layer.create():name("child"):with(function()
                entity.blueprint():spawn()
                on.world.awake():each(function() end)
                layer.create():name("grandchild"):with(function()
                    entity.blueprint():spawn()
                    return function() table.insert(order, "grandchild") end
                end):commit()
                return function() table.insert(order, "child") end
            end):commit()
            return function() table.insert(order, "world") end
        end):commit()
        world:clear()
        child:clear()
        assert(table.concat(order, ",") == "grandchild,child,world")
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn failed_parent_rolls_back_successfully_committed_children() {
    let lua = lua();
    assert!(
        lua.load(
            r#"
        layer.create():name("parent"):with(function()
            layer.create():name("child"):with(function()
                entity.blueprint():spawn()
                on.world.awake():each(function() end)
            end):commit()
            error("parent failed")
        end):commit()
    "#
        )
        .exec()
        .is_err()
    );
    assert_empty(&lua);
}

#[test]
fn child_cleanup_error_does_not_interrupt_siblings_or_parent() {
    let lua = lua();
    lua.load(
        r#"
        order = {}
        local world = layer.create():with(function()
            layer.create():with(function()
                entity.blueprint():spawn()
                return function() table.insert(order, "first") end
            end):commit()
            layer.create():with(function()
                on.world.awake():each(function() end)
                return function()
                    table.insert(order, "second")
                    error("child cleanup failed")
                end
            end):commit()
            return function() table.insert(order, "parent") end
        end):commit()
        local ok, err = pcall(function() world:clear() end)
        assert(not ok and tostring(err):find("child cleanup failed", 1, true))
        assert(table.concat(order, ",") == "second,first,parent")
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn clearing_child_detaches_it_without_clearing_parent() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():name("parent"):with(function()
            entity.blueprint():spawn()
            child = layer.create():name("child"):with(function()
                entity.blueprint():spawn()
            end):commit()
        end):commit()
        child:clear()
    "#,
    )
    .exec()
    .unwrap();
    {
        let registry = lua.app_data_ref::<app_data::LayerRegistry>().unwrap();
        assert_eq!(registry.layers.len(), 1);
        assert!(registry.layers.values().next().unwrap().children.is_empty());
        assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    }
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn failed_child_restores_parent_context_when_error_is_caught() {
    let lua = lua();
    lua.load(
        r#"
        local world = layer.create():with(function()
            local ok = pcall(function()
                layer.create():name("failed"):with(function()
                    entity.blueprint():spawn()
                    error("child failed")
                end):commit()
            end)
            assert(not ok)
            entity.blueprint():spawn()
            on.world.awake():each(function() end)
        end):commit()
        world:clear()
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn builder_can_only_initialize_once_including_copies_and_cleared_layers() {
    let lua = lua();
    lua.load(
        r#"
        calls = 0
        local builder = layer.create():with(function()
            calls = calls + 1
            entity.blueprint():spawn()
        end)
        local copy = builder:name("copy")
        local handle = builder:commit()
        assert(not pcall(function() builder:commit() end))
        assert(not pcall(function() copy:commit() end))
        assert(calls == 1)
        handle:clear()
        assert(not pcall(function() builder:commit() end))
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn failed_builder_is_not_reusable() {
    let lua = lua();
    lua.load(
        r#"
        local calls = 0
        local builder = layer.create():with(function()
            calls = calls + 1
            error("failed")
        end)
        assert(not pcall(function() builder:commit() end))
        assert(not pcall(function() builder:commit() end))
        assert(calls == 1)
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn name_collision_does_not_run_callbacks_or_affect_existing_layer() {
    let lua = lua();
    lua.load(
        r#"
        local first = layer.create():name("world"):with(function()
            entity.blueprint():spawn()
        end):commit()
        local calls = 0
        local other = layer.create():name("world"):with(function() calls = calls + 1 end)
        assert(not pcall(function() other:commit() end))
        assert(calls == 0)
        first:clear()
        other:commit():clear()
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn recursive_clear_is_harmless_and_does_not_remove_a_replacement_name() {
    let lua = lua();
    lua.load(
        r#"
        local world
        local replacement
        world = layer.create():name("world"):with(function()
            return function()
                world:clear()
                replacement = layer.create():name("world"):with(function()
                    entity.blueprint():spawn()
                end):commit()
            end
        end):commit()
        world:clear()
        world:clear()
        replacement:clear()
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn cannot_clear_an_initializing_layer_or_ancestor() {
    let lua = lua();
    let err = lua
        .load(
            r#"
        layer.create():name("parent"):with(function()
            layer.create():with(function()
                entity.blueprint():spawn()
                layer.clear("parent")
            end):commit()
        end):commit()
    "#,
        )
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains("while it is initializing"));
    assert_empty(&lua);
}

#[test]
fn reentrant_commit_cannot_initialize_twice() {
    let lua = lua();
    lua.load(
        r#"
        local builder
        local calls = 0
        builder = layer.create():with(function()
            calls = calls + 1
            assert(not pcall(function() builder:commit() end))
        end)
        builder:commit():clear()
        assert(calls == 1)
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}

#[test]
fn clear_and_rollback_release_entity_custom_data() {
    let lua = lua();
    lua.load(
        r#"
        local world = layer.create():with(function()
            entity.blueprint():custom({ health = 100 }):spawn()
        end):commit()
        world:clear()
        assert(not pcall(function()
            layer.create():with(function()
                entity.blueprint():custom({ health = 50 }):spawn()
                error("failed after spawn")
            end):commit()
        end))
    "#,
    )
    .exec()
    .unwrap();
    assert_empty(&lua);
}
