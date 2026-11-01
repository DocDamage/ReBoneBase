using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEngine;

/// Editor check of ReBonePlayer/ReBoneImporter against exported packages:
///   Unity -batchmode -projectPath <project> -executeMethod ReBoneCheck.Run
///         -rebonePackages Assets/Fixture/main/package.json,Assets/Fixture/directions/package.json
///         -reboneReport <file>
/// Prints "CHECK <name> FAIL <detail>" lines and a "SUMMARY {json}" line.
public static class ReBoneCheck
{
    static int checks, failures;
    static readonly StringBuilder report = new StringBuilder();

    static void Check(string name, bool ok, string detail = "")
    {
        checks++;
        if (ok) return;
        failures++;
        var line = $"CHECK {name} FAIL {detail}";
        report.AppendLine(line);
        Debug.Log(line);
    }

    static string Arg(string name)
    {
        var args = Environment.GetCommandLineArgs();
        var i = Array.IndexOf(args, name);
        return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
    }

    public static void Run()
    {
        var clips = 0;
        try
        {
            foreach (var path in (Arg("-rebonePackages") ?? "").Split(new[] { ',' }, StringSplitOptions.RemoveEmptyEntries))
                clips += Package(path);
        }
        catch (Exception e)
        {
            Check("exception", false, e.ToString());
        }
        var summary = $"SUMMARY {{\"engine\":\"Unity {Application.unityVersion}\",\"checks\":{checks},\"failures\":{failures},\"clips\":{clips}}}";
        report.AppendLine(summary);
        Debug.Log(summary);
        var file = Arg("-reboneReport");
        if (file != null) File.WriteAllText(file, report.ToString());
        EditorApplication.Exit(failures == 0 ? 0 : 1);
    }

    static int Package(string path)
    {
        var player = ReBoneImporter.Import(path);
        Check(path + " imports", player != null);
        if (player == null) return 0;
        Check(path + " reimport updates the same actor", ReBoneImporter.Import(path) == player);
        Check(path + " initializes", player.Initialize());
        var data = player.Data;
        Placement(player, data);
        for (int c = 0; c < data.clips.Length; c++)
        {
            Timeline(player, data, c, 1f / 240f);
            Timeline(player, data, c, 0.1f);
        }
        if (data.clips.Any(c => c.direction != null && !string.IsNullOrEmpty(c.direction.id)))
            DirectionSwitch(player, data);
        else
            PauseAndRestart(player, data);
        return data.clips.Length;
    }

    static void Placement(ReBonePlayer player, ReBonePlayer.Manifest data)
    {
        for (int p = 0; p < data.pages.Length; p++)
        {
            var sheet = player.sheets[p];
            Check($"page {p} keeps its size", sheet.width == data.pages[p].w && sheet.height == data.pages[p].h, $"{sheet.width}x{sheet.height}");
            Check($"page {p} point filtering", sheet.filterMode == FilterMode.Point);
        }
        for (int c = 0; c < data.clips.Length; c++)
        {
            var clip = data.clips[c];
            for (int i = 0; i < clip.frames.Length; i++)
            {
                var f = clip.frames[i];
                var r = f.rect;
                var sprite = player.SpriteAt(c, i);
                var name = $"{clip.animation} {i}";
                Check(name + " sprite", sprite != null);
                if (sprite == null) continue;
                Check(name + " rect", sprite.rect == new Rect(r.x, data.pages[f.page].h - r.y - r.h, r.w, r.h), sprite.rect.ToString());
                // Pivot in pixels from the rect's bottom-left: untrimmed pivot minus trim, flipped.
                var pivot = new Vector2(data.pivot.x - f.trim.x, r.h - (data.pivot.y - f.trim.y));
                Check(name + " pivot", (sprite.pivot - pivot).magnitude < 1e-3f, $"{sprite.pivot} vs {pivot}");
                Check(name + " texture", sprite.texture == player.sheets[f.page]);
            }
        }
    }

    static float Total(ReBonePlayer.Clip clip) => clip.frames.Sum(f => f.duration_ms) / 1000f;

    static (int frame, float start, float margin) Expected(ReBonePlayer.Clip clip, double t)
    {
        var total = clip.frames.Sum(f => f.duration_ms) / 1000.0;
        var time = clip.looped ? t % total : Math.Min(t, total - 1e-9);
        var loopStart = t - time;
        double start = 0;
        for (int i = 0; i < clip.frames.Length; i++)
        {
            var end = start + clip.frames[i].duration_ms / 1000.0;
            if (time < end) return (i, (float)(loopStart + start), (float)Math.Min(time - start, end - time));
            start = end;
        }
        return (clip.frames.Length - 1, (float)(loopStart + start - clip.frames[clip.frames.Length - 1].duration_ms / 1000.0), 1);
    }

    static List<(int, string)> ExpectedMarkers(ReBonePlayer.Clip clip, double t)
    {
        var output = new List<(int, string)>();
        double start = 0;
        for (int i = 0; ; )
        {
            foreach (var e in clip.frames[i].events) output.Add((i, e.name));
            var end = start + clip.frames[i].duration_ms / 1000.0;
            if (end > t + 1e-6) return output;
            start = end;
            if (++i == clip.frames.Length)
            {
                if (!clip.looped) return output;
                i = 0;
            }
        }
    }

    static List<(string animation, int frame, string name)> Listen(ReBonePlayer player)
    {
        var fired = new List<(string, int, string)>();
        player.marker = new ReBonePlayer.MarkerEvent();
        player.marker.AddListener((n, p) => fired.Add((player.CurrentClip.animation, player.FrameIndex, n)));
        return fired;
    }

    static void Timeline(ReBonePlayer player, ReBonePlayer.Manifest data, int c, float dt)
    {
        var clip = data.clips[c];
        var name = $"{clip.animation} @{Mathf.RoundToInt(1 / dt)}fps";
        var fired = Listen(player);
        Check(name + " plays", player.Play(clip.animation));
        var duration = clip.looped ? Total(clip) * 2.5 : Total(clip) + 0.5;
        List<BoxCollider2D> pool = null;
        double t = 0;
        for (int step = 0; t < duration; step++)
        {
            var want = Expected(clip, t);
            if (want.margin > 1e-4f)
            {
                Check($"{name} frame at {t:F3}", player.FrameIndex == want.frame, $"got {player.FrameIndex} want {want.frame}");
                var f = clip.frames[player.FrameIndex];
                var boxes = f.boxes ?? Array.Empty<ReBonePlayer.Region>();
                Check($"{name} regions at {t:F3}", player.ActiveRegions == boxes.Length && player.RegionColliders.Count(r => r.enabled) == boxes.Length);
                for (int r = 0; r < boxes.Length; r++)
                    Corners($"{name} region {r}", player, data, player.RegionColliders[r], boxes[r]);
                if (data.audio != null && data.audio.enabled)
                {
                    var at = (f.source_time_ms + (t - want.start) * 1000 - data.audio.start_ms) / 1000;
                    Check($"{name} audio target at {t:F3}", Math.Abs(player.AudioTarget - at) < 1e-3, $"{player.AudioTarget:F4} vs {at:F4}");
                }
            }
            if (pool == null && player.RegionColliders.Count > 0) pool = player.RegionColliders.ToList();
            player.Step(dt);
            t += dt;
        }
        if (pool != null)
            Check(name + " reuses region colliders", pool.Select((r, i) => player.RegionColliders[i] == r).All(x => x));
        if (Expected(clip, t).margin > 1e-4f)
        {
            var got = fired.Where(m => m.animation == clip.animation).Select(m => (m.frame, m.name)).ToList();
            var want = ExpectedMarkers(clip, t);
            Check(name + " markers in order", got.SequenceEqual(want), $"{string.Join(",", got)} vs {string.Join(",", want)}");
        }
        if (!clip.looped)
            Check(name + " stops on last frame", !player.IsPlaying && player.FrameIndex == clip.frames.Length - 1);
    }

    static void Corners(string name, ReBonePlayer player, ReBonePlayer.Manifest data, BoxCollider2D collider, ReBonePlayer.Region region)
    {
        var t = collider.transform;
        var half = collider.size / 2;
        var actual = new[] { new Vector2(-1, -1), new Vector2(1, -1), new Vector2(1, 1), new Vector2(-1, 1) }
            .Select(s => (Vector2)(t.localPosition + t.localRotation * new Vector3(s.x * half.x, s.y * half.y, 0))).ToList();
        foreach (var corner in region.corners)
        {
            var want = new Vector2((corner.x - data.pivot.x) / player.pixelsPerUnit, (data.pivot.y - corner.y) / player.pixelsPerUnit);
            Check(name + " corner", actual.Any(a => (a - want).magnitude < 1e-3f), $"{want} not in {string.Join(",", actual)}");
        }
    }

    static void PauseAndRestart(ReBonePlayer player, ReBonePlayer.Manifest data)
    {
        var clip = data.clips[0];
        var fired = Listen(player);
        player.Play(clip.animation);
        for (int i = 0; i < 144; i++) player.Step(1f / 240f);
        var frame = player.FrameIndex;
        var elapsed = player.FrameElapsedMs;
        fired.Clear();
        player.Pause();
        for (int i = 0; i < 120; i++) player.Step(1f / 240f);
        Check("pause holds frame", player.FrameIndex == frame && Mathf.Approximately(player.FrameElapsedMs, elapsed));
        Check("pause fires nothing", fired.Count == 0);
        player.Resume();
        player.Step(1f / 240f);
        Check("resume continues", player.IsPlaying && (player.FrameIndex != frame || player.FrameElapsedMs > elapsed));
        fired.Clear();
        player.Restart();
        Check("restart returns to frame 0", player.FrameIndex == 0 && player.IsPlaying);
        var names = fired.Select(m => m.name).ToList();
        var want = clip.frames[0].events.Select(e => e.name).ToList();
        Check("restart fires frame 0 once", names.SequenceEqual(want), string.Join(",", names));
        Check("audio source assigned", data.audio == null || !data.audio.enabled || (player.Audio != null && player.Audio.clip == player.audioClip && Mathf.Approximately(player.Audio.volume, data.audio.volume)));
    }

    static void DirectionSwitch(ReBonePlayer player, ReBonePlayer.Manifest data)
    {
        var first = data.clips[0];
        var other = data.clips[2];
        var fired = Listen(player);
        player.Play(first.name, first.direction.id);
        var into = (first.frames[0].duration_ms + first.frames[1].duration_ms * 0.5f) / 1000f;
        for (float t = 0; t < into; t += 1f / 240f) player.Step(1f / 240f);
        var frame = player.FrameIndex;
        var elapsed = player.FrameElapsedMs;
        fired.Clear();
        Check("direction switch", player.Play(other.name, other.direction.id));
        Check("direction switch animation", player.CurrentClip.animation == other.animation, player.CurrentClip.animation);
        Check("direction switch keeps frame", player.FrameIndex == frame && Mathf.Approximately(player.FrameElapsedMs, elapsed));
        Check("direction switch fires nothing", fired.Count == 0);
        Check("unknown direction is refused", !player.Play(first.name, "up"));
    }
}
