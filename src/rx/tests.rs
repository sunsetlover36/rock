use mlua::{AnyUserData, IntoLuaMulti, Lua, MultiValue, UserData, UserDataMethods};

use super::{
    HasPipeline, RxPipeline, RxSentry, RxSentryError,
    core::{CoreSentryError, add_core_pipeline_methods},
    operator::add_op_pipeline_methods,
};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Chain(RxPipeline);
impl HasPipeline for Chain {
    fn pipeline(&self) -> &RxPipeline {
        &self.0
    }
    fn pipeline_mut(&mut self) -> &mut RxPipeline {
        &mut self.0
    }
}
impl UserData for Chain {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        add_core_pipeline_methods(methods);
        add_op_pipeline_methods(methods);
    }
}

fn lua() -> Lua {
    let lua = Lua::new();
    lua.globals().set("rx", Chain::default()).unwrap();
    lua
}

fn sentry(lua: &Lua, expression: &str) -> RxSentry {
    let builder: AnyUserData = lua.load(expression).eval().unwrap();
    RxSentry::new(builder.borrow::<Chain>().unwrap().0.clone())
}

fn run(lua: &Lua, expression: &str, values: &[i64]) -> Vec<i64> {
    let mut sentry = sentry(lua, expression);
    values
        .iter()
        .filter_map(|&n| {
            if sentry.is_exhausted() {
                return None;
            }
            sentry
                .process(n.into_lua_multi(lua).unwrap())
                .unwrap()
                .map(|args| args.front().unwrap().as_integer().unwrap())
        })
        .collect()
}

#[test]
fn filtering_and_taking_follow_written_order() {
    let lua = lua();
    assert_eq!(
        run(
            &lua,
            "rx:where(function(n) return n % 2 == 0 end):take(2)",
            &[1, 2, 3, 4, 6]
        ),
        [2, 4]
    );
    assert_eq!(
        run(
            &lua,
            "rx:take(2):where(function(n) return n % 2 == 0 end)",
            &[1, 2, 3, 4, 6]
        ),
        [2]
    );
}

#[test]
fn mapping_and_filtering_follow_written_order() {
    let lua = lua();
    assert_eq!(
        run(
            &lua,
            "rx:select(function(n) return n * 10 end):where(function(n) return n > 15 end)",
            &[1, 2, 3]
        ),
        [20, 30]
    );
    assert_eq!(
        run(
            &lua,
            "rx:where(function(n) return n > 15 end):select(function(n) return n * 10 end)",
            &[1, 2, 3]
        ),
        Vec::<i64>::new()
    );
}

#[test]
fn skip_and_take_do_not_repeat_or_add_take_limits() {
    let lua = lua();
    let values = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    assert_eq!(run(&lua, "rx:skip(2):take(1)", &values), [3]);
    assert_eq!(run(&lua, "rx:take(3):skip(1)", &values), [2, 3]);
    assert_eq!(run(&lua, "rx:take(1):take(1)", &values), [1]);
    assert_eq!(
        run(&lua, "rx:skip(1):skip(1)", &values),
        [3, 4, 5, 6, 7, 8, 9]
    );
    assert_eq!(run(&lua, "rx:take(2):skip(5)", &values), Vec::<i64>::new());
}

#[test]
fn zero_and_maximum_counts_are_safe() {
    let lua = lua();
    assert_eq!(run(&lua, "rx:take(0)", &[1, 2]), Vec::<i64>::new());
    assert_eq!(run(&lua, "rx:skip(0)", &[1, 2]), [1, 2]);
    assert_eq!(run(&lua, "rx:skip(4294967295)", &[1, 2]), Vec::<i64>::new());
    assert_eq!(run(&lua, "rx:take(4294967295)", &[1, 2]), [1, 2]);
    assert_eq!(
        run(&lua, "rx:skip(2):take(0)", &[1, 2, 3]),
        Vec::<i64>::new()
    );
}

#[test]
fn exhausted_chain_does_not_run_upstream_callbacks() {
    let lua = lua();
    lua.load("calls = 0").exec().unwrap();
    let mut chain = sentry(
        &lua,
        "rx:select(function(n) calls = calls + 1; return n end):take(1)",
    );
    chain.process(1.into_lua_multi(&lua).unwrap()).unwrap();
    assert!(chain.is_exhausted());
    assert!(matches!(
        chain.process(MultiValue::new()),
        Err(RxSentryError::Core(CoreSentryError::LimitReached(1)))
    ));
    assert_eq!(lua.globals().get::<i64>("calls").unwrap(), 1);
}

#[test]
fn downstream_filter_still_completes_an_upstream_take() {
    let lua = lua();
    let mut chain = sentry(&lua, "rx:take(1):where(function() return false end)");
    assert!(chain.process(MultiValue::new()).unwrap().is_none());
    assert!(chain.is_exhausted());
}

#[test]
fn changed_has_independent_state_at_each_position() {
    let lua = lua();
    assert_eq!(
        run(
            &lua,
            "rx:changed():select(function(n) return n * 10 end):changed()",
            &[1, 1, 2, 2, 3]
        ),
        [10, 20, 30]
    );
    assert_eq!(
        run(
            &lua,
            "rx:select(function(n) return n % 2 end):changed()",
            &[1, 3, 2, 4, 5]
        ),
        [1, 0, 1]
    );
    assert_eq!(run(&lua, "rx:changed():take(2)", &[1, 1, 2, 3]), [1, 2]);
    assert_eq!(run(&lua, "rx:take(2):changed()", &[1, 1, 2, 3]), [1]);
}

#[test]
fn changed_compares_all_arguments_and_preserves_table_identity() {
    let lua = lua();
    let mut chain = sentry(&lua, "rx:changed()");
    for (args, expected) in [((1, 2), true), ((1, 2), false), ((1, 3), true)] {
        assert_eq!(
            chain
                .process(args.into_lua_multi(&lua).unwrap())
                .unwrap()
                .is_some(),
            expected
        );
    }
    let table = lua.create_table().unwrap();
    assert!(
        chain
            .process(table.clone().into_lua_multi(&lua).unwrap())
            .unwrap()
            .is_some()
    );
    table.set("health", 90).unwrap();
    assert!(
        chain
            .process(table.into_lua_multi(&lua).unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        chain
            .process(lua.create_table().unwrap().into_lua_multi(&lua).unwrap())
            .unwrap()
            .is_some()
    );
}

#[test]
fn changed_handles_empty_arguments_and_nil() {
    let lua = lua();
    let mut chain = sentry(&lua, "rx:changed()");
    assert!(chain.process(MultiValue::new()).unwrap().is_some());
    assert!(chain.process(MultiValue::new()).unwrap().is_none());
    let nil = MultiValue::from_vec(vec![mlua::Value::Nil]);
    assert!(chain.process(nil.clone()).unwrap().is_some());
    assert!(chain.process(nil).unwrap().is_none());
}

#[test]
fn select_preserves_multiple_return_values() {
    let lua = lua();
    let mut chain = sentry(&lua, "rx:select(function(n) return n, nil, n + 1 end)");
    let args = chain
        .process(7.into_lua_multi(&lua).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(args.len(), 3);
    assert_eq!(args[0].as_integer(), Some(7));
    assert!(args[1].is_nil());
    assert_eq!(args[2].as_integer(), Some(8));
}

#[test]
fn lua_operator_errors_are_reported() {
    let lua = lua();
    for expression in [
        "rx:where(function() error('predicate failed') end)",
        "rx:select(function() error('mapper failed') end)",
    ] {
        assert!(matches!(
            sentry(&lua, expression).process(MultiValue::new()),
            Err(RxSentryError::Op(_))
        ));
    }
}

#[test]
fn builders_and_subscriptions_have_independent_state() {
    let lua = lua();
    lua.load("base = rx:skip(1); short = base:take(1)")
        .exec()
        .unwrap();
    assert_eq!(run(&lua, "base", &[1, 2, 3]), [2, 3]);
    assert_eq!(run(&lua, "short", &[1, 2, 3]), [2]);
    assert_eq!(run(&lua, "short", &[4, 5, 6]), [5]);
}

#[test]
fn invalid_counts_are_lua_errors() {
    let lua = lua();
    for op in ["take", "skip"] {
        for value in [
            "-1",
            "0.5",
            "4294967296",
            "0/0",
            "math.huge",
            "nil",
            "true",
            "'2'",
        ] {
            assert!(
                lua.load(format!("rx:{op}({value})"))
                    .eval::<AnyUserData>()
                    .is_err(),
                "{op}({value})"
            );
        }
    }
}

#[test]
fn invalid_throttle_durations_are_lua_errors() {
    let lua = lua();
    for value in ["-1", "0/0", "math.huge", "-math.huge", "1e30"] {
        let err = lua
            .load(format!("rx:throttle({value})"))
            .eval::<AnyUserData>()
            .unwrap_err();
        assert!(err.to_string().contains("throttle:"));
    }
}

#[test]
fn throttle_works_without_take_or_skip_and_at_exact_boundary() {
    let lua = lua();
    let mut chain = sentry(&lua, "rx:throttle(1)");
    let now = Instant::now();
    for (ms, expected) in [
        (0, true),
        (0, false),
        (999, false),
        (1000, true),
        (1500, false),
        (2000, true),
    ] {
        assert_eq!(
            chain
                .process_at(MultiValue::new(), now + Duration::from_millis(ms))
                .unwrap()
                .is_some(),
            expected
        );
    }
}

#[test]
fn throttle_is_leading_edge_and_uses_written_position() {
    let lua = lua();
    let now = Instant::now();
    let args = || 1.into_lua_multi(&lua).unwrap();
    let mut take_first = sentry(&lua, "rx:take(2):throttle(1)");
    assert!(take_first.process_at(args(), now).unwrap().is_some());
    assert!(take_first.process_at(args(), now).unwrap().is_none());
    assert!(take_first.is_exhausted());
    let mut throttle_first = sentry(&lua, "rx:throttle(1):take(2)");
    assert!(throttle_first.process_at(args(), now).unwrap().is_some());
    assert!(throttle_first.process_at(args(), now).unwrap().is_none());
    assert!(!throttle_first.is_exhausted());
    assert!(
        throttle_first
            .process_at(args(), now + Duration::from_secs(1))
            .unwrap()
            .is_some()
    );
    assert!(throttle_first.is_exhausted());

    let mut filtered_first = sentry(&lua, "rx:where(function(n) return n > 0 end):throttle(1)");
    assert!(
        filtered_first
            .process_at(0.into_lua_multi(&lua).unwrap(), now)
            .unwrap()
            .is_none()
    );
    assert!(filtered_first.process_at(args(), now).unwrap().is_some());
    let mut throttled_first = sentry(&lua, "rx:throttle(1):where(function(n) return n > 0 end)");
    assert!(
        throttled_first
            .process_at(0.into_lua_multi(&lua).unwrap(), now)
            .unwrap()
            .is_none()
    );
    assert!(throttled_first.process_at(args(), now).unwrap().is_none());
}

#[test]
fn repeated_throttles_are_independent_and_nil_removes_them_from_builder() {
    let lua = lua();
    let now = Instant::now();
    let mut chain = sentry(&lua, "rx:throttle(1):throttle(2)");
    for (ms, expected) in [(0, true), (1500, false), (2000, false), (2500, true)] {
        assert_eq!(
            chain
                .process_at(MultiValue::new(), now + Duration::from_millis(ms))
                .unwrap()
                .is_some(),
            expected
        );
    }
    let mut cleared = sentry(&lua, "rx:throttle(1):throttle(2):throttle(nil)");
    assert!(
        cleared
            .process_at(MultiValue::new(), now)
            .unwrap()
            .is_some()
    );
    assert!(
        cleared
            .process_at(MultiValue::new(), now)
            .unwrap()
            .is_some()
    );
    let mut zero = sentry(&lua, "rx:throttle(0)");
    assert!(zero.process_at(MultiValue::new(), now).unwrap().is_some());
    assert!(zero.process_at(MultiValue::new(), now).unwrap().is_some());
}

#[test]
fn event_bus_retires_listener_after_last_matching_event() {
    use crate::runtime::{
        EventBus, app_data,
        plugins::{
            OnPlugin,
            on::{
                event_descriptors::GLOBAL_EVENT_DESCRIPTORS,
                protocol::{EventScope, GameModeEvent, GameModeEventData, WorldEventData},
            },
            protocol::GameModePlugin,
        },
    };
    use std::{collections::HashMap, rc::Rc};

    let lua = lua();
    let bus = Rc::new(EventBus::new());
    lua.set_app_data(app_data::EventBus(bus.clone()));
    lua.set_app_data(app_data::EventListeners(HashMap::new()));
    lua.set_app_data(app_data::ExecutionContext::Global);
    lua.set_app_data(app_data::ActiveLayers(Vec::new()));
    let plugin = OnPlugin {
        descriptors: GLOBAL_EVENT_DESCRIPTORS,
    };
    lua.globals()
        .set("on", plugin.create_api(&lua).unwrap().unwrap())
        .unwrap();
    lua.load(
        r#"
        received = {}
        on.world.impromptu()
          :where(function(name) return name ~= "ignore" end)
          :skip(1)
          :take(2)
          :each(function(name) table.insert(received, name) end)
    "#,
    )
    .exec()
    .unwrap();
    for name in ["ignore", "skip", "first", "ignore", "second", "extra"] {
        bus.schedule_event(GameModeEvent {
            scopes: smallvec::smallvec![EventScope::Global],
            data: GameModeEventData::World(WorldEventData::Impromptu {
                name: Some(name.into()),
            }),
        });
    }
    bus.flush(&lua).unwrap();
    let received = lua
        .globals()
        .get::<mlua::Table>("received")
        .unwrap()
        .sequence_values::<String>()
        .collect::<mlua::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(received, ["first", "second"]);
    assert!(
        lua.app_data_ref::<app_data::EventListeners>()
            .unwrap()
            .0
            .values()
            .all(Vec::is_empty)
    );
}

#[test]
fn entity_query_uses_ordered_pipeline_and_fresh_state_per_execution() {
    use crate::runtime::{
        app_data,
        plugins::{
            EntityPlugin,
            entity::components::{Blueprint, Position},
            protocol::GameModePlugin,
        },
    };
    let lua = lua();
    let mut world = hecs::World::new();
    for x in 0..4 {
        world.spawn((
            Blueprint(1),
            Position(rock_wire::components::Position {
                x: x as f32,
                y: 0.0,
            }),
        ));
    }
    lua.set_app_data(app_data::World(world));
    lua.globals()
        .set("entity", EntityPlugin {}.create_api(&lua).unwrap().unwrap())
        .unwrap();
    lua.load(
        r#"
        local query = entity.query()
          :where(function(e) return e:position().x % 2 == 0 end)
          :take(2)
        assert(query:count() == 2)
        assert(query:count() == 2)
        local visited = 0
        query:each(function(e)
            assert(e:position().x % 2 == 0)
            visited = visited + 1
        end)
        assert(visited == 2)

        local calls = 0
        local count = entity.query():take(1):where(function()
            calls = calls + 1
            return false
        end):count()
        assert(count == 0 and calls == 1)
    "#,
    )
    .exec()
    .unwrap();
}
