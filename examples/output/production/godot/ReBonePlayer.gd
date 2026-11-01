extends AnimatedSprite2D
## Emitted for every marker of every frame entered, in frame order.
signal marker(name: String, payload: String)
@export_file("*.json") var manifest: String
## Play the package's reference audio in sync with the animation.
@export var play_audio := true
var metadata: Dictionary
var regions: Area2D
var audio: AudioStreamPlayer
var _shapes: Array[CollisionPolygon2D] = []
var _quiet := false
var _audio_start := 0.0
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
    _setup_audio()
    frame_changed.connect(_on_frame_changed)
    play_clip(metadata.clips[0].animation)
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
            frames.append({"page": page, "rect": {"x": item.x, "y": item.y, "w": item.w, "h": item.h}, "source_size": {"w": data.width, "h": data.height}, "trim": {"x": 0, "y": 0}, "duration_ms": item.duration_ms, "source_time_ms": item.get("source_time_ms", 0), "events": item.events, "boxes": item.boxes})
        clips.append({"id": clip.get("id", ""), "name": clip.get("name", ""), "animation": clip.animation, "looped": clip.looped, "fps": clip.fps, "frames": frames})
    var result = {"pivot": {"x": data.width * data.pivot.x, "y": data.height * data.pivot.y}, "pages": pages, "clips": clips}
    if data.get("audio_file") != null:
        result["audio"] = {"file": data.audio_file, "start_ms": data.get("audio_start_ms", 0), "volume": data.get("audio_volume", 1.0), "enabled": data.get("audio_enabled", false)}
    return result
func _setup_audio() -> void:
    var info = metadata.get("audio")
    if not play_audio or not (info is Dictionary) or not info.get("enabled", false):
        return
    var stream = load(manifest.get_base_dir().path_join(info.file)) as AudioStream
    if stream == null:
        push_warning("ReBoneBase audio could not be loaded: " + str(info.file))
        return
    audio = AudioStreamPlayer.new()
    audio.name = "ReBoneAudio"
    audio.stream = stream
    audio.volume_db = linear_to_db(maxf(float(info.volume), 0.0001))
    add_child(audio)
    _audio_start = float(info.start_ms) / 1000.0
## Start an animation from its first frame.
func play_clip(animation_name: String) -> bool:
    if _clip(animation_name) == null:
        return false
    _quiet = true
    play(StringName(animation_name))
    set_frame_and_progress(0, 0.0)
    _quiet = false
    _enter(true)
    return true
## Return to the first frame, firing its markers, and play.
func restart() -> void:
    play_clip(str(animation))
## Pause the animation and its audio.
func pause_playback() -> void:
    pause()
    if audio:
        audio.stream_paused = true
## Continue from the current frame.
func resume_playback() -> void:
    play()
    if audio:
        audio.stream_paused = false
        _sync_audio(_current_frame())
## Play `base` (the authored clip name) facing `direction` (e, se, s, ...).
## Changing only the direction keeps the current frame and its progress and fires nothing.
func play_direction(base: String, direction: String) -> bool:
    var current = _clip(str(animation))
    for clip in metadata.clips:
        var facing = clip.get("direction")
        if clip.name == base and facing is Dictionary and facing.id == direction:
            var keep = current != null and current.name == base and current.frames.size() == clip.frames.size()
            if not keep:
                return play_clip(clip.animation)
            var keep_frame = frame
            var keep_progress = frame_progress
            _quiet = true
            play(StringName(clip.animation))
            set_frame_and_progress(keep_frame, keep_progress)
            _quiet = false
            _enter(false)
            return true
    return false
func _clip(animation_name: String):
    for clip in metadata.clips:
        if clip.animation == animation_name:
            return clip
    return null
func _current_frame():
    var clip = _clip(str(animation))
    return null if clip == null else clip.frames[mini(frame, clip.frames.size() - 1)]
func _on_frame_changed() -> void:
    if not _quiet:
        _enter(true)
# Start audio that becomes due inside a frame; stop it when the track or a one-shot clip ends.
func _process(_delta: float) -> void:
    if audio == null:
        return
    if not is_playing():
        if audio.playing and not audio.stream_paused:
            audio.stop()
        return
    var at = audio_target()
    var due = at >= 0.0 and at < audio.stream.get_length()
    if due and not audio.playing:
        audio.play(at)
    elif not due and audio.playing:
        audio.stop()
func _enter(fire: bool) -> void:
    var item = _current_frame()
    if item == null:
        return
    if fire:
        for event in item.events:
            marker.emit(event.name, event.payload)
    _apply_regions(item.boxes)
    _sync_audio(item)
## Where the audio should be for the current frame: the frame's retained source
## time minus the track's start. Negative before the track starts.
func audio_target() -> float:
    var item = _current_frame()
    if audio == null or item == null:
        return -1.0
    return float(item.get("source_time_ms", 0)) / 1000.0 - _audio_start + frame_progress * float(item.duration_ms) / 1000.0
func _sync_audio(item) -> void:
    if audio == null or item == null:
        return
    var at = audio_target()
    if at < 0.0 or at >= audio.stream.get_length():
        audio.stop()
    elif is_playing() and (not audio.playing or absf(audio_position() - at) > 0.08):
        audio.play(at)
## Audible audio position, compensated for mix granularity and output latency.
func audio_position() -> float:
    if audio == null or not audio.playing:
        return -1.0
    return audio.get_playback_position() + AudioServer.get_time_since_last_mix() - AudioServer.get_output_latency()
# One reused polygon per region; unused polygons are disabled, never freed.
func _apply_regions(boxes: Array) -> void:
    while _shapes.size() < boxes.size():
        var shape = CollisionPolygon2D.new()
        regions.add_child(shape)
        _shapes.append(shape)
    for i in _shapes.size():
        var shape = _shapes[i]
        if i >= boxes.size():
            shape.disabled = true
            continue
        var region = boxes[i]
        var points = PackedVector2Array()
        for point in region.corners:
            points.append(Vector2(point.x, point.y) + offset)
        shape.polygon = points
        shape.disabled = false
        shape.set_meta("id", region.id)
        shape.set_meta("kind", region.kind)
        shape.set_meta("region_name", region.name)
