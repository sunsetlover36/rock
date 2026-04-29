# architecture of ROCK engine

## how to use
`cargo run -- config.cfg`

## what's this?
ROCK engine is an attempt to reimagine the good old times of SA-MP servers.

except, here you use Lua to write gamemodes and the engine is platform-agnostic.
to write a SA-MP server, you've needed to use C modification called Pawn. it was quite messy and required a skill to write a clean code. with ROCK engine, i aim to ease this process so anyone can do it seamlessly (without AI help).

## how it works?
i use `mlua` to bridge Rust world and Lua world.

## why Rust and Lua?
i really enjoy writing Rust because this is a very strict language that makes me think differently about problems. that's all i have to say right now. lua is a winner when it comes to scripting tools (GMod, Roblox, etc.).

## architectural invariants
* **game loop is deterministic:** no blocking logic should be located inside the game loop. shared state/data must be presented with explicit locking mechanisms.
* **gamemode is synchronous and single-threaded:** rust and lua are connected here. `mlua` doesn't want to see `Send`.
* **sandboxing:** lua script can't crash the engine. gamemode execution may be interrupted, but the engine must live.
* **plugins:** all injected tables in Lua are plugins. no implicit or hidden injections.
* **gamemode is transport-agnostic:** gamemode should know nothing about the recipient. client -> that's all.
* **world commits are managed by the commit router:** there is no way to 'send' a message about a specific action outside of the game loop, except using commit router to emit it.
* **plugin actions should be presented as strum enums:** see `plugins/memory.rs`. it's needed to parse the type of operation that needs to be handled.

## aura invariants
* **server config must be simple to understand for any non-programming person**

## concepts
this is an ordered combination of concepts/domains that are being leveraged in the engine.

### server config
the engine starts here. this file is responsible for parsing the specified `.cfg` file from arguments.

the key feature here is the ability to easily set up all the rules. right now, it parses options like this:
```
gamemode name is Wonderful Role Play
max players is 10
```

`is` is a descriptor. that's clean.

server config is a subject to changes. `are` might be introduced. list of options might be introduced.

### actors
previously, this concept was heavily used in the engine but i got rid of it. the world logic was separated from the gamemode (there was no world tick method back then). and these were actors (running in the background as a tokio task endlessly).

actor is a block of code as a person who needs to do something in the background and needs to communicate with the outer world.
each actor implements `Actor` async trait and returns:
1. `tokio::sync::mpsc::Sender` to send messages to the actor.
2. itself -> a struct with `run` method that's being called by `ActorRuntime`.

right now, there is an only one actor -> client messenger.

#### client messenger actor
creates an `tokio::sync::mpsc` channel that accepts `ClientEnvelope<IncomingRequest>`. used to process client intents such as moving, clicking, etc.
sends all client intents to gamemode callback channel (processed by gamemode).

**transport-agnostic**. in the default implementation (pre-v1), websockets are used to send client envelopes to this channel.

### player pool
an implementation of the storage for player IDs (just like in SA-MP).
slots are stored in `Vec<Slot>`.

`Slot` struct:
```rust
pub struct Slot {
  pub generation: u32,
  pub occupied: bool,
}
```

generates a `Slot` entry for each new player that joins if no free slots are available already.
if someone logs off it's slot is being freed and pushed to reversed binary heap (`pop` operations are starting from low to high). so if there is an already generated free slot -> player gets assigned with the slot that already have been generated.

### envelope
envelope is just a real-world envelope. we have `ClientEnvelope` and `ServerEnvelope`.

`ClientEnvelope<T>` stores a payload of type `T` (generic) with the sender's player key -> we should know who've sent what.
`ServerEnvelope<T>` stores a payload of type `T` with the recipient. There are four types of recipients:
1. `All` -> all clients (broadcasting)
2. `Single(PlayerKey)` -> private message to a client
3. `List(Vec<PlayerKey>)` -> list of players
4. `Except(PlayerKey)` -> everyone, except one player

all types are declared as `EnvelopeRecipient` enum.

### session registry
session registry is a websocket session and delivery layer. it's responsibility is to provide an API to track sessions and send server messages to all listening websocket clients.
session registry creates a broadcasting channel (to broadcast messages to all clients), as well as a separate channel for each client. it also accepts `player_pool` as a param to differentiate clients safely.

also, session registry creates an `inner` state. `inner` state is used to store:
1. player pool.
2. broadcast hub -> `tokio::sync::broadcast` channel.
3. `session_channel_buffer` -> to create a new private channel for each session with a predefined buffer.
4. `sessions` -> `DashMap<PlayerKey, tokio::mpsc::sync::channel<SessionCommand>`. basically, find the private channel of a client by its key.

session registry has two methods:
1. `registrar() -> SessionRegistrar` -> a helper to handle the registration of new clients. passed as a state (`State<AppState>`) for `axum`. each WebSocket connection handler has a session registrar accessible from the `axum` state so the engine can register a new WebSocket client by calling `.register()` method that returns a new `Session` for the socket. `Session` contains all the information that socket needs to function (see below).
2. `sender() -> SessionSender` -> a helper that handles all stuff related to sending new messages to WebSocket clients.

#### about `Session`
`Session` contains:
1. a player key (`PlayerKey`) attached to it (basically, a client's address).
2. `broadcast_rx` -> what channel to listen for global broadcast messages.
3. `session_rx` -> client's private channel.
4. `registry` -> a private field that is used for `Drop` implementation to call the `unregister` method from `SessionRegistrar` helper and remove the client from the `sessions` list.

#### about `SessionRegistrar`
1. `register() -> Session` -> **locks** (`parking_lot::Mutex`) the player pool and claims a player key, creates a private channel for the session and returns a session with cloned self and `tokio::sync::broadcast::Receiver<OutgoingPacket>`.
2. `unregister(pk: &PlayerKey)` -> removes a client from `sessions` list and releases a player key from the player pool.

#### about `SessionSender`
this one is a bit more complicated than the registrar.

1. `send_ephemeral(message: ServerMessage)` -> fire-and-forget type of message to clients. errors doesn't matter. synchronous.
2. `send_reliable(message: ServerMessage) -> Result<(), SessionSendError<SessionCommand>>` -> this one is for messages that must be sent. may return an error with the initial command. asynchronous.
**caveats:** sending a reliable message to everyone (`EnvelopeRecipient::All`) or to everyone, except.. (`EnvelopeRecipient::Except`) is prohibited as of now, because there is no tryhard solution for this (at this point, this is not a problem and this isn't required in most cases)
3. `send_control(message: ControlMessage) -> Result<(), SessionSendError<SessionCommand>>` -> control panel for sockets. used by the engine to operate a socket, for example, force to close the connection. synchronous but converts to async if needed (if client's private channel is full with other messages). uses a tokio handle to spawn a tokio task in the background to await for the reliable sending. it ignores the closed channel error intentionally, because if the channel is closed then the socket is disconnected already.

### commit router
commit router's mission is to propagate what happens in the inner world to the outer world (the Internet). the beauty is that the commit router propagates world changes to sockets too (using `SessionSender` helper).

```rust
pub struct CommitRouter {
  ws_session_sender: WsCommitRouter,
}
```

`CommitRouter` just gathers all listeners to one place and calls `publish` method on each listener. it's being done manually. you need to create a listener for each entity (discord, ai bot, etc.) and write a call to this method in `emit` method of `CommitRouter`.

`emit(commit: WorldCommit)` is synchronous and used by the gamemode to publish world commits as they happen in game.

### meta db
another powerful concept in the engine. SA-MP server devs were free to pick their favorite database. not that like i'm trying to create a boundary for devs, but rather i want to give a useful and optimized tool to store the in-game data with no pain.

meta db acts as a caching mechanism for hot data and a storage for cold data (long-term data). cache using `DashMap<String, MetaEntry>` to store recently pulled data. `DashMap` handles locks/unlocks under the hood, and doing its task really good, especially with shards of data.

as a persistent database, i decided to go with SQLite. nosql databases are not really suited for games imo, things can get messy really fast. from the list of sql databases, i excluded postgres due to its complexity and unneeded overhead. sqlite is quite simple and suits the engine best (as long as nothing breaks). i use a few pragmas such as `synchronous = NORMAL` and WAL to be able to restore the database from the log in case something breaks in the middle of nothing. `synchronous = NORMAL` reduces the number of synchronizations and WAL mode is safe from accidental corruption with this `PRAGMA`.

#### meta db config
meta db config is a combination of fields:
1. `mode_id` -> gamemode name (parsed from the server config)
2. `default_ttl` -> default TTL (`Duration`) for fields in the cache

#### why meta db requires a gamemode name
because there can be multiple gamemodes present for the same engine. meta db needs to differentiate data for different gamemodes.

#### what is `MetaValue` and `MetaEntry`
this stuff is used by the cache. what about sqlite? the engine stores jsonbs in it. that's all.

1. `MetaValue` -> enum to indicate if the value is missing, fresh or stale. simple.
2. `MetaEntry` -> struct with the metadata about `MetaValue`. stores `ttl: Duration` and `updated_at: SystemTime` to know when this value expires and when it was last updated.

#### methods
1. `get(key: &str) -> MetaValue` -> cache interaction. give key then get value. synchronous.
2. `update_entry(key: &str, value: Option<JsonValue>)` -> cache interaction. private `set` method. synchronous.
3. `ensure_key(key: &str) -> Result<Option<JsonValue>, MetaEnsureError>` -> get a fresh row from sqlite by the key. updates cache with the new value automatically. asynchronous.

example: `ensure_key("player/42/weapons_list")` -> gets you a list of weapons (jsonb)
4. `ensure_prefix(prefix: &str) -> Result<Option<JsonValue>, MetaEnsureError>` -> get a map (`serde_json::Map`) of rows by the prefix. asynchronous.

example: `ensure_prefix("player/42")` -> returns everything under that prefix in sqlite for a specific gamemode. stripes a prefix by default, so you get a map like this:
```json
{
  "hp": 100,
  "weapons": ["knife"]
}
```

instead of,
```json
{
  "player/42/hp": 100,
  "player/42/weapons": ["knife"]
}
```
5. `get_or_ensure_key(key: &str)` and `get_or_ensure_prefix(prefix: &str)` -> lazy `ensure` methods. if the value is fresh -> return it from the cache straight away, otherwise, pull it from sqlite and update the cache. asynchronous.

### runtime
runtime is a heart of the engine. it gives a birth to gamemodes.

runtime initialization params:
* runtime gets initialized with the active gamemode name from the server config.
* it also requires `client_api` -> runtime uses client api to give gamemodes an opportunity to communicate with clients. runtime is **transport-agnostic**. it means that we can attach any client api to it. a client api decides what to do with the message from the world. as a default option, i implemented a default client api that converts `GameModeClientCommand` to `ServerMessage` messages, essentially communicating with the session registry.
* `callback_rx` -> a channel for gamemode callbacks. this is the way to befriend gamemode with other entities and domains. for example, client messenger actor sends client intents to this channel (`GameModeClientCommand::Client(...)`).
* `commit_router` -> handed to the runtime from the outside because commit router should be initialized in the main file because it gathers all types of listeners located throughout the engine.
* `meta_db` -> runtime uses meta db to inject a `memory` plugin (see below)
* `tokio_handle` -> scheduler is using it to spawn async background tasks (see below)

gamemode runtime operates on a different synchronous thread (apart of tokio runtime), so the tokio runtime is not being blocked by gamemode calculations and ticks.

the great three pillars of `runtime.rs`:
1. world state and world natives
2. gamemode callbacks
3. lua, plugins and scheduler

#### world state and world natives
SA-MP had its natives (`GetVehiclePos`, `SetPlayerPos`, etc.). natives is a convenient way to modify the world state. that's all.

world state is an in-game state. this is the place where the difference between meta db and world state can be easily spotted. meta db is used for custom scripter's data (anything). world state is the state of the world. world has its own laws and deterministic behavior and data model, for example, entity positions, cars if present etc. state that is forged to the world (just like San Andreas in GTA SA with its cars and skins and physics laws).

#### gamemode callbacks
SA-MP had its callbacks (`OnGameModeInit`, `OnPlayerConnect`, etc.). callbacks here is a more convenient way (compared to SA-MP) to handle different events. the beauty and obvious strength of callbacks in ROCK engine is that:
1. you can define and code your own callbacks (for example: redis indexer events, blockchain events)
2. engine callbacks (lifecycle stages, like `OnGameModeInit`) are predefined and already separated from client callbacks
3. callbacks DX is simply better:
```lua
-- init.lua
when.player.connects(function(p)
  p:send_message("Welcome to the Rock City!")
end)

-- auth.lua
when.player.connects(function(p)
  if p:is_banned() then
    p:kick("Banned")
  end
)
```
vs.
```c
// gamemode.pwn
public OnPlayerConnect(playerid) {
  Auth_OnConnect(playerid);
  Welcome_Message(playerid);
  return 1;
}
```

and

```lua
when.player.connects.pipe()
  :filter(function(p) return p:is_vip() end)
  :subscribe(function(p) p:give_item("gold_ak47") end)

when.player.connects.pipe()
  :timestamp()
  :buffer(10)
  :subscribe(function(batch)
    BroadcastMessage("10 players connected in", batch:timespan())
  end)
```
vs.
```c
// ???
```

currently, there are two active types of callback: `RuntimeCallback::System` for system callbacks (called by the engine) and `RuntimeCallback::Client` for client messages.

#### lua, plugins and scheduler
the engine uses `mlua` to make Rust and Lua friends. one of the most powerful concepts of the engine shines here: scenes and plugins.

let me show you a simple example:
```lua
scene.create{
  name = "strange_pickup",
  action = function(p)
    local has_vip_nft = memory.fetch(string.format("player/%d/has_vip_nft", p.id));
    if has_vip_nft then
      p:give_item("gold_ak47")
    end
  end
}

when.player.enters_zone("my_pickup_37", function(p)
  scene.play{ name = "strange_pickup", details = {p} }
end)
```

implicit async with entity-scoped execution. the scene will be interrupted if player disconnects.

`memory` is a plugin. every injected namespace/table is a plugin that defines two methods:
1. `create_global_api` -> synchronous API that is accessible outside of scenes (async code).
2. `create_scene_api` -> sync + async API that is accessible inside the scenes only.

scripter can use global API everywhere. it's behavior is deterministic, non-blocking and expected.
scene API is where the things get beautiful. you can perform async tasks under the hood but on the surface you just write a simple declarative code.

global API injects tables with synchronous-only methods and scene API uses a global API table to extend it with new sync or async methods inside the scene coroutine scope.
the beauty is that everything is synchronous at the surface. async methods return opcodes for async operations to be performed and handled by scheduler.

async methods in the scene API don't perform any kind of asynchronous code nor the engine uses async directly to handle the request.

once the async method is called inside the coroutine -> it's result is being yielded as `coroutine.yield(OPCODE)`. `scene.play` and `scene.run` methods create a coroutine from the closure. after that, it sends a channel message to the scheduler (`SchedulerMessage::AddTask`): add task (new coroutine arrived) to a queue. the thing is that i'm not passing an actual coroutine or something. i'm passing its registry key so the scheduler can get this coroutine when it needs, because scheduler also has `&lua` borrowed reference.
scheduler's task is to keep track of incoming coroutines, advance and wake up them when needed (keep yielding until an actual return) and then finish its execution.

it has a `tick` method that pings a channel for new messages (add task, cancel task, etc.). `scheduler.tick` method is being called in the main game loop (each 16ms as of now). so it keeps running along with the game loop.
scheduler parses opcode and gets two elements: prefix and suffix.

`prefix` is a plugin's name, `suffix` is an action related to this plugin. to keep things clean, each plugin has a `name()` method that returns a constant `&str`, so opcodes are being constructed safely. i also use `strum` to parse operations from enums (for example, `MemoryOp`). opcodes are written in `SCREAMING_SNAKE_CASE`.
each plugin has a `handle_op` method that returns `Result<Option<AsyncTask>>` where `AsyncTask` is a `BoxFuture`. basically, a work that needs to be done. so scheduler synchronously calls `handle_op` method of the parsed plugin (scheduler stores plugins as a `HashMap<String, Box<dyn GameModePlugin>>`). it returns a `BoxFuture` and scheduler spawns a tokio task using a handle where it executes and awaits for the task to complete. after that, it uses its own channel to wake up a task or alert about an error.

scheduler and plugins are initialized in `api::register` module.

### axum
web app framework. public API: HTTP and WebSockets. not much to describe.

