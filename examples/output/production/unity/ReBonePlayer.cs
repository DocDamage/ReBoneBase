using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Events;

/// Plays a ReBoneBase sprite package: exact holds, markers fired in frame order
/// (including frames passed within one update), one reused trigger collider per
/// collision region, looping, pause/resume/restart, directions and reference
/// audio kept in sync with each frame's retained source time.
[RequireComponent(typeof(SpriteRenderer))]
public sealed class ReBonePlayer : MonoBehaviour
{
    [Serializable] public class Point { public float x, y; }
    [Serializable] public class Size { public int w, h; }
    [Serializable] public class Area { public int x, y, w, h; }
    [Serializable] public class Marker { public string name, payload; }
    [Serializable] public class Region { public long id; public string name, kind; public Point center, size; public float angle; public Point[] corners; }
    [Serializable] public class Page { public string file; public int w, h; }
    [Serializable] public class AudioInfo { public string file; public long start_ms; public float volume = 1; public bool enabled; }
    // x, y, w, h, Clip.file, width/height, audio_* and a normalized pivot are version 1 fields that Load upgrades.
    [Serializable] public class Frame { public int page, x, y, w, h, source_frame, duration_ms; public long source_time_ms; public Area rect; public Size source_size; public Point trim; public Marker[] events; public Region[] boxes; }
    [Serializable] public class Direction { public string id; public float angle; }
    [Serializable] public class Clip { public string name, file, id, animation; public bool looped; public float fps; public Direction direction; public Frame[] frames; }
    [Serializable] public class Manifest { public string format; public int format_version, version, width, height; public Size frame_size; public Point pivot; public Page[] pages; public Clip[] clips; public AudioInfo audio; public string audio_file; public long audio_start_ms; public float audio_volume = 1; public bool audio_enabled; }
    [Serializable] public class MarkerEvent : UnityEvent<string, string> { }

    public TextAsset manifest;
    public Texture2D[] sheets;
    public float pixelsPerUnit = 16;
    /// Optional reference audio; the importer assigns the package's WAV.
    public AudioClip audioClip;
    public bool playAudio = true;
    /// Invoked for every marker of every frame entered, in frame order.
    public MarkerEvent marker = new MarkerEvent();

    Manifest data;
    SpriteRenderer output;
    AudioSource audioSource;
    Sprite[][] sprites;
    int clipIndex, frameIndex;
    float elapsedMs;
    bool playing = true;
    readonly List<BoxCollider2D> regions = new List<BoxCollider2D>();
    int activeRegions;

    public Manifest Data => data;
    public Clip CurrentClip => data?.clips[clipIndex];
    public int FrameIndex => frameIndex;
    public bool IsPlaying => playing;
    /// Milliseconds spent in the current frame.
    public float FrameElapsedMs => elapsedMs;
    public IReadOnlyList<BoxCollider2D> RegionColliders => regions;
    public int ActiveRegions => activeRegions;
    public AudioSource Audio => audioSource;
    public Sprite SpriteAt(int clip, int frame) => sprites[clip][frame];

    /// Parse a manifest, upgrading version 1 to the version 2 shape; null when unsupported.
    public static Manifest Load(string json)
    {
        var m = JsonUtility.FromJson<Manifest>(json);
        if (m == null || m.clips == null) return null;
        if (m.format == "rebonebase-sprites") return m.format_version <= 2 ? m : null;
        if (m.version != 1) return null;
        m.frame_size = new Size { w = m.width, h = m.height };
        m.pivot = new Point { x = m.pivot.x * m.width, y = m.pivot.y * m.height };
        m.pages = new Page[m.clips.Length];
        for (int c = 0; c < m.clips.Length; c++)
        {
            m.pages[c] = new Page { file = m.clips[c].file };
            foreach (var f in m.clips[c].frames)
            {
                f.page = c;
                f.rect = new Area { x = f.x, y = f.y, w = f.w, h = f.h };
                f.source_size = m.frame_size;
                f.trim = new Point();
            }
        }
        m.audio = string.IsNullOrEmpty(m.audio_file) ? null : new AudioInfo { file = m.audio_file, start_ms = m.audio_start_ms, volume = m.audio_volume, enabled = m.audio_enabled };
        return m;
    }

    void Start() => Initialize();

    /// Build sprites and start the first clip. Safe to call more than once.
    public bool Initialize()
    {
        if (data != null) return true;
        data = manifest ? Load(manifest.text) : null;
        if (data == null) { enabled = false; return false; }
        output = GetComponent<SpriteRenderer>();
        sprites = new Sprite[data.clips.Length][];
        for (int c = 0; c < data.clips.Length; c++)
        {
            sprites[c] = new Sprite[data.clips[c].frames.Length];
            for (int i = 0; i < sprites[c].Length; i++)
            {
                var f = data.clips[c].frames[i];
                var r = f.rect;
                var texture = sheets[f.page];
                texture.filterMode = FilterMode.Point;
                if (r.w <= 0 || r.h <= 0) continue;
                // The pivot is in untrimmed frame pixels; express it relative to the trimmed rect, bottom-up.
                var pivot = new Vector2((data.pivot.x - f.trim.x) / r.w, (r.h - (data.pivot.y - f.trim.y)) / r.h);
                sprites[c][i] = Sprite.Create(texture, new Rect(r.x, texture.height - r.y - r.h, r.w, r.h), pivot, pixelsPerUnit, 0, SpriteMeshType.FullRect);
            }
        }
        if (playAudio && audioClip && data.audio != null && data.audio.enabled)
        {
            audioSource = gameObject.AddComponent<AudioSource>();
            audioSource.clip = audioClip;
            audioSource.playOnAwake = false;
            audioSource.volume = data.audio.volume;
        }
        Enter(true);
        return true;
    }

    /// Start an animation (by animation name or clip id) from its first frame.
    public bool Play(string name)
    {
        if (data == null) return false;
        for (int i = 0; i < data.clips.Length; i++)
        {
            if (data.clips[i].animation == name || data.clips[i].id == name)
            {
                Begin(i);
                return true;
            }
        }
        return false;
    }

    /// Play the authored clip `name` facing `direction` (e, se, s, ...). Changing only the
    /// direction keeps the current frame and timing and fires nothing.
    public bool Play(string name, string direction)
    {
        if (data == null) return false;
        var current = data.clips[clipIndex];
        for (int i = 0; i < data.clips.Length; i++)
        {
            var c = data.clips[i];
            if (c.name != name || c.direction == null || c.direction.id != direction) continue;
            if (current.name == name && current.frames.Length == c.frames.Length)
            {
                clipIndex = i;
                Enter(false);
            }
            else
            {
                Begin(i);
            }
            return true;
        }
        return false;
    }

    public void Pause()
    {
        playing = false;
        if (audioSource) audioSource.Pause();
    }

    public void Resume()
    {
        if (data == null) return;
        playing = true;
        SyncAudio(true);
    }

    /// Return to the first frame, firing its markers, and play.
    public void Restart() => Begin(clipIndex);

    void Begin(int clip)
    {
        clipIndex = clip;
        frameIndex = 0;
        elapsedMs = 0;
        playing = true;
        Enter(true);
    }

    void Update()
    {
        if (data != null) Step(Time.deltaTime);
    }

    /// Advance playback by `seconds`, entering every frame passed, in order.
    public void Step(float seconds)
    {
        if (!playing || data == null || seconds <= 0) return;
        elapsedMs += seconds * 1000;
        var clip = data.clips[clipIndex];
        for (int guard = 0; guard < 100000 && elapsedMs >= clip.frames[frameIndex].duration_ms; guard++)
        {
            elapsedMs -= clip.frames[frameIndex].duration_ms;
            if (frameIndex + 1 < clip.frames.Length) frameIndex++;
            else if (clip.looped) frameIndex = 0;
            else
            {
                elapsedMs = clip.frames[frameIndex].duration_ms;
                playing = false;
                if (audioSource) audioSource.Stop();
                break;
            }
            Enter(true);
        }
        UpdateAudio();
    }

    // Start audio that becomes due inside a frame; stop it once the track ends.
    void UpdateAudio()
    {
        if (!audioSource || !playing) return;
        var at = AudioTarget;
        var due = at >= 0 && at < audioClip.length;
        if (due && !audioSource.isPlaying)
        {
            audioSource.time = at;
            audioSource.Play();
        }
        else if (!due && audioSource.isPlaying)
        {
            audioSource.Stop();
        }
    }

    void Enter(bool fireMarkers)
    {
        var f = data.clips[clipIndex].frames[frameIndex];
        output.sprite = sprites[clipIndex][frameIndex];
        if (fireMarkers)
            foreach (var e in f.events) marker.Invoke(e.name, e.payload);
        ApplyRegions(f.boxes);
        SyncAudio(false);
    }

    // One collider per region, created once and moved; unused colliders are disabled.
    void ApplyRegions(Region[] boxes)
    {
        boxes = boxes ?? Array.Empty<Region>();
        while (regions.Count < boxes.Length)
        {
            var obj = new GameObject("Region");
            obj.transform.SetParent(transform, false);
            var collider = obj.AddComponent<BoxCollider2D>();
            collider.isTrigger = true;
            regions.Add(collider);
        }
        for (int i = 0; i < regions.Count; i++)
        {
            var collider = regions[i];
            if (i >= boxes.Length)
            {
                collider.enabled = false;
                continue;
            }
            var b = boxes[i];
            var t = collider.transform;
            t.localPosition = new Vector3((b.center.x - data.pivot.x) / pixelsPerUnit, (data.pivot.y - b.center.y) / pixelsPerUnit, 0);
            t.localRotation = Quaternion.Euler(0, 0, -b.angle);
            collider.size = new Vector2(b.size.x, b.size.y) / pixelsPerUnit;
            collider.enabled = true;
            if (collider.gameObject.name != b.name) collider.gameObject.name = b.name;
        }
        activeRegions = boxes.Length;
    }

    /// Where the audio should be now: the frame's retained source time plus time spent
    /// in the frame, minus the track's start. Negative before the track starts.
    public float AudioTarget
    {
        get
        {
            if (data?.audio == null) return -1;
            var f = data.clips[clipIndex].frames[frameIndex];
            return (f.source_time_ms + elapsedMs - data.audio.start_ms) / 1000f;
        }
    }

    void SyncAudio(bool resumed)
    {
        if (!audioSource) return;
        var at = AudioTarget;
        if (at < 0 || at >= audioClip.length)
        {
            audioSource.Stop();
            return;
        }
        if (!playing) return;
        if (resumed) audioSource.UnPause();
        if (!audioSource.isPlaying || Mathf.Abs(audioSource.time - at) > 0.08f)
        {
            audioSource.time = at;
            if (!audioSource.isPlaying) audioSource.Play();
        }
    }

    void OnDestroy()
    {
        if (sprites == null) return;
        foreach (var clip in sprites)
            foreach (var sprite in clip)
                if (sprite) Destroy(sprite);
    }
}
