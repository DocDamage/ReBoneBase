extends AnimatedSprite2D
signal marker(name: String, payload: String)
@export_file("*.json") var manifest: String
var metadata: Dictionary
var regions: Area2D
func _ready() -> void:
    texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
    var parsed = JSON.parse_string(FileAccess.get_file_as_string(manifest))
    metadata = _normalize(parsed if parsed is Dictionary else {})
    if metadata.is_empty():
        push_error("Invalid or unsupported ReBoneBase manifest")
        return
    sprite_frames = SpriteFrames.new()
    sprite_frames.remove_animation("default")
    centered = false
    offset = -Vector2(metadata.pivot.x, metadata.pivot.y)
    regions = Area2D.new()
    regions.name = "ReBoneRegions"
    add_child(regions)
    var textures = []
    for page in metadata.pages:
        textures.append(load(manifest.get_base_dir().path_join(page.file)) as Texture2D)
    for clip in metadata.clips:
        var clip_name = StringName(clip.animation)
        sprite_frames.add_animation(clip_name)
        sprite_frames.set_animation_loop(clip_name, clip.looped)
        sprite_frames.set_animation_speed(clip_name, clip.fps)
        for item in clip.frames:
            var atlas = AtlasTexture.new()
            atlas.atlas = textures[int(item.page)]
            atlas.region = Rect2(item.rect.x, item.rect.y, item.rect.w, item.rect.h)
            # Restore trimmed transparent space so every frame keeps its source size.
            atlas.margin = Rect2(item.trim.x, item.trim.y, item.source_size.w - item.rect.w, item.source_size.h - item.rect.h)
            sprite_frames.add_frame(clip_name, atlas, item.duration_ms * clip.fps / 1000.0)
    frame_changed.connect(_on_frame)
    play(StringName(metadata.clips[0].animation))
    _on_frame()
# Upgrade a version 1 package to the version 2 shape; reject newer versions.
func _normalize(data: Dictionary) -> Dictionary:
    if data.get("format", "") == "rebonebase-sprites":
        return data if int(data.get("format_version", 0)) <= 2 else {}
    if int(data.get("version", 0)) != 1:
        return {}
    var pages = []
    var clips = []
    for clip in data.clips:
        var page = pages.size()
        pages.append({"file": clip.file})
        var frames = []
        for item in clip.frames:
            frames.append({"page": page, "rect": {"x": item.x, "y": item.y, "w": item.w, "h": item.h}, "source_size": {"w": data.width, "h": data.height}, "trim": {"x": 0, "y": 0}, "duration_ms": item.duration_ms, "events": item.events, "boxes": item.boxes})
        clips.append({"id": clip.get("id", ""), "animation": clip.animation, "looped": clip.looped, "fps": clip.fps, "frames": frames})
    return {"pivot": {"x": data.width * data.pivot.x, "y": data.height * data.pivot.y}, "pages": pages, "clips": clips}
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
