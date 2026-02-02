scene.create{
  name = "test",
  action = function()
    local player = memory.recall("player/37")
    print(player)
  end
}

when.world.awakes(function ()
  print(memory.peek("player/42"))
  scene.run{
    action = function()
     local player = memory.fetch("player/42/")
     print(player)
    end
  }

  scene.play("test")
end)
