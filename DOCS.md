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

#### `config.toml`:

```toml
[gamemode]
# Gamemode file without .lua extension.
name = "farcaster"

[auth]
# Optional. Enables auth providers and requires authenticated sessions.
providers = ["ticket", "farcaster"]

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
```

* `HOST`: address to bind the server (default: `127.0.0.1`)
* `PORT`: port to listen on (default: `3000`)

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

Every global table in your Lua environment is a **plugin**. There are 10 plugins:

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

Rooms are spatial groups for network replication. Players and entities exist in rooms. A player only receives updates for entities in rooms they've joined:

```lua
-- player joins a room
p:room():enter("lobby")

-- entity lives in a room
zombie:spawn():room("lobby")
```

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
local keyboard = input.bindings.keyboard
local controller = input.bindings.controller
local stick = input.bindings.stick

input.new():vector()
  :defaults({
    keyboard = {
      up = { keyboard.KeyW, keyboard.ArrowUp },
      down = { keyboard.KeyS, keyboard.ArrowDown },
      left = { keyboard.KeyA, keyboard.ArrowLeft },
      right = { keyboard.KeyD, keyboard.ArrowRight },
    },
    controller = {
      up = { controller.DPadUp },
      down = { controller.DPadDown },
      left = { controller.DPadLeft },
      right = { controller.DPadRight },
    },
    stick = stick.LeftStick,
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
on.player.online():each(function(p)
  local pid = p:id()

  -- spawn a player entity
  local ent = player_bp:spawn()
    :owned_by(pid)
    :position({ x = 5, y = 5 })
    :room("world")

  -- player joins the room and attaches vision to their entity
  p:room():enter("world")
  p:vision():attach(ent)

  -- send the player their identity
  p:signal("Identity"):data({ pid = pid }):send()

  print(string.format("Player %d joined", pid))
end)

on.player.offline():each(function(p)
  -- clean up: despawn all entities owned by this player
  entity.query():owned_by(p:id()):each(function(ent)
    ent:despawn()
  end)
  print(string.format("Player %d left", p:id()))
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
| `on.player.online()` | `PlayerHandle` | Player connected |
| `on.player.offline()` | `PlayerHandle` | Player disconnected |
| `on.player.input()` | `PlayerHandle, InputAction` | Player sent input (see `:bind_action` below) |
| `on.player.enter()` | `PlayerHandle, room_name` | Player entered a room |
| `on.player.exit()` | `PlayerHandle, room_name` | Player exited a room |
| `on.player.chat()` | `PlayerHandle, message` | Player sent a chat message |
| `on.timer.fire()` | `timer_id, data` | A timer fired (see `:named` below) |
| `on.fc.webhook()` | `WebhookEvent` | Farcaster webhook received (see [fc](#fc)) |

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
| `:room([name])` | optional `string` | room_id (get) or self (set) | Get/set room |
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
  :at({ position = { x = 0, y = 0 }, radius = 50 })
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
| `:at(area)` | `{ position, radius }` | self | Filter by spatial area |
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
| `:room()` | `PlayerRoom` | Access room management |
| `:vision()` | `PlayerVision` | Access vision/anchor management |
| `:connection_params()` | `table` | Query params from the WebSocket connection URL |
| `:who()` | `string?` | Auth identity, e.g. `fc:423406`, or `nil` for anon sessions |

**Connection params.** When a client connects via `ws://host:port/?room=0xabc&name=Bob`, the query string is captured at handshake time and exposed as a Lua table:

```lua
on.player.online():each(function(p)
  local params = p:connection_params()
  local room_hash = params.room     -- "0xabc"
  local display_name = params.name  -- "Bob"

  if not room_hash then
    p:kick()
    return
  end

  p:room():enter(room_hash)
end)
```

All values are strings (or `nil` if the param is absent). Use `tonumber()` to coerce numeric params yourself.

#### PlayerRoom

| Method | Args | Description |
|--------|------|-------------|
| `:enter(name)` | `string` | Join a room |
| `:exit([name])` | optional `string` | Leave a room (or all rooms if no name) |

#### PlayerVision

Vision determines what a player can "see" for network replication. You attach the player's vision to entities → the player will receive updates for entities near their anchors, in the same rooms.

| Method | Args | Description |
|--------|------|-------------|
| `:attach(ent)` | `EntityHandle` | Attach vision to an entity |
| `:detach([ent])` | optional `EntityHandle` | Detach from an entity (or all if no arg) |

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
| `:signal([name])` | optional `string` | `SignalRx` | Create a signal to all players |

---

### `input`

Register input actions that clients can send. Inputs have a **kind** (vector, button, or axis) and **default key bindings**.

#### `input.new()`

Creates a new input builder. Returns `InputRx`.

```lua
local keyboard = input.bindings.keyboard
local controller = input.bindings.controller
local stick = input.bindings.stick

-- vector input (WASD movement)
input.new():vector()
  :defaults({
    keyboard = {
      up = { keyboard.KeyW, keyboard.ArrowUp },
      down = { keyboard.KeyS, keyboard.ArrowDown },
      left = { keyboard.KeyA, keyboard.ArrowLeft },
      right = { keyboard.KeyD, keyboard.ArrowRight },
    },
    controller = {
      up = { controller.DPadUp },
      down = { controller.DPadDown },
      left = { controller.DPadLeft },
      right = { controller.DPadRight },
    },
    stick = stick.LeftStick,
  })
  :register("Move")

-- button input
input.new():button()
  :defaults({
    keyboard = { keyboard.KeyE },
    controller = { controller.ButtonA },
  })
  :register("Use")

-- axis input
input.new():axis()
  :defaults({
    keyboard = { negative = { keyboard.KeyA }, positive = { keyboard.KeyD } },
    stick = stick.LeftStick,
  })
  :register("Strafe")
```

#### InputRx

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:vector()` | -- | self | Input produces `{ x, y }` (Vector2D) |
| `:axis()` | -- | self | Input produces a float (-1.0 to 1.0) |
| `:button()` | -- | self | Input produces a boolean (pressed/released) |
| `:defaults(t)` | `table` | self | Set default key bindings (shape depends on kind) |
| `:register(name)` | `string` | -- | Register the input action by name |

When a player sends input for a registered action, it fires `on.player.input()`. Use `:bind_action("Move")` to filter:

```lua
on.player.input()
  :bind_action("Move")
  :each(function(p, data)
    -- data = { x = 1, y = 0 } for vector
    -- data = true/false for button
    -- data = 0.5 for axis
  end)
```

#### Key Constants

Access via `input.bindings.*`:

**`input.bindings.keyboard`:**
`KeyQ`, `KeyW`, `KeyE`, `KeyR`, `KeyT`, `KeyY`, `KeyU`, `KeyI`, `KeyO`, `KeyP`,
`KeyA`, `KeyS`, `KeyD`, `KeyF`, `KeyG`, `KeyH`, `KeyJ`, `KeyK`, `KeyL`,
`KeyZ`, `KeyX`, `KeyC`, `KeyV`, `KeyB`, `KeyN`, `KeyM`,
`LeftShift`, `RightShift`, `LeftCtrl`, `RightCtrl`,
`Space`, `Tab`, `CapsLock`, `Enter`, `Backspace`,
`ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`

**`input.bindings.mouse`:**
`Left`, `Right`, `Middle`, `Scroll`

**`input.bindings.controller`:**
`DPadUp`, `DPadDown`, `DPadLeft`, `DPadRight`,
`LeftStick`, `RightStick`, `LeftBumper`, `RightBumper`,
`LeftTrigger`, `RightTrigger`,
`ButtonY`, `ButtonA`, `ButtonX`, `ButtonB`

**`input.bindings.stick`:**
`LeftStick`, `RightStick`

---

### `memory`

Persistent key-value storage backed by SQLite with an in-memory cache.

Keys are hierarchical paths like `"player/42/health"`. Keys ending with `/` are **prefixes** that return a map of all nested values.

#### Global API (available everywhere)

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `memory.peek(key)` | `string` | value or nil | Read from cache only (fast, synchronous, may be stale or missing) |
| `memory.node(key)` | `string` | `SyncRx` | Create a replication policy for a memory node (see [Network Replication](#network-replication)) |

#### Scene API (available inside scenes only)

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `memory.recall(key)` | `string` | value | Read from cache; if missing, fetch from DB first |
| `memory.fetch(key)` | `string` | value | Always fetch the latest value from DB |
| `memory.store(key, value)` | `string`, any | -- | Write a value to DB. Prefix keys store a map of values |

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

Utility for room ID generation.

#### `room.generate_id()`

Returns a random unique string ID (nanoid). Use this when you need a dynamic room name:

```lua
local room_name = room.generate_id()
p:room():enter(room_name)
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

#### `fc.cast(signer_uuid)`

Start composing a cast. `signer_uuid` is the UUID of the Neynar signer authorized to post on behalf of a user. Returns a `CastRx` builder. Chain fields, then `:send()` inside a scene to publish.

```lua
scene.run(function()
  local cast = fc.cast(SIGNER_UUID)
    :text("Roger.")
    :reply_to(parent_hash)   -- optional
    :send()

  print("sent:", cast.hash)
end)
```

**CastRx**

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `:text(s)` | `string` | self | Set the cast body |
| `:reply_to(hash)` | `string` | self | Make this cast a reply to another cast (by hash). Omit for a top-level cast |
| `:send()` | -- | `CreatedCast` | Publish the cast (scene-only) |

`:send()` returns the created cast:

| Field | Type | Description |
|-------|------|-------------|
| `hash` | `string` | Hash of the new cast |
| `author` | `{ fid }` | Author reference |
| `text` | `string` | Final cast text |

> Note: `SIGNER_UUID` is not provided by the engine — set it yourself (e.g. as a Lua global or via `memory`) with the signer UUID from your Neynar app.

#### `on.fc.webhook()`

Fires when the engine receives an inbound Farcaster webhook. The handler receives a single `WebhookEvent`:

```lua
on.fc.webhook()
  :where(function(event) return event.type == "cast.created" end)
  :select(function(event) return event.data end)
  :each(function(cast)
    print(string.format("new mention. hash: %s. text: %s", cast.hash, cast.text))

    scene.run(function()
      local reply = fc.cast(SIGNER_UUID)
        :text("Roger.")
        :reply_to(cast.hash)
        :send()
      print("sent a response:", reply.hash)
    end)
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

    scene.run(function()
      local reply = fc.cast(SIGNER_UUID)
        :text("Roger.")
        :reply_to(cast.hash)
        :send()
      print("sent a response:", reply.hash)
    end)
  end)
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
2. Players join rooms via `p:room():enter("world")`
3. Players attach **vision anchors** to entities via `p:vision():attach(ent)`
4. The engine builds per-player snapshots each tick, sending only what's relevant

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

Field selectors accept both **component keys** (`c.position`, `c.rotation`, `c.name`, `c.owned_by`, `c.control`, `c.sprite`, `c.char`) and **custom field names** as strings.

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
| Room | `:room()` | set by name, get returns room ID | `string -> u64` |

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

---

## WebSocket Protocol

The server communicates via JSON over WebSocket on `ws://127.0.0.1:3000`. Query params passed at connection time are available via `p:connection_params()`.

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
      "12345": {
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

You can connect to protected worlds with:
```txt
ws://127.0.0.1:3000/?auth=ticket&token=...
ws://127.0.0.1:3000/?auth=farcaster&token=...
```

Omit both for anon sessions. Tickets use JWT (HS256). Farcaster Quick Auth uses RS256.

---

## Geodes

Geodes are modular plugin packages. Place them in `geodes/`:

```
geodes/
  my_geode/
    geode.toml          -- manifest
    glyphs/             -- globals, utilities (loaded first)
    blueprints/         -- entity templates (loaded second)
    systems/            -- game logic (loaded third)
    assets/             -- static files
```

Geodes are loaded before the gamemode script. Use them to share reusable code across gamemodes.

---

## Impromptu (Live Coding)

The engine exposes a `POST /impromptu` endpoint for injecting Lua code at runtime:

```bash
curl -X POST http://127.0.0.1:3000/impromptu \
  -H "Content-Type: application/json" \
  -d '{"code": "print(\"hello from live code\")", "name": "test"}'
```

This fires `on.world.impromptu()` events and executes the code within the running game loop. Useful for debugging and live development.

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
