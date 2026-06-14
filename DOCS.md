# ROCK Server Runtime → Scripting Guide

**ROCK is a server runtime for Lua-scripted multiplayer worlds.**

Documentation for world scripters. Everything you need to build multiplayer worlds with ROCK.

## Table of Contents

- [Getting Started](#getting-started)
- [Core Concepts](#core-concepts)
- [Tutorial: Build Your First Gamemode](#tutorial-build-your-first-gamemode)
- [API Reference](#api-reference)
  - [on → Events](#on)
  - [entity → Entities and Blueprints](#entity)
  - [player → Players](#player)
  - [input → Input Bindings](#input)
  - [memory → Persistent Storage](#memory)
  - [scene → Async Scenes](#scene)
  - [timer → Timers](#timer)
  - [layer → Layers](#layer)
  - [room → Rooms](#room)
  - [fc → Farcaster](#fc)
  - [Const -> Constants](#const)
- [Reactive Operators](#reactive-operators)
- [Network Replication](#network-replication)
- [Components Reference](#components-reference)
- [WebSocket Protocol](#websocket-protocol)
- [Geodes](#geodes)
- [Impromptu (Live Coding)](#impromptu-live-coding)
- [Static Assets](#static-assets)

---

## Getting Started

### Project Structure

A ROCK server consists of:

```
config.toml         -- server configuration
gamemodes/
  my_gamemode.lua   -- your gamemode script
geodes/             -- plugin packages (optional)
assets/             -- static files served at /assets/* (optional)
db/
  db.sqlite         -- persistent storage (auto-created)
```

### Configuration

#### `config.toml`

```toml
[gamemode]
# Gamemode file without .lua extension.
name = "farcaster"

[auth]
# Optional. Enables auth providers and requires authenticated sessions.
providers = ["ticket", "farcaster"]
# Optional. Allows clients without a token to connect as anonymous sessions.
allow_anonymous = false

[auth.ticket]
# Env var that stores the HS256 secret.
secret_env = "TICKET_SECRET"
audience = "rock"

[auth.farcaster]
issuer = "https://auth.farcaster.xyz"
# Your miniapp / site domain.
audience = "YOUR_DOMAIN"
jwks_url = "https://auth.farcaster.xyz/.well-known/jwks.json"

[farcaster]
# Env var that stores the Neynar webhook secret.
webhook_env = "NEYNAR_WEBHOOK_SECRET"
# Neynar API key.
api_key = "YOUR_NEYNAR_API_KEY"
```

#### Environment variables
```env
HOST=127.0.0.1
PORT=3000
ROCK_SESSION_COOKIE=rock_session
ROCK_ALLOWED_ORIGINS=http://127.0.0.1:3000,http://localhost:5173
ROCK_IMPROMPTU_TOKEN=change-me
```

* `HOST`: address to bind the server (default: `127.0.0.1`)
* `PORT`: port to listen on (default: `3000`)
* `ROCK_SESSION_COOKIE`: cookie name used for authenticated WebSocket sessions (default: `rock_session`)
* `ROCK_ALLOWED_ORIGINS`: comma-separated WebSocket `Origin` allowlist for protected cookie sessions. Required when auth providers are enabled and clients connect with the session cookie.
* `ROCK_IMPROMPTU_TOKEN`: token required by the live-code `/impromptu` endpoint. If unset or empty, `/impromptu` rejects every request.

The engine loads environment variables from a local `.env` file on startup, then falls back to the process environment.

### CLI

```bash
# Start the engine
rock ignite

# Scaffold a new gamemode
rock genesis my_gamemode
```

`rock genesis` creates the gamemode Lua file in `gamemodes/` and, if `config.toml` doesn't exist yet, creates it with the gamemode name already set.

`rock ignite` starts the server on `127.0.0.1:3000`. Clients connect via WebSocket at `ws://127.0.0.1:3000`.

### Hot Reload

The engine watches an active gamemode file in `gamemodes/`. Save a file and the runtime reloads automatically → no restart needed.

---

## Core Concepts

### Game Loop

The engine runs a deterministic game loop at ~60 ticks per second. Each tick:

1. Process incoming events (player connect/disconnect, client input)
2. Advance scenes (coroutines)
3. Fire timers
4. Dispatch all queued events to your Lua handlers
5. Replicate world state to connected clients

Your Lua code is **synchronous and single-threaded**. You never deal with threads or async directly.

### Plugins

Every global table in your Lua environment is a **plugin**. There are 11 plugins:

| Plugin | Purpose |
|--------|---------|
| `on` | Listen to events |
| `entity` | Create and manage game entities |
| `player` | Interact with connected players |
| `input` | Register input bindings |
| `memory` | Persistent key-value storage |
| `scene` | Run async code (database calls, etc.) |
| `timer` | Schedule timed events |
| `layer` | Group resources for bulk cleanup |
| `room` | Generate room IDs |
| `fc` | Farcaster (users, casts, webhooks) |
| `Const` | Engine constants |

### Entities

Entities are the building blocks of your world. Each entity is a bag of **components** (position, rotation, name, etc.) plus **custom data** (any Lua table).

You define **blueprints** (templates), then **spawn** entities from them:

```lua
local zombie = entity.blueprint()
  :position({ x = 0, y = 0 })
  :custom({ health = 50, alive = true })
  :name("zombie")

-- spawn one
local z = zombie:spawn():position({ x = 10, y = 5 }):room("world")
```

### Events

Events are how you react to things happening in the world. Use `on.*` to register handlers:

```lua
on.player.online():each(function(p)
  print("Player connected:", p:id())
end)
```

Handlers support **reactive operators** → you can filter, transform, throttle, and limit events with a chainable API.

### Scenes

Scenes let you write async code that looks synchronous. Inside a scene, you can call async functions like `memory.fetch()` that would normally block:

```lua
scene.run(function()
  local data = memory.fetch("player/42/")
  print(data.health)
end)
```

Under the hood, scenes are Lua coroutines. The engine handles yielding and resuming transparently.

### Layers

Layers group resources (entities, event listeners) so you can clean them all up at once:

```lua
local world = layer.create():with(function()
  -- everything created here belongs to this layer
  on.world.awake():each(function() print("world ready") end)
  entity.blueprint():name("tree"):spawn()

  -- return a cleanup function (optional)
  return function()
    print("world layer cleared")
  end
end):commit()

-- later: destroy everything in the layer
world:clear()
```

### Rooms

Rooms are spatial replication spaces. Entities exist in rooms, and players receive updates through vision anchors attached to entities inside those rooms:

```lua
player:vision():attach(camera)
camera:room("lobby")
```

### Presence
Presence is a lightweight membership system for lobbies, chat channels, matchmaking, or social grouping. It does not affect spatial replication.

```lua
player:presence():enter("lobby")
player:presence():exit("lobby")
```

Presence is a pretty small module for now. It tracks membership and emits enter/exit events. Most gameplay replication should be modeled through entity rooms and vision anchors.

---

## Tutorial: Build Your First Gamemode

Let's build a simple multiplayer world where players move on a grid.

### Step 1: Create the gamemode

```bash
rock genesis my_world
```

This creates `gamemodes/my_world.lua` and sets up `config.toml` pointing to it (if it doesn't already exist).

### Step 2: Register input

```lua
local Keyboard = Const.Input.Keyboard
local Controller = Const.Input.Controller
local Stick = Const.Input.Stick

input.vector()
  :defaults({
    keyboard = {
      up = { Keyboard.KeyW, Keyboard.ArrowUp },
      down = { Keyboard.KeyS, Keyboard.ArrowDown },
      left = { Keyboard.KeyA, Keyboard.ArrowLeft },
      right = { Keyboard.KeyD, Keyboard.ArrowRight },
    },
    controller = {
      up = { Controller.DPadUp },
      down = { Controller.DPadDown },
      left = { Controller.DPadLeft },
      right = { Controller.DPadRight },
    },
    stick = Stick.LeftStick,
  })
  :register("Move")
```

### Step 3: Define a player blueprint

```lua
local player_bp = entity.blueprint()
  :position({ x = 5, y = 5 })
  :custom({ health = 100 })
  :name("player")

-- replicate all fields to clients
player_bp:sync():commit()
```

### Step 4: Handle player connections

```lua
on.player.online():each(function(p, params)
  local pid = p:id()

  -- spawn a player entity
  local ent = player_bp:spawn()
    :owned_by(pid)
    :position({ x = 5, y = 5 })
    :room("world")

  -- attach player's vision to their entity
  p:vision():attach(ent)

  -- send the player their identity
  p:signal("Identity"):data({ pid = pid, room = params.room }):send()

  print(string.format("Player %d joined", pid))
end)

on.player.offline():each(function(p)
  -- offline receives a snapshot, not a live player handle.
  print("Player left:", p:id(), p:who() or "anonymous")
end)
```

### Step 5: Handle movement

```lua
on.player.input()
  :bind_action("Move")
  :each(function(p, data)
    entity.query()
      :owned_by(p:id())
      :blueprint(player_bp)
      :each(function(ent)
        local pos = ent:position()
        ent:position({
          x = pos.x + (data.x or 0),
          y = pos.y + (data.y or 0),
        })
      end)
  end)
```

### Step 6: Run it

```bash
rock ignite
```

Connect a client to `ws://127.0.0.1:3000`. You now have a multiplayer world with player movement and automatic state replication.

---

## API Reference

### `on`

The event system. Every event in ROCK is listened to through `on`.

#### Available Events

| Event | Handler Args | Description |
|-------|-------------|-------------|
| `on.world.awake()` | *(none)* | Fires once when the gamemode starts |
| `on.world.impromptu()` | *(none)* | Fires when code is injected via `/impromptu` endpoint |
| `on.player.online()` | `PlayerHandle, connection_params` | Player connected |
| `on.player.offline()` | `PlayerSnapshot` | Player disconnected |
| `on.player.input()` | `PlayerHandle, InputAction` | Player sent input (see `:bind_action` below) |
| `on.player.enter()` | `PlayerHandle, room_name` | Player entered a room |
| `on.player.exit()` | `PlayerHandle, room_name` | Player exited a room |
| `on.player.signal()` | `PlayerHandle, PlayerSignal` | Player sent a custom signal |
| `on.timer.fire()` | `timer_id, data` | A timer fired (see `:named` below) |
| `on.fc.webhook()` | `WebhookEvent` | Farcaster webhook received (see [fc](#fc)) |

#### PlayerSignal

`on.player.signal()` receives custom client signals.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Signal name |
| `data` | `any` | JSON-compatible payload sent by the client |

Example:

```lua
on.player.signal()
  :where(function(_, s) return s.name == "Attack" end)
  :each(function(p, s)
    print("use spell", s.data.use_spell)
  end)
```

#### Event Builder (OnRx)

Every `on.*` call returns a builder. Chain methods, then finish with `:each(handler)`:

```lua
on.player.online()
  :take(5)                    -- only handle first 5 connections
  :where(function(p)          -- filter
    return p:id() < 100
  end)
  :each(function(p)           -- handler
    print("early player:", p:id())
  end)
```

| Method | Args | Description |
|--------|------|-------------|
| `:take(n)` | `number` | Only fire for the first N events, then stop |
| `:skip(n)` | `number` | Skip the first N events before firing |
| `:throttle(secs)` | `number` | Minimum seconds between fires |
| `:where(fn)` | `function(...) -> bool` | Only fire if predicate returns true |
| `:select(fn)` | `function(...) -> ...` | Transform the handler arguments |
| `:changed()` | -- | Only fire when the value differs from last time |
| `:name(s)` | `string` | Name this listener (prevents duplicates with same name) |
| `:priority(n)` | `number` | Higher priority fires first (default 0) |
| `:bind_action(name)` | `string` | **Input only.** Filter to a specific input action and remap args to `(PlayerHandle, data)` |
| `:named(name)` | `string` | **Timer only.** Filter to a specific timer ID and remap args to just `(data)` |
| `:each(fn)` | `function(...)` | Subscribe the handler. Returns a `ListenerHandle` |

#### ListenerHandle

Returned by `:each()`. Use it to unsubscribe:

```lua
local handle = on.player.online():each(function(p) end)
handle:off()  -- removes the listener
```

| Method/Field | Description |
|-------------|-------------|
| `:off()` | Removes this listener |
| `.name` | The listener's name (or nil) |

#### Event Propagation

Return `true` from a handler to stop propagation to lower-priority handlers:

```lua
on.player.input()
  :priority(1)
  :bind_action("Move")
  :each(function(p, data)
    -- handle movement
    return true  -- stops other input handlers from seeing this event
  end)
```

#### Entity-Scoped Events

Blueprints and entity handles have an `.on` field for entity-specific events:

```lua
-- listen to position changes on all entities from this blueprint
zombie_bp.on.move():each(function(entity_id, position)
  print("moved to", position.x, position.y)
end)

-- listen to custom data changes on a specific entity
my_entity.on.custom():each(function(entity_id, data)
  if data.health <= 0 then
    print("entity died")
  end
end)
```

| Event | Handler Args | Description |
|-------|-------------|-------------|
| `.on.move()` | `entity_id, position` | Entity position changed |
| `.on.custom()` | `entity_id, data` | Entity custom data changed |

---

### `entity`

Create, manage, and query game entities.

#### `entity.blueprint()`

Creates a new blueprint (template). Returns an `EntityBlueprint`.

```lua
local bp = entity.blueprint()
  :position({ x = 0, y = 0 })
  :rotation(0)
  :control({ speed = 5 })
  :sprite({ texture = "zombie.png", scale = { x = 1, y = 1 }, layer = 0, visible = true })
  :char({ char = "Z", color = "#FF0000", bg_color = nil, visible = true })
  :custom({ health = 100, alive = true })
  :name("zombie")
```

**Blueprint Methods:**

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:position(t)` | `{ x, y }` | self | Set default position |
| `:rotation(r)` | `number (u8)` | self | Set default rotation |
| `:control(t)` | `{ speed }` | self | Set control component |
| `:sprite(t)` | `{ texture, scale, layer, visible }` | self | Set 2D sprite |
| `:char(t)` | `{ char, color, bg_color, visible }` | self | Set character sprite |
| `:owned_by(pid)` | `PlayerId` | self | Set owner |
| `:name(s)` | `string` | self | Set display name |
| `:room(s)` | `string` | self | Set room |
| `:custom(t)` | `table` | self | Set custom data |
| `:from(name)` | `string` | self | Inherit from a registered blueprint |
| `:register(name)` | `string` | -- | Register this blueprint by name (for `:from()`) |
| `:spawn()` | -- | `EntityHandle` | Spawn an entity from this blueprint |
| `:sync()` | -- | `SyncRx` | Create a replication policy builder (see [Network Replication](#network-replication)) |

#### `EntityHandle`

A live entity in the world. Returned by `:spawn()` or from queries.

**Getter/Setter methods** → call with no args to get, call with args to set (returns self for chaining):

```lua
local ent = bp:spawn():position({ x = 10, y = 5 }):name("bob")

-- get
local pos = ent:position()   -- { x = 10, y = 5 }
local name = ent:name()      -- "bob"

-- set (returns self)
ent:position({ x = 20, y = 10 }):rotation(90)
```

| Method | Get | Set |
|--------|-----|-----|
| `:position([t])` | `{ x, y }` or nil | Sets position |
| `:rotation([r])` | `number` or nil | Sets rotation |
| `:control([t])` | `{ speed }` or nil | Sets control |
| `:sprite([t])` | sprite table or nil | Sets sprite |
| `:char([t])` | char table or nil | Sets char sprite |
| `:owned_by([pid])` | `PlayerId` or nil | Sets owner |
| `:name([s])` | `string` or nil | Sets name |

**Other methods:**

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:room([name])` | `string?` | room_id (get) or self (set) | Get/set room |
| `:custom([value])` | table, function, or nil | table (get) or self (set) | Get/set/update custom data |
| `:despawn()` | -- | -- | Remove entity from the world |
| `:exists()` | -- | `boolean` | Check if entity still exists |
| `:sync()` | -- | `SyncRx` | Create replication policy for this entity |

**Custom data** has three modes:

```lua
-- get
local data = ent:custom()

-- set (replace entirely)
ent:custom({ health = 50, alive = true })

-- update (merge via function)
ent:custom(function(c)
  c.health = c.health - 10
  return c
end)
```

#### `entity.query()`

Query entities with filters. Returns a `QueryRx`.

```lua
-- count entities near a point
local n = entity.query()
  :at({ position = { x = 0, y = 0 }, radius = 50, shape = Const.AreaShape.Circle })
  :count()

-- iterate over a player's entities
entity.query()
  :owned_by(pid)
  :blueprint(player_bp)
  :each(function(ent)
    print(ent:position().x)
  end)
```

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:owned_by(pid)` | `PlayerId` | self | Filter by owner |
| `:named(s)` | `string` | self | Filter by name |
| `:in_room(s)` | `string` | self | Filter by room name |
| `:at(area)` | `{ position, radius, shape }` | self | Filter by spatial area |
| `:blueprint(bp)` | `EntityBlueprint` | self | Filter by blueprint |
| `:count()` | -- | `number` | Count matching entities |
| `:first()` | -- | `EntityHandle` or `nil` | Return the first matching entity, or `nil` if none |
| `:each(fn)` | `function(EntityHandle)` | -- | Iterate over matches |

**`:first()` vs `:each()`.** Use `:first()` when you expect at most one result and want to skip the closure:

```lua
-- instead of this
entity.query():owned_by(pid):blueprint(player_bp):each(function(ent)
  ent:position({ x = 0, y = 0 })
end)

-- do this
local ent = entity.query():owned_by(pid):blueprint(player_bp):first()
if ent then
  ent:position({ x = 0, y = 0 })
end
```

---

### `player`

Interact with connected players.

#### `player.get(pid)`

Look up a player by ID. Returns `PlayerHandle` or `nil`.

```lua
local p = player.get(42)
if p then
  p:signal("Hello"):data({ text = "hi" }):send()
end
```

#### `player.list()`

Returns a table of all connected players as `PlayerHandle`s.

#### `player.broadcast()`

Create a broadcast signal (to all or a subset of players).

```lua
-- signal to everyone
player.broadcast():signal("Message"):data({ text = "Server restart in 5 min" }):send()

-- signal to players in an area
player.broadcast()
  :signal("Explosion")
  :data({ radius = 10, damage = 50 })
  :area({ position = { x = 100, y = 50 }, radius = 10 })
  :send()
```

#### PlayerHandle

| Method | Returns | Description |
|--------|---------|-------------|
| `:id()` | `number` | The player's ID (slot index) |
| `:kick()` | -- | Disconnect the player |
| `:signal([name])` | `SignalRx` | Create a signal targeted to this player |
| `:presence()` | `PlayerPresence` | Access presence management |
| `:vision()` | `PlayerVision` | Access vision/anchor management |
| `:who()` | `string?` | Auth identity, e.g. `fc:423406`, or `nil` for anon sessions |
| `:fid()` | `number?` | Farcaster ID parsed from `:who()`, or `nil` for anon/non-Farcaster sessions |

**Connection params.** When a client connects via `ws://host:port/?room=0xabc&name=Bob`, the query string is captured at handshake time and passed as the second `on.player.online()` argument:

```lua
on.player.online():each(function(p, params)
  local room_hash = params.room     -- "0xabc"
  local display_name = params.name  -- "Bob"

  if not room_hash then
    p:kick()
    return
  end
end)
```

All values are strings (or `nil` if the param is absent). Use `tonumber()` to coerce numeric params yourself.

#### PlayerSnapshot

`on.player.offline()` receives a `PlayerSnapshot` instead of a live `PlayerHandle`. By the time the offline event fires, the socket has gone away, so you can read identity data but cannot send signals, kick, edit presence, or attach vision.

| Method | Returns | Description |
|--------|---------|-------------|
| `:who()` | `string?` | Auth identity, e.g. `fc:423406`, or `nil` for anon sessions |
| `:fid()` | `number?` | Farcaster ID parsed from `:who()`, or `nil` for anon/non-Farcaster sessions |

#### PlayerPresence

| Method | Args | Description |
|--------|------|-------------|
| `:enter(name)` | `string` | Enter a presence group |
| `:exit([name])` | `string?` | Exit a presence group, or all groups if no name is provided |

#### PlayerVision

Vision determines what a player can "see" for network replication. You attach the player's vision to entities → the player will receive updates for entities near their anchors, in the same rooms.

| Method | Args | Description |
|--------|------|-------------|
| `:attach(ent)` | `EntityHandle` | Attach vision to an entity |
| `:detach([ent])` | `EntityHandle?` | Detach from an entity (or all if no arg) |

#### SignalRx

Signals are custom packets you send to clients. The client receives them as `{ t: "signal", d: { name, data } }`.

```lua
p:signal("Identity"):data({ pid = p:id() }):send()
```

| Method | Args | Description |
|--------|------|-------------|
| `:data(t)` | `table` | Set the signal payload |
| `:area(a)` | `{ position, radius }` | Limit recipients to an area (broadcast only) |
| `:room(s)` | `string` | Limit recipients to a room (broadcast only) |
| `:send()` | -- | Send the signal |

#### BroadcastRx

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:signal([name])` | `string?` | `SignalRx` | Create a signal to all players |

---

### `input`

Register input actions that clients can send. Inputs have a **kind**: `vector`, `axis`, `button`.

Use:

- `input.vector()`
- `input.axis()`
- `input.button()`

Each returns an `InputRx` builder. Key, mouse, controller and stick constants live in the [`Const`](#const) plugin.

---

#### Vector input
```lua
local Keyboard = Const.Input.Keyboard
local Controller = Const.Input.Controller
local Stick = Const.Input.Stick

input.vector()
  :defaults({
    keyboard = {
      up = { Keyboard.KeyW, Keyboard.ArrowUp },
      down = { Keyboard.KeyS, Keyboard.ArrowDown },
      left = { Keyboard.KeyA, Keyboard.ArrowLeft },
      right = { Keyboard.KeyD, Keyboard.ArrowRight },
    },
    controller = {
      up = { Controller.DPadUp },
      down = { Controller.DPadDown },
      left = { Controller.DPadLeft },
      right = { Controller.DPadRight },
    },
    stick = Stick.LeftStick,
  })
  :register("Move")
```

#### Button input
```lua
local Keyboard = Const.Input.Keyboard
local Controller = Const.Input.Controller

input.button()
  :defaults({
    keyboard = { Keyboard.KeyE },
    controller = { Controller.ButtonA },
  })
  :register("Use")
```

#### Axis input
```lua
local Keyboard = Const.Input.Keyboard
local Stick = Const.Input.Stick

input.axis()
  :defaults({
    keyboard = { negative = { Keyboard.KeyA }, positive = { Keyboard.KeyD } },
    stick = Stick.LeftStick,
  })
  :register("Strafe")
```

#### InputRx

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:defaults(t)` | `table` | self | Set default key bindings. Shape depends on input kind |
| `:register(name)` | `string` | -- | Register the input action by name |

#### Reading input
When a player sends input for a registered action, it fires `on.player.input()`.

Use `:bind_action("Move")` to filter by action:

```lua
on.player.input()
  :bind_action("Move")
  :each(function(p, data)
    -- data = { x = 1, y = 0 } for vector
    -- data = true/false for button
    -- data = 0.5 for axis
  end)
```

---

### `memory`

Persistent key-value storage backed by SQLite with an in-memory cache.

Keys are hierarchical paths like `"player/42/health"`. Keys ending with `/` are **prefixes** that return a map of all nested values.

#### API

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `memory.peek(key)` | `string` | value or nil | Read from cache only (fast, synchronous, may be stale or missing) |
| `memory.node(key)` | `string` | `SyncRx` | Create a replication policy for a memory node (see [Network Replication](#network-replication)) |
| `memory.recall(key)` | `string` | value | Scene-only. Read from cache; if missing, fetch from DB first |
| `memory.fetch(key)` | `string` | value | Scene-only. Always fetch the latest value from DB |
| `memory.store(key, value)` | `string`, any | -- | Scene-only. Write a value to DB. Prefix keys store a map of values |
| `memory.delete(key)` | `string` | -- | Scene-only. Delete a key or prefix |

```lua
-- synchronous cache read (global)
local cached = memory.peek("player/42/health")

-- async operations (scene only)
scene.run(function()
  -- fetch from DB
  local health = memory.fetch("player/42/health")

  -- fetch all player data as a map
  local player = memory.fetch("player/42/")
  -- player = { health = 100, money = 2000, ... }

  -- store a value
  memory.store("player/42/health", 100)

  -- store multiple values under a prefix
  memory.store("player/42/", {
    health = 100,
    money = 2000,
  })

  -- delete a key or everything under a prefix
  memory.delete("player/42/temporary_boost")
end)
```

---

### `scene`

Scenes let you run async code (database calls, network requests) inside what looks like synchronous Lua. Under the hood they use coroutines → the engine yields when an async operation starts and resumes when it completes.

#### `scene.run(fn)`

Immediately run a function as a scene:

```lua
scene.run(function()
  local data = memory.fetch("player/42/")
  memory.store("player/42/health", data.health + 10)
  print("healed!")
end)
```

#### `scene.create()`

Create a reusable scene builder.

```lua
-- create and register a named scene
scene.create():script(function()
  local data = memory.fetch("player/1/")
  print("loaded:", data)
end):register("load_data")

-- play it later
scene.play("load_data")
```

#### `scene.play(name)`

Play a previously registered scene by name.

#### SceneRx

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:script(fn)` | `function` | self | Add a script function to this scene |
| `:play()` | -- | -- | Immediately run the scene |
| `:register(name)` | `string` | -- | Register by name for later playback |

---

### `timer`

Schedule one-shot, repeating, or cron-based timers. Timers fire as events through `on.timer.fire()`.

#### `timer.create()`

Create a new `TimerRx` builder.

```lua
-- one-shot (3 seconds)
timer.create():timeout(3):register("my_timeout")

-- repeating (every 30 seconds)
timer.create():interval(30):register("heartbeat")

-- cron (every 10 seconds)
timer.create():cron("*/10 * * * * *"):register("cron_job")

-- with data
timer.create()
  :timeout(5)
  :data({ x = 10, y = 20, kind = "health" })
  :register("respawn_pickup")
```

#### `timer.cancel(id)`

Cancel a timer by its ID.

#### TimerRx

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:timeout(secs)` | `number` | self | Fire once after N seconds |
| `:interval(secs)` | `number` | self | Fire every N seconds |
| `:cron(expr)` | `string` | self | Fire on a cron schedule |
| `:data(value)` | any | self | Attach data payload (passed to the handler) |
| `:register(id)` | `string` | `TimerHandle` | Start the timer with this ID |

#### TimerHandle

| Method | Description |
|--------|-------------|
| `:cancel()` | Cancel this timer |

#### Listening to Timers

```lua
-- listen to a specific timer
on.timer.fire()
  :named("heartbeat")
  :each(function(data)
    print("heartbeat!", data)
  end)

-- listen to all timers
on.timer.fire():each(function(id, data)
  print("timer fired:", id)
end)

-- pattern match timer IDs
on.timer.fire()
  :where(function(id) return string.find(id, "^respawn_") end)
  :each(function(id, data)
    print("respawning at", data.x, data.y)
  end)
```

---

### `layer`

Layers group resources (entities, event listeners, timers) so you can clean them all up at once. Useful for organizing game logic into composable blocks.

#### `layer.create()`

Create a new layer builder. Returns `LayerRx`.

```lua
local world = layer.create():with(function()
  -- everything created here is tracked by this layer

  on.world.awake():each(function()
    print("world ready")
  end)

  local tree = entity.blueprint():name("tree"):spawn():room("forest")

  -- return an optional cleanup function
  return function()
    print("world layer cleared")
  end
end):commit()

-- later: destroy everything
world:clear()
```

#### `layer.clear(name)`

Clear a named layer:

```lua
local l = layer.create():name("combat"):with(function()
  -- ...
end):commit()

-- clear by name
layer.clear("combat")
```

#### LayerRx

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:with(fn)` | `function() -> function?` | self | Add a callback; optionally return a cleanup function |
| `:name(s)` | `string` | self | Name this layer (for `layer.clear(name)`) |
| `:commit()` | -- | `LayerHandle` | Activate the layer, run all callbacks |

#### LayerHandle

| Method | Description |
|--------|-------------|
| `:clear()` | Destroy this layer and run all cleanup functions |

---

### `room`

Utility for working with rooms.

#### `room.generate_id()`

Returns a random unique room ID. Useful for temporary spaces:

```lua
local room_id = room.generate_id()
entity:room(room_id)
```

---

### `fc`

Farcaster integration. Look up users, post casts, and react to incoming webhook events from the Farcaster network (via Neynar).

> **Requires configuration.** The `fc` plugin is only available when a Neynar API key is set in `config.toml`:
>
> ```toml
> [farcaster]
> api_key = "YOUR_NEYNAR_API_KEY"
> ```
>
> Only Neynar API keys are supported. If `api_key` is not configured, the `fc` plugin will not be registered.
>
> **Webhook support.** `on.fc.*` events are powered by webhooks, so you need to provide the name of the env var containing a webhook secret in `config.toml`:
>
> ```toml
> [farcaster]
> webhook_env = "FARCASTER_WEBHOOK_SECRET"
> ```

`fc` calls talk to a remote HTTP API and must be run inside a scene:

```lua
scene.run(function()
  local me = fc.user("ruburi"):get()
  print(me.username, me.fid)
end)
```

Webhook events arrive asynchronously and are surfaced through `on.fc.webhook()` — these do **not** require a scene.

#### `fc.user(identifier)`

Look up a Farcaster user. The argument can be either:

- a **username** string → `fc.user("ruburi")`
- a **FID list** (table of numeric FIDs) → `fc.user({ 423406, 3 })`

Returns a `UserRx` builder. Chain `:get()` inside a scene to execute the request.

```lua
scene.run(function()
  -- by username -> returns a single User
  local user = fc.user("ruburi"):get()
  print("user:", user.username)

  -- by fids -> returns an array of Users
  local users = fc.user({ 423406, 3 }):get()
  print(users[1].username)
end)
```

**UserRx**

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:get()` | -- | `User` or `User[]` | Execute the lookup (scene-only). Single `User` when called with a username, array when called with a FID list |
| `:search([params])` | `table?` | `CursorRx` | Search users by username query. Only valid when `fc.user()` was created with a username string |
| `:casts([params])` | `table?` | `CursorRx` | Fetch casts for a single FID |
| `:notifications([params])` | `table?` | `CursorRx` | Fetch notifications for a single FID |
| `:follow_as(player, [args])` | `PlayerHandle, table?` | response | Follow one or more FIDs as the connected player (scene-only) |
| `:unfollow_as(player, [args])` | `PlayerHandle, table?` | response | Unfollow one or more FIDs as the connected player (scene-only) |

Cursor-returning methods are consumed with `:next()` inside a scene:

```lua
scene.run(function()
  local notifications = fc.user({ 423406 })
    :notifications({ limit = 25 })
    :next()

  print("notifications:", #notifications.notifications)
end)
```

The returned `User` table matches the Neynar user shape. Commonly used fields:

| Field | Type | Description |
|-------|------|-------------|
| `fid` | `number` | Farcaster ID |
| `username` | `string` | Handle (no `@`) |
| `display_name` | `string` | Display name |
| `pfp_url` | `string` | Profile picture URL |
| `registered_at` | `string` | ISO timestamp |
| `profile` | `table` | `{ bio = { text, mentioned_profiles, ... }, location?, banner? }` |
| `follower_count` | `number` | Follower count |
| `following_count` | `number` | Following count |
| `verified_addresses` | `table` | `{ eth_addresses, sol_addresses, primary = { eth_address?, sol_address? } }` |
| `url` | `string` | Profile URL |
| `score` | `number` | Neynar score |
| `pro` | `table?` | `{ status, subscribed_at, expires_at }` when present |

Some nested payloads (e.g. `app` on a webhook, `mentioned_profiles` inside a bio) use a **dehydrated** user shape with a subset of fields — `fid` is always present, the rest (`username`, `display_name`, `pfp_url`, `custody_address`, `score`) may be `nil`.

#### `fc.cast(identifier)`

Look up casts or start composing a cast. The identifier can be a hash, URL, or a table of identifiers, depending on the read method you call. With no identifier, use the returned `CastRx` as a composer. Chain fields, then call `:send_as(player)` inside a scene to publish as a connected player with an approved signer.

```lua
scene.run(function()
  local cast = fc.cast()
    :text("Roger.")
    :reply_to(parent_hash)   -- optional
    :embed_url("https://example.com")
    :send_as(p)

  print("sent:", cast.hash)
end)
```

**CastRx**

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:get([options])` | `table?` | `Cast` or `Cast[]` | Fetch one or more casts (scene-only) |
| `:convo([options])` | `table?` | `CursorRx` | Fetch a cast conversation thread |
| `:reactions([options])` | `table?` | `CursorRx` | Fetch all reactions for a cast |
| `:likes([options])` | `table?` | `CursorRx` | Fetch likes for a cast |
| `:recasts([options])` | `table?` | `CursorRx` | Fetch recasts for a cast |
| `:text(s)` | `string` | self | Set the cast body |
| `:reply_to(hash)` | `string` | self | Make this cast a reply to another cast (by hash). Omit for a top-level cast |
| `:channel(id)` | `string` | self | Post into a channel |
| `:quote(cast_id)` | `table` | self | Attach a quoted cast embed. Counts toward the 4-embed limit |
| `:embed_url(url)` | `string` | self | Attach a URL embed. Counts toward the 4-embed limit |
| `:send_as(player, [args])` | `PlayerHandle, table?` | `CreatedCast` | Publish the cast as a connected player (scene-only) |
| `:like_as(player, [args])` | `PlayerHandle, table?` | response | Like a cast as a connected player (scene-only) |
| `:recast_as(player, [args])` | `PlayerHandle, table?` | response | Recast as a connected player (scene-only) |
| `:unlike_as(player, [args])` | `PlayerHandle, table?` | response | Remove a like as a connected player (scene-only) |
| `:unrecast_as(player, [args])` | `PlayerHandle, table?` | response | Remove a recast as a connected player (scene-only) |

`:get()` accepts `sort_type`; use `Const.CastSort` values instead of raw strings:

```lua
scene.run(function()
  local casts = fc.cast({ hash_a, hash_b })
    :get({ sort_type = Const.CastSort.Recent })
end)
```

`:send_as()` returns the created cast:

| Field | Type | Description |
|-------|------|-------------|
| `hash` | `string` | Hash of the new cast |
| `author` | `{ fid }` | Author reference |
| `text` | `string` | Final cast text |

`args` is optional and can include `app_fid` when a server has multiple configured Farcaster apps. If omitted, ROCK uses the default app FID from config.

#### `on.fc.webhook()`

Fires when the engine receives an inbound Farcaster webhook. The handler receives a single `WebhookEvent`:

```lua
on.fc.webhook()
  :where(function(event) return event.type == "cast.created" end)
  :select(function(event) return event.data end)
  :each(function(cast)
    print(string.format("new mention. hash: %s. text: %s", cast.hash, cast.text))

    -- Webhooks are not tied to a connected player. To write back, publish from
    -- a player-scoped handler with an approved signer using fc.cast():send_as(p).
  end)
```

**WebhookEvent**

| Field | Type | Description |
|-------|------|-------------|
| `type` | `string` | Event discriminator, e.g. `"cast.created"` |
| `data` | `table` | Event-specific payload (shape depends on `type`) |

> Currently `"cast.created"` is the only supported event type. More event types may be added later.

**`cast.created` data**

| Field | Type | Description |
|-------|------|-------------|
| `object` | `string` | Always `"cast"` |
| `hash` | `string` | Cast hash |
| `author` | `User` | Full user record of the caster |
| `app` | `UserDehydrated` | App that created the cast |
| `thread_hash` | `string` | Root of the thread |
| `parent_hash` | `string` | Parent cast hash (if reply) |
| `parent_url` | `string?` | Parent URL (channel casts) |
| `root_parent_url` | `string?` | Root parent URL |
| `parent_author` | `{ fid }` | Parent author reference |
| `text` | `string` | Cast body |
| `timestamp` | `string` | ISO timestamp |
| `embeds` | `any[]` | Embed payloads |
| `channel` | `any?` | Channel data if posted in a channel |
| `reactions` | `table` | `{ likes_count, recasts_count, likes, recasts }` |
| `replies` | `table` | `{ count }` |
| `mentioned_profiles` | `User[]` | Users mentioned in the text |
| `mentioned_profiles_ranges` | `table[]` | `[{ start, end }]` character ranges for each mention |
| `mentioned_channels` | `ChannelDehydrated[]` | Channels mentioned |
| `mentioned_channels_ranges` | `table[]` | Character ranges for channel mentions |
| `event_timestamp` | `string` | ISO timestamp of the event |

Use standard reactive operators (`:where`, `:select`, `:throttle`, `:take`, …) to filter and transform webhook events like any other event.

#### End-to-end Example

```lua
-- at startup, resolve a user
on.world.awake():each(function()
  scene.run(function()
    local user = fc.user("ruburi"):get()
    print("user:", user.username)
  end)
end)

-- reply whenever someone mentions the bot
on.fc.webhook()
  :where(function(event) return event.type == "cast.created" end)
  :select(function(event) return event.data end)
  :each(function(cast)
    print(string.format("new mention. hash: %s. text: %s", cast.hash, cast.text))

    -- Webhooks are not tied to a connected player. To write back, publish from
    -- a player-scoped handler with an approved signer using fc.cast():send_as(p).
  end)
```

---

## Const

The `Const` plugin exposes engine constants to Lua. Use constants instead of raw strings where possible.

### Input constants

**`Input.Keyboard`:**
`KeyQ`, `KeyW`, `KeyE`, `KeyR`, `KeyT`, `KeyY`, `KeyU`, `KeyI`, `KeyO`, `KeyP`,
`KeyA`, `KeyS`, `KeyD`, `KeyF`, `KeyG`, `KeyH`, `KeyJ`, `KeyK`, `KeyL`,
`KeyZ`, `KeyX`, `KeyC`, `KeyV`, `KeyB`, `KeyN`, `KeyM`,
`LeftShift`, `RightShift`, `LeftCtrl`, `RightCtrl`,
`Space`, `Tab`, `CapsLock`, `Enter`, `Backspace`,
`ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`

**`Input.Mouse`:**
`Left`, `Right`, `Middle`, `Scroll`

**`Input.Controller`:**
`DPadUp`, `DPadDown`, `DPadLeft`, `DPadRight`,
`LeftStick`, `RightStick`, `LeftBumper`, `RightBumper`,
`LeftTrigger`, `RightTrigger`,
`ButtonY`, `ButtonA`, `ButtonX`, `ButtonB`

**`Input.Stick`:**
`LeftStick`, `RightStick`

### Farcaster constants

Use `Const.CastSort` for `fc.cast(...):get({ sort_type = ... })`.

**`CastSort`:**
`Trending`, `Likes`, `Recasts`, `Replies`, `Recent`

```lua
scene.run(function()
  local latest = fc.cast({ hash_a, hash_b })
    :get({ sort_type = Const.CastSort.Recent })
end)
```

### Area shapes

Use `Const.AreaShape` for spatial queries.
```lua
local Shape = Const.AreaShape
{Shape.Circle, Shape.Square, Shape.Diamond}

-- Example
local obj = entity.query()
  :in_room(room)
  :at({
    position = pos,
    radius = 1,
    shape = Const.AreaShape.Square
  })
  :first()
```

---

## Reactive Operators

Many builders in ROCK support **reactive operators** -- chainable methods that control *when* and *how* events fire.

### Core Pipeline

| Operator | Description |
|----------|-------------|
| `:take(n)` | Fire for the first N events, then stop |
| `:skip(n)` | Ignore the first N events, then fire for all |
| `:throttle(secs)` | Minimum time between fires |

These compose. The combination of `:take` and `:skip` creates repeating patterns:

```lua
-- fire once, then done
:take(1)

-- skip 5, then fire forever
:skip(5)

-- skip 2, take 1, repeat (fires on 3rd, 6th, 9th, ...)
:skip(2):take(1)

-- take 3, skip 1, repeat (skips every 4th)
:take(3):skip(1)

-- take(1) + take(1) = take(2) (fires twice, then done)
:take(1):take(1)

-- skip(1) + skip(1) = skip(2) (skip first 2, then fire forever)
:skip(1):skip(1)
```

### Operator Pipeline

| Operator | Description |
|----------|-------------|
| `:where(fn)` | Filter -- only fire if the predicate returns true |
| `:select(fn)` | Map -- transform the arguments passed to the handler |
| `:changed()` | Deduplicate -- only fire when the value differs from last time |

```lua
-- filter: only handle players with id < 10
on.player.online()
  :where(function(p) return p:id() < 10 end)
  :each(function(p) print("early player") end)

-- map: extract just the x coordinate
on.player.input()
  :bind_action("Move")
  :select(function(p, data) return data.x end)
  :each(function(x) print("x =", x) end)
```

### Where They Apply

| Builder | Core (take/skip/throttle) | Operators (where/select/changed) |
|---------|:---:|:---:|
| `on.*` events | yes | yes |
| `entity.query()` | yes | yes |
| Entity `sync()` | yes | field masks only |
| Memory `sync()` | yes | yes |

---

## Network Replication

The engine automatically sends entity state to connected clients. You control *what* gets sent, *to whom*, and *how often* using **sync policies**.

### How It Works

1. Entities exist in **rooms**
2. Players see through **vision anchors** attached to entities
3. A player receives updates for entities visible to their anchors
4. The engine builds per-player snapshots each tick, sending only what's relevant

```lua
local player_ent = player_bp:spawn():room("world")
p:vision():attach(player_ent)
```

### Defining Policies

Use `:sync()` on blueprints or entity handles, then chain options and `:commit()`:

```lua
-- replicate all fields of all entities from this blueprint
player_bp:sync():commit()

-- replicate only specific fields
zombie_bp:sync()
  :only(function(c) return { c.position, "health", "alive" } end)
  :commit()

-- hide specific fields
secret_bp:sync()
  :hide(function(c) return { c.position } end)
  :commit()
```

Field selectors accept both **component keys** (`c.position`, `c.rotation`, `c.name`, `c.owned_by`, `c.control`, `c.sprite_2d`, `c.sprite_char`) and **custom field names** as strings.

Replication fields use a compact 64-bit mask. Built-in entity fields reserve part of that budget (`position`, `rotation`, `control`, `sprite_2d`, `sprite_char`, `owned_by`, `blueprint`, `name`, `room`), so the current practical limit is **55 custom top-level fields** per entity. If more engine components are added later, this custom field budget gets smaller.

### Spatial Filters

Control the area within which entities are replicated:

```lua
-- replicate to everyone (default)
bp:sync():global():commit()

-- replicate within a radius from the entity's position
bp:sync():radius(20):commit()

-- replicate within a fixed area
bp:sync():area({ position = { x = 10, y = 5 }, radius = 10 }):commit()
```

### Throttling Replication

```lua
-- send updates at most every 0.5 seconds
bp:sync():throttle(0.5):commit()

-- send only once (snapshot on first sight)
bp:sync():take(1):commit()
```

### Policy Handles

`:commit()` returns a handle that you can use to modify or revoke policies at runtime:

```lua
local policy = entity:sync()
  :only(function(c) return { c.position } end)
  :radius(50)
  :commit()

-- change spatial filter dynamically
policy:global()
policy:area({ position = { x = 10, y = 5 }, radius = 10 })
policy:radius(25)

-- update room routing
policy:room("new_room")

-- remove the policy entirely
policy:revoke()
```

### Clearing Policies

Revoke all policies for a target:

```lua
-- clear all policies for a blueprint
bp:sync():clear()
```

### Memory Node Replication

You can also replicate database values to clients reactively:

```lua
-- replicate a memory node to clients in an area
memory.node("player/37")
  :where(function(p) return p.hp > 30 end)
  :select(function(p) return p.name end)
  :area({ position = { x = 42, y = 42 }, radius = 20 })
  :throttle(0.1)
  :room("lobby")
  :commit()
```

---

## Components Reference

Built-in entity components and their fields:

| Component | Method | Fields | Type |
|-----------|--------|--------|------|
| Position | `:position()` | `{ x, y }` | `f32, f32` |
| Rotation | `:rotation()` | single value | `u8` (0-255) |
| Control | `:control()` | `{ speed }` | `u32` |
| Sprite2D | `:sprite()` | `{ texture, scale, layer, visible }` | `string, {x,y}, u32, bool` |
| SpriteChar | `:char()` | `{ char, color, bg_color, visible }` | `string, string, string?, bool` |
| OwnedBy | `:owned_by()` | single value | `PlayerId (number)` |
| Name | `:name()` | single value | `string` |
| Room | `:room()` | set/get room name | `string` |

### Custom Data

In addition to built-in components, every entity can carry arbitrary **custom data** as a Lua table:

```lua
-- on a blueprint
entity.blueprint():custom({ health = 100, weapon = "axe", alive = true })

-- on a live entity
ent:custom({ health = 50 })                          -- replace
ent:custom(function(c) c.health = c.health - 10; return c end)  -- merge
local data = ent:custom()                              -- read
```

Custom fields are replicated to clients alongside components when included in sync policies (referenced by string name in `:only()` / `:hide()`).

Custom data is tracked at the **top level**. Updating `health` only replicates `custom.health`, not the whole custom table. If a top-level custom key disappears, the server sends that key as `null` in the next update; clients should treat `null` in `custom` as a delete marker.

```lua
ent:custom({ health = 100, weapon = "axe" })

ent:custom(function(c)
  c.health = 80
  c.weapon = nil
  return c
end)
```

The update can look like this:

```json
{ "custom": { "health": 80, "weapon": null } }
```

Nested tables are still normal JSON values. If `inventory` is one custom field and one slot inside it changes, the whole `inventory` value is replicated. Use separate top-level custom fields when you want fine-grained replication.

---

## WebSocket Protocol

The server communicates via JSON over WebSocket on `ws://127.0.0.1:3000`. Query params passed at connection time are available as the second argument to `on.player.online()`.

The server sends WebSocket ping frames every 25 seconds to keep idle connections alive and detect dead sockets.

### Client to Server

```json
{ "t": "input", "d": { "id": 1, "data": { "x": 0.5, "y": -1.0 } } }
{ "t": "input", "d": { "id": 2, "data": true } }
{ "t": "chat",  "d": "hello world" }
```

### Server to Client

**World snapshot** (entity state, sent each tick if changed):
```json
{
  "t": "world",
  "d": {
    "tick": 1234,
    "rooms": {
      "room1": {
        "spawn": { "1": { "position": { "x": 5, "y": 10 }, "custom": { "health": 100 } } },
        "update": { "1": { "position": { "x": 6, "y": 10 } } },
        "state": {}
      }
    },
    "despawn": [3, 7]
  }
}
```

**Signal** (custom game event):
```json
{ "t": "signal", "d": { "name": "Identity", "data": { "pid": 1 } } }
```

**System** (engine event):
```json
{ "t": "system", "d": "PlayerKicked" }
```

### Authentication

Protected worlds authenticate WebSocket sessions from an HTTP cookie or from a `token` query parameter. By default the cookie is named `rock_session`; override the name with `ROCK_SESSION_COOKIE`.

Use `?auth=ticket` or `?auth=farcaster` to select the verifier. If only one provider is configured, the server selects it automatically. If multiple providers are configured, clients must select one explicitly.

```txt
Cookie: rock_session=...
ws://127.0.0.1:3000/?auth=ticket&token=...
ws://127.0.0.1:3000/?auth=farcaster&token=...
```

Tickets use JWT (HS256). Farcaster Quick Auth uses RS256.

To allow one-shot anonymous sessions in an otherwise protected world, set:

```toml
[auth]
providers = ["farcaster"]
allow_anonymous = true
```

Anonymous clients connect without `auth` and without `token`:

```txt
ws://127.0.0.1:3000/
```

For anonymous players, `p:who()` and `p:fid()` return `nil`; use `p:id()` for runtime-local player identity and cleanup.

Protected cookie sessions also check the WebSocket `Origin` header against `ROCK_ALLOWED_ORIGINS`. This blocks browser-based cross-site WebSocket hijacking. Set it to the exact frontend origins that are allowed to open authenticated sockets, for example:

```env
ROCK_ALLOWED_ORIGINS=https://game.example.com,http://localhost:5173
```

---

## Geodes

Geodes are modular plugin packages. Place them in `geodes/`:

```txt
geodes/
  my_geode/
    geode.toml          -- manifest
    glyphs/             -- globals, utilities (loaded first)
    systems/            -- game logic (loaded second)
```

Geodes are loaded before the gamemode script. Use them to share reusable code across gamemodes.

### Importing geodes

Glyphs can be imported from Lua with `require` using the geode name as the module prefix:

For example:

```txt
geodes/
  worldkit/
    glyphs/
      grid.lua
      colors.lua
```

becomes:

```lua
local Grid = require("worldkit.grid")
local Colors = require("worldkit.colors")
```

Systems are loaded automatically after glyphs. They are intended for event listeners, runtime hooks, and gamemode-level behavior that should be injected when the geode is loaded.

---

## Impromptu (Live Coding)

The engine exposes a `POST /impromptu` endpoint for injecting Lua code at runtime:

```bash
curl -X POST http://127.0.0.1:3000/impromptu \
  -H "Content-Type: application/json" \
  -H "X-Rock-Impromptu-Token: $ROCK_IMPROMPTU_TOKEN" \
  -d '{"code": "print(\"hello from live code\")", "name": "test"}'
```

This fires `on.world.impromptu()` events and executes the code within the running game loop. Useful for debugging and live development.

`/impromptu` is protected by `ROCK_IMPROMPTU_TOKEN`, not by client IP. This matters behind reverse proxies such as nginx or Caddy: the engine may see the proxy as a local peer, so loopback checks are not a reliable security boundary. Requests without the correct `X-Rock-Impromptu-Token` header are rejected with `404`.

The request body is capped at 256 KiB. Farcaster webhook bodies are capped at 1 MiB.

---

## Static Assets

Everything under `assets/` is served over HTTP at `/assets/*`. This is where you drop textures, sounds, or any other static files the client needs to render your world.

```
assets/
  packs/
    basic/
      textures/
        frutiger_tile.png
        lamp_small.png
      music/
        loony_tunes.js
```

Fetched with:

```
http://127.0.0.1:3000/assets/packs/basic/textures/lamp_small.png
```

No configuration needed. Drop a file into `assets/`, it's live on the next request.
