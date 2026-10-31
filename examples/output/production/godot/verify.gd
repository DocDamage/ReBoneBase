extends SceneTree
func _initialize() -> void:
    _verify.call_deferred()
func _verify() -> void:
    var script = load("res://ReBonePlayer.gd")
    if script == null:
        quit(1)
        return
    var actor = AnimatedSprite2D.new()
    actor.set_script(script)
    actor.manifest = "res://package.json"
    root.add_child(actor)
    await process_frame
    if actor.sprite_frames.get_animation_names().size() != 2:
        push_error("Expected both exported clips")
        quit(2)
        return
    var markers: Array = []
    actor.marker.connect(func(n: String, p: String): markers.append([n,p]))
    actor.play("Walk")
    actor.frame = 5
    await process_frame
    await process_frame
    if markers.is_empty() or actor.get_node("ReBoneRegions").get_child_count() == 0:
        push_error("Events or collision regions did not load")
        quit(3)
        return
    actor.stop()
    print("REBONE_GODOT_SMOKE_OK")
    quit(0)
