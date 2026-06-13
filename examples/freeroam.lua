-- ROCK freeroam example
-- Copy to gamemodes/freeroam.lua and set [gamemode].name = "freeroam".

local WORLD_ROOM = "world"
local MAP_SIZE = 32
local PLAYER_SPEED = 1

local SPAWNS = {
	{ x = 8, y = 8 },
	{ x = 16, y = 16 },
	{ x = 24, y = 8 },
}

local function clamp(value, min, max)
	if value < min then
		return min
	end
	if value > max then
		return max
	end
	return value
end

local function random_spawn()
	return SPAWNS[math.random(#SPAWNS)]
end

local keyboard = input.bindings.keyboard
local controller = input.bindings.controller
local stick = input.bindings.stick

input
	.new()
	:vector()
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

input
	.new()
	:button()
	:defaults({
		keyboard = { keyboard.KeyE },
		controller = { controller.ButtonA },
	})
	:register("Cheer")

local player_bp = entity
	.blueprint()
	:name("player")
	:position({ x = 0, y = 0 })
	:custom({ health = 100, score = 0, alive = true })

local rock_bp = entity
	.blueprint()
	:name("rock")
	:position({ x = 0, y = 0 })
	:custom({ solid = true })

player_bp
	:sync()
	:only(function(c)
		return { c.position, c.owned_by, "health", "score", "alive" }
	end)
	:commit()

rock_bp
	:sync()
	:only(function(c)
		return { c.position, "solid" }
	end)
	:commit()

on.world.awake():take(1):each(function()
	print("[freeroam] booting example world")

	for x = 6, 26, 4 do
		rock_bp:spawn():position({ x = x, y = 12 }):room(WORLD_ROOM)
	end
end)

on.player.online():each(function(p)
	local pid = p:id()
	local spawn = random_spawn()

	local ent = player_bp
		:spawn()
		:owned_by(pid)
		:position({ x = spawn.x, y = spawn.y })
		:name("player_" .. pid)
		:room(WORLD_ROOM)

	p:presence():enter(WORLD_ROOM)
	p:vision():attach(ent)

	p:signal("Identity"):data({ pid = pid, room = WORLD_ROOM }):send()
	p:signal("Message"):data({ text = "Welcome to ROCK freeroam." }):send()

	print(string.format("[freeroam] player %d joined at (%d, %d)", pid, spawn.x, spawn.y))
end)

on.player.offline():each(function(snapshot)
	print("[freeroam] player disconnected", snapshot:who() or "anonymous")
end)

on.player.input()
	:bind_action("Move")
	:each(function(p, input)
		local pid = p:id()

		entity.query():owned_by(pid):blueprint(player_bp):each(function(ent)
			local data = ent:custom()
			if not data.alive then
				return
			end

			local pos = ent:position()
			local nx = clamp(pos.x + ((input.x or 0) * PLAYER_SPEED), 0, MAP_SIZE - 1)
			local ny = clamp(pos.y + ((input.y or 0) * PLAYER_SPEED), 0, MAP_SIZE - 1)

			local blocked = entity
				.query()
				:blueprint(rock_bp)
				:at({ position = { x = nx, y = ny }, radius = 0 })
				:count() > 0

			if blocked then
				p:signal("Message"):data({ text = "A rock blocks the way." }):send()
				return
			end

			ent:position({ x = nx, y = ny })
		end)
	end)

on.player.input()
	:bind_action("Cheer")
	:each(function(p)
		player
			.broadcast()
			:signal("Message")
			:room(WORLD_ROOM)
			:data({ text = string.format("Player %d cheers.", p:id()) })
			:send()
	end)
