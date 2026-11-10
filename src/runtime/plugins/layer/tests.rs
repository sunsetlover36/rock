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

fn fire(lua: &Lua, name: &str) -> color_eyre::eyre::Result<()> {
    use crate::runtime::plugins::on::protocol::{
        EventScope, GameModeEvent, GameModeEventData, WorldEventData,
    };
    let bus = lua.app_data_ref::<app_data::EventBus>().unwrap().0.clone();
    bus.schedule_event(GameModeEvent {
        scopes: smallvec::smallvec![EventScope::Global],
        data: GameModeEventData::World(WorldEventData::Impromptu {
            name: Some(name.into()),
        }),
    });
    bus.flush(lua)
}

#[test]
fn callback_resources_and_new_subscriptions_belong_to_the_listener_layer() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():take(1):each(function()
                entity.blueprint():custom({ health = 100 }):spawn()
                on.world.awake():each(function() end)
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "spawn").unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn layers_committed_from_callbacks_are_children_of_the_listener_owner() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():name("world"):with(function()
            on.world.impromptu():each(function()
                layer.create():name("late-child"):with(function()
                    entity.blueprint():spawn()
                end):commit()
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "spawn").unwrap();
    {
        let registry = lua.app_data_ref::<app_data::LayerRegistry>().unwrap();
        let parent = registry.aliases["world"];
        let child = registry.aliases["late-child"];
        assert_eq!(registry.layers[&parent].children, [child]);
        assert_eq!(registry.layers[&child].parent, Some(parent));
    }
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn where_and_select_run_in_owner_context_without_borrowing_the_listener_registry() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu()
              :where(function()
                  entity.blueprint():spawn()
                  on.world.awake():each(function() end)
                  return true
              end)
              :select(function(name)
                  entity.blueprint():spawn()
                  on.world.impromptu():each(function() end)
                  return name
              end)
              :each(function() end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "spawn").unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 2);
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn clearing_layer_suppresses_prepared_handlers_even_when_take_already_completed() {
    let lua = lua();
    lua.load(
        r#"
        calls = 0
        local killer = on.world.impromptu():priority(100):each(function()
            world:clear()
            replacement = layer.create():name("world"):commit()
        end)
        world = layer.create():name("world"):with(function()
            on.world.impromptu():take(1):each(function()
                calls = calls + 1
                entity.blueprint():spawn()
            end)
        end):commit()
        killer_handle = killer
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "clear").unwrap();
    lua.load("assert(calls == 0); replacement:clear(); killer_handle:off()")
        .exec()
        .unwrap();
    assert_empty(&lua);
}

#[test]
fn listener_off_also_suppresses_a_prepared_final_handler() {
    let lua = lua();
    lua.load(
        r#"
        calls = 0
        killer = on.world.impromptu():priority(100):each(function() victim:off() end)
        victim = on.world.impromptu():take(1):each(function() calls = calls + 1 end)
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "off").unwrap();
    lua.load("assert(calls == 0); killer:off()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn clearing_owner_inside_filter_stops_remaining_operators_and_handler() {
    let lua = lua();
    lua.load(
        r#"
        calls = 0
        world = layer.create():with(function()
            on.world.impromptu():priority(100)
              :where(function() world:clear(); return true end)
              :select(function() error("map after cancellation") end)
              :each(function() error("handler after cancellation") end)
            on.world.impromptu()
              :where(function() error("filter after cancellation") end)
              :each(function() calls = calls + 1 end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "clear").unwrap();
    assert_empty(&lua);
}

#[test]
fn callback_can_clear_its_owner_but_cannot_create_orphaned_resources_afterwards() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():each(function()
                entity.blueprint():spawn()
                world:clear()
                local ok, err = pcall(function() entity.blueprint():spawn() end)
                assert(not ok and tostring(err):find("owning layer has been cleared", 1, true))
                assert(not pcall(function() on.world.awake():each(function() end) end))
                assert(not pcall(function() layer.create():commit() end))
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "clear").unwrap();
    assert_empty(&lua);
}

#[test]
fn clearing_owner_does_not_cancel_other_layers_or_global_listeners() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():priority(100):each(function() world:clear() end)
        end):commit()
        other = layer.create():with(function()
            on.world.impromptu():each(function() entity.blueprint():spawn() end)
        end):commit()
        global = on.world.impromptu():each(function() unowned = entity.blueprint():spawn() end)
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "clear").unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 2);
    lua.load("other:clear()").exec().unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    lua.load("global:off(); unowned:despawn()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn callback_error_restores_context_without_rolling_back_live_layer() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():take(1):each(function()
                entity.blueprint():spawn()
                error("callback failed")
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    assert!(
        fire(&lua, "fail")
            .unwrap_err()
            .to_string()
            .contains("event listener")
    );
    assert!(
        lua.app_data_ref::<app_data::ActiveLayers>()
            .unwrap()
            .0
            .is_empty()
    );
    lua.load("unowned = entity.blueprint():spawn(); world:clear()")
        .exec()
        .unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    lua.load("unowned:despawn()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn operator_error_restores_context_and_resources_remain_owned() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():where(function()
                entity.blueprint():spawn()
                error("operator failed")
            end):each(function() end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    assert!(fire(&lua, "fail").is_err());
    assert!(
        lua.app_data_ref::<app_data::ActiveLayers>()
            .unwrap()
            .0
            .is_empty()
    );
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn live_parent_cannot_be_cleared_while_late_child_is_initializing() {
    let lua = lua();
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():each(function()
                local ok, err = pcall(function()
                    layer.create():with(function()
                        entity.blueprint():spawn()
                        world:clear()
                    end):commit()
                end)
                assert(not ok and tostring(err):find("initializing descendant", 1, true))
                entity.blueprint():spawn()
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "child").unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn execution_context_is_inherited_and_restored_for_late_subscriptions() {
    let lua = lua();
    lua.set_app_data(app_data::ExecutionContext::Impromptu);
    lua.load(
        r#"
        world = layer.create():with(function()
            on.world.impromptu():take(1):each(function()
                on.world.awake():each(function() end)
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    lua.set_app_data(app_data::ExecutionContext::Global);
    fire(&lua, "subscribe").unwrap();
    assert!(matches!(
        *lua.app_data_ref::<app_data::ExecutionContext>().unwrap(),
        app_data::ExecutionContext::Global
    ));
    {
        let listeners = lua.app_data_ref::<app_data::EventListeners>().unwrap();
        let contexts = listeners
            .0
            .values()
            .flatten()
            .map(|l| l.context)
            .collect::<Vec<_>>();
        assert_eq!(contexts.len(), 1);
        assert!(matches!(contexts[0], app_data::ExecutionContext::Impromptu));
    }
    lua.load("world:clear()").exec().unwrap();
    assert_empty(&lua);
}

fn enable_dispatch(lua: &Lua) {
    lua.globals()
        .set(
            "dispatch",
            lua.create_function(|lua, name: String| {
                fire(lua, &name).map_err(|err| mlua::Error::runtime(err.to_string()))
            })
            .unwrap(),
        )
        .unwrap();
}

#[test]
fn unowned_listener_does_not_inherit_the_dispatching_layers_context() {
    let lua = lua();
    enable_dispatch(&lua);
    lua.load(
        r#"
        local global = on.world.impromptu():take(1):each(function()
            unowned = entity.blueprint():spawn()
        end)
        local world = layer.create():with(function()
            dispatch("inside-initialization")
            entity.blueprint():spawn()
        end):commit()
        world:clear()
    "#,
    )
    .exec()
    .unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    lua.load("unowned:despawn()").exec().unwrap();
    assert_empty(&lua);
}

#[test]
fn nested_dispatch_restores_the_outer_listeners_owner() {
    let lua = lua();
    enable_dispatch(&lua);
    lua.load(
        r#"
        outer = layer.create():with(function()
            on.world.impromptu():where(function(name) return name == "outer" end):each(function()
                dispatch("inner")
                entity.blueprint():spawn()
            end)
        end):commit()
        inner = layer.create():with(function()
            on.world.impromptu():where(function(name) return name == "inner" end):each(function()
                entity.blueprint():spawn()
            end)
        end):commit()
    "#,
    )
    .exec()
    .unwrap();
    fire(&lua, "outer").unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 2);
    lua.load("outer:clear()").exec().unwrap();
    assert_eq!(lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    lua.load("inner:clear()").exec().unwrap();
    assert_empty(&lua);
}
