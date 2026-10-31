extends AnimatedSprite2D
signal marker(name: String, payload: String)
@export_file("*.json") var manifest: String
var metadata: Dictionary
var regions: Area2D
func _ready() -> void:
    texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
    metadata = JSON.parse_string(FileAccess.get_file_as_string(manifest))
    if metadata.is_empty():
        push_error("Invalid ReBoneBase manifest")
        return
    sprite_frames = SpriteFrames.new()
    sprite_frames.remove_animation("default")
    centered = false
    offset = -Vector2(metadata.width * metadata.pivot.x, metadata.height * metadata.pivot.y)
    regions = Area2D.new()
    regions.name = "ReBoneRegions"
    add_child(regions)
    for clip in metadata.clips:
        var clip_name = StringName(clip.animation)
        sprite_frames.add_animation(clip_name)
        sprite_frames.set_animation_loop(clip_name, clip.looped)
        sprite_frames.set_animation_speed(clip_name, clip.fps)
        var texture = load(manifest.get_base_dir().path_join(clip.file)) as Texture2D
        for item in clip.frames:
            var atlas = AtlasTexture.new()
            atlas.atlas = texture
            atlas.region = Rect2(item.x, item.y, item.w, item.h)
            sprite_frames.add_frame(clip_name, atlas, item.duration_ms * clip.fps / 1000.0)
    frame_changed.connect(_on_frame)
    play(StringName(metadata.clips[0].animation))
    _on_frame()
func _on_frame() -> void:
    for clip in metadata.clips:
        if clip.animation == str(animation):
            var item = clip.frames[mini(frame, clip.frames.size()-1)]
            for event in item.events:
                marker.emit(event.name, event.payload)
            _regions.call_deferred(item.boxes)
            break
func _regions(boxes: Array) -> void:
    for child in regions.get_children():
        regions.remove_child(child)
        child.queue_free()
    for region in boxes:
        var shape = CollisionPolygon2D.new()
        shape.name = str(region.id)
        shape.set_meta("kind", region.kind)
        shape.set_meta("region_name", region.name)
        var points = PackedVector2Array()
        for point in region.corners:
            points.append(Vector2(point.x, point.y) + offset)
        shape.polygon = points
        regions.add_child(shape)
