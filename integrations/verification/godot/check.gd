extends SceneTree
## Runtime check of ReBonePlayer.gd against its package.json.
##   godot --headless --fixed-fps 240 --path <project> --script res://check.gd -- <player.gd> <package.json>
## Prints "CHECK <name> OK|FAIL <detail>" lines and "SUMMARY <json>".

var player
var package: Dictionary
var fired: Array = []
var failures := 0
var checks := 0
var dt := 0.0
## Real-time runs compare against the audio mixer clock; fixed-fps runs cannot.
var realtime := false
var drift: Array = []

func _initialize() -> void:
    _run.call_deferred()

func check(name: String, ok: bool, detail := "") -> void:
    checks += 1
    if not ok:
        failures += 1
        print("CHECK %s FAIL %s" % [name, detail])

func _run() -> void:
    var args = OS.get_cmdline_user_args()
    package = JSON.parse_string(FileAccess.get_file_as_string(args[1]))
    realtime = args.size() > 2 and args[2] == "realtime"
    dt = 1.0 / float(Engine.physics_ticks_per_second if Engine.max_fps == 0 else Engine.max_fps)
    player = load(args[0]).new()
    player.manifest = args[1]
    player.marker.connect(func(n, p): fired.append([str(player.animation), player.frame, n]))
    root.add_child(player)
    # The first frame's delta includes startup; the second is the steady step.
    await process_frame
    await process_frame
    dt = player.get_process_delta_time() if player.get_process_delta_time() > 0.0 else dt
    _placement()
    for clip in package.clips:
        await _timeline(clip)
    if package.clips.any(func(c): return c.get("direction") is Dictionary):
        await _direction_switch()
    else:
        await _pause_and_restart()
    var audio = {}
    if not drift.is_empty():
        drift.sort()
        audio = {"samples": drift.size(), "median_ms": drift[drift.size() / 2] * 1000.0, "p90_ms": drift[int(drift.size() * 0.9)] * 1000.0, "max_ms": drift[-1] * 1000.0}
        check("audio drift p90 under 120 ms", audio.p90_ms < 120.0, str(audio))
    print("SUMMARY " + JSON.stringify({"checks": checks, "failures": failures, "dt": dt, "realtime": realtime, "audio_drift": audio, "engine": Engine.get_version_info().string, "clips": package.clips.size()}))
    quit(1 if failures else 0)

func _placement() -> void:
    check("pivot offset", player.offset.is_equal_approx(-Vector2(package.pivot.x, package.pivot.y)), str(player.offset))
    for clip in package.clips:
        var anim = StringName(clip.animation)
        check("frame count " + clip.animation, player.sprite_frames.get_frame_count(anim) == clip.frames.size())
        check("loop " + clip.animation, player.sprite_frames.get_animation_loop(anim) == clip.looped)
        for i in clip.frames.size():
            var f = clip.frames[i]
            var t = player.sprite_frames.get_frame_texture(anim, i) as AtlasTexture
            var r = f.rect
            check("%s %d region" % [clip.animation, i], t.region == Rect2(r.x, r.y, r.w, r.h), str(t.region))
            check("%s %d trim" % [clip.animation, i], t.margin == Rect2(f.trim.x, f.trim.y, f.source_size.w - r.w, f.source_size.h - r.h), str(t.margin))
            check("%s %d size" % [clip.animation, i], t.get_size() == Vector2(f.source_size.w, f.source_size.h))
            check("%s %d page" % [clip.animation, i], t.atlas.resource_path.get_file() == package.pages[int(f.page)].file)
            var hold = player.sprite_frames.get_frame_duration(anim, i) / clip.fps
            check("%s %d hold" % [clip.animation, i], absf(hold - f.duration_ms / 1000.0) < 1e-6, str(hold))

# Frame shown at `t` by the exported holds, and the distance to the nearest boundary.
func expected_frame(clip, t: float) -> Array:
    var total = 0.0
    for f in clip.frames:
        total += f.duration_ms / 1000.0
    var time = fmod(t, total) if clip.looped else minf(t, total - 1e-9)
    var start = 0.0
    for i in clip.frames.size():
        var end = start + clip.frames[i].duration_ms / 1000.0
        if time < end:
            return [i, minf(time - start, end - time)]
        start = end
    return [clip.frames.size() - 1, 1.0]

# Markers expected from 0 to `t` seconds, as [frame, name].
func expected_markers(clip, t: float) -> Array:
    var out = []
    var start = 0.0
    var i = 0
    while true:
        for e in clip.frames[i].events:
            out.append([i, e.name])
        var end = start + clip.frames[i].duration_ms / 1000.0
        if end > t + 1e-6:
            return out
        start = end
        i += 1
        if i == clip.frames.size():
            if not clip.looped:
                return out
            i = 0
    return out

func _shapes() -> Array:
    return player.regions.get_children().filter(func(c): return c is CollisionPolygon2D)

func _timeline(clip) -> void:
    var total = 0.0
    var shortest = 1e9
    for f in clip.frames:
        total += f.duration_ms / 1000.0
        shortest = minf(shortest, f.duration_ms / 1000.0)
    fired.clear()
    player.play_clip(clip.animation)
    var name = clip.animation
    var duration = (total * 2.5 if clip.looped else total + 0.5)
    var pool = []
    var t = 0.0
    var audio_checked = 0
    # A coroutine resumes on process_frame before nodes process that frame, so
    # the state sampled after each await includes the previous frame's delta.
    var pending = player.get_process_delta_time()
    while t < duration:
        var want = expected_frame(clip, t)
        # Real-time runs measure audio drift only; exact frames are checked on the fixed clock.
        if realtime:
            if player.audio != null and player.is_playing() and player.audio.playing:
                var at_rt = player.audio_target()
                if at_rt > 0.03 and at_rt < player.audio.stream.get_length() - 0.06:
                    drift.append(absf(player.audio_position() - at_rt))
        elif want[1] > 2.5 * dt:
            check("%s frame at %.3f" % [name, t], player.frame == want[0], "got %d want %d" % [player.frame, want[0]])
            var item = clip.frames[player.frame]
            var active = _shapes().filter(func(s): return not s.disabled)
            check("%s regions at %.3f" % [name, t], active.size() == item.boxes.size(), "%d vs %d" % [active.size(), item.boxes.size()])
            for r in mini(active.size(), item.boxes.size()):
                var poly = active[r].polygon
                for c in item.boxes[r].corners.size():
                    var want_point = Vector2(item.boxes[r].corners[c].x, item.boxes[r].corners[c].y) + player.offset
                    check("%s region %d corner" % [name, r], poly[c].is_equal_approx(want_point), "%s vs %s" % [poly[c], want_point])
            if player.audio != null and player.is_playing():
                var at = item.source_time_ms / 1000.0 - package.audio.start_ms / 1000.0 + (t - (_frame_start(clip, t)))
                check("%s audio target at %.3f" % [name, t], absf(player.audio_target() - at) < 0.002 + dt, "%.4f vs %.4f" % [player.audio_target(), at])
                if at < -0.01:
                    check("%s audio silent before start" % name, not player.audio.playing)
                elif at > 0.03 and at < player.audio.stream.get_length() - 0.06:
                    audio_checked += 1
                    check("%s audio playing" % name, player.audio.playing)
        if pool.is_empty() and _shapes().size() > 0:
            pool = _shapes().map(func(s): return s.get_instance_id())
        await process_frame
        t += pending
        pending = player.get_process_delta_time()
    var now = _shapes().map(func(s): return s.get_instance_id())
    if realtime:
        return
    check(name + " reuses region polygons", pool.is_empty() or now.slice(0, pool.size()) == pool, "%s vs %s" % [pool, now])
    var got = fired.filter(func(m): return m[0] == name).map(func(m): return [m[1], m[2]])
    # Compare only when the run did not end right at a frame boundary.
    if expected_frame(clip, t)[1] > 2.0 * dt:
        var want_markers = expected_markers(clip, t)
        check(name + " markers in order", got == want_markers, "%s vs %s" % [got, want_markers])
    if not clip.looped:
        check(name + " stops on last frame", not player.is_playing() and player.frame == clip.frames.size() - 1, "playing %s frame %d" % [player.is_playing(), player.frame])
    if player.audio != null and clip.looped and dt < 0.02:
        check(name + " audio exercised", audio_checked > 0 or total * 2.5 < package.audio.start_ms / 1000.0)

func _frame_start(clip, t: float) -> float:
    var total = 0.0
    for f in clip.frames:
        total += f.duration_ms / 1000.0
    var time = fmod(t, total) if clip.looped else minf(t, total)
    var start = 0.0
    for f in clip.frames:
        var end = start + f.duration_ms / 1000.0
        if time < end:
            return t - time + start
        start = end
    return t - time + start

func _pause_and_restart() -> void:
    var clip = package.clips[0]
    player.play_clip(clip.animation)
    # Pause after the audio has started (its start lies inside the first clip).
    var until = Time.get_ticks_msec() + 600
    while Time.get_ticks_msec() < until:
        await process_frame
    var audio_was_playing = player.audio != null and player.audio.playing
    var frame = player.frame
    var progress = player.frame_progress
    fired.clear()
    player.pause_playback()
    for i in 30:
        await process_frame
    check("pause holds frame", player.frame == frame and is_equal_approx(player.frame_progress, progress))
    check("pause fires nothing", fired.is_empty())
    if audio_was_playing:
        check("pause pauses audio", player.audio.stream_paused)
    player.resume_playback()
    for i in 3:
        await process_frame
    check("resume continues", player.is_playing() and (player.frame != frame or player.frame_progress > progress))
    if audio_was_playing:
        check("resume resumes audio", not player.audio.stream_paused)
    fired.clear()
    player.restart()
    check("restart returns to frame 0", player.frame == 0 and player.is_playing())
    var names = fired.map(func(m): return m[2])
    var want = clip.frames[0].events.map(func(e): return e.name)
    check("restart fires frame 0 once", names == want, "%s vs %s" % [names, want])

func _direction_switch() -> void:
    var first = package.clips[0]
    var other = package.clips[2]
    player.play_direction(first.name, first.direction.id)
    var into = (first.frames[0].duration_ms + first.frames[1].duration_ms * 0.5) / 1000.0
    for i in int(into / dt):
        await process_frame
    var frame = player.frame
    var progress = player.frame_progress
    fired.clear()
    check("direction switch", player.play_direction(other.name, other.direction.id))
    check("direction switch animation", str(player.animation) == other.animation, str(player.animation))
    check("direction switch keeps frame", player.frame == frame and is_equal_approx(player.frame_progress, progress), "%d %.3f vs %d %.3f" % [player.frame, player.frame_progress, frame, progress])
    check("direction switch fires nothing", fired.is_empty(), str(fired))
    check("unknown direction is refused", not player.play_direction(first.name, "up"))
