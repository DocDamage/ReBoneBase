//! Data-only reader for original Bonebase .bbrig table-literal files.
//! No Lua VM or expression evaluation is used in the shipping application.
use crate::{
    animation::{Channel, Clip, Ease, Keyframe, Track},
    model::{Bone, Bulge, Mirror, Part, Rig},
};
use std::collections::HashSet;

#[derive(Debug)]
enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    Table(Vec<(Key, Value)>),
}
#[derive(Debug, PartialEq, Eq, Hash)]
enum Key {
    Name(String),
    Index(i64),
}
impl Value {
    fn get(&self, name: &str) -> Option<&Self> {
        match self {
            Self::Table(t) => t
                .iter()
                .find_map(|(k, v)| matches!(k,Key::Name(n) if n==name).then_some(v)),
            _ => None,
        }
    }
    fn number(&self) -> Result<f64, String> {
        match self {
            Self::Number(n) => Ok(*n),
            _ => Err("Expected a number".into()),
        }
    }
    fn text(&self) -> Result<&str, String> {
        match self {
            Self::String(s) => Ok(s),
            _ => Err("Expected a string".into()),
        }
    }
    fn boolean(&self) -> Result<bool, String> {
        match self {
            Self::Bool(b) => Ok(*b),
            _ => Err("Expected a boolean".into()),
        }
    }
    fn array(&self) -> Result<Vec<&Self>, String> {
        let Self::Table(entries) = self else {
            return Err("Expected a table".into());
        };
        let mut indexed = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let Key::Index(index) = key else {
                return Err("Expected a contiguous array".into());
            };
            indexed.push((*index, value));
        }
        indexed.sort_by_key(|(i, _)| *i);
        if indexed
            .iter()
            .enumerate()
            .any(|(i, (n, _))| *n != i as i64 + 1)
        {
            return Err("Sparse array is unsupported".into());
        }
        Ok(indexed.into_iter().map(|(_, v)| v).collect())
    }
    fn nonempty(&self) -> bool {
        match self {
            Self::Nil => false,
            Self::Table(t) => !t.is_empty(),
            _ => true,
        }
    }
}
struct Parser<'a> {
    input: &'a [u8],
    at: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.at)
    }
    fn skip(&mut self) -> Result<(), String> {
        loop {
            while self.input.get(self.at).is_some_and(u8::is_ascii_whitespace) {
                self.at += 1;
            }
            if self.input.get(self.at..self.at + 2) == Some(b"--") {
                self.at += 2;
                if self.input.get(self.at..self.at + 2) == Some(b"[[") {
                    self.at += 2;
                    while self.at + 1 < self.input.len()
                        && self.input.get(self.at..self.at + 2) != Some(b"]]")
                    {
                        self.at += 1;
                    }
                    if self.at + 1 >= self.input.len() {
                        return Err(self.error("Unterminated comment"));
                    }
                    self.at += 2;
                } else {
                    while self.input.get(self.at).is_some_and(|c| *c != b'\n') {
                        self.at += 1;
                    }
                }
            } else {
                return Ok(());
            }
        }
    }
    fn take(&mut self, c: u8) -> Result<(), String> {
        self.skip()?;
        if self.input.get(self.at) != Some(&c) {
            return Err(self.error("Unexpected syntax"));
        }
        self.at += 1;
        Ok(())
    }
    fn ident(&mut self) -> Result<String, String> {
        let start = self.at;
        if !self
            .input
            .get(self.at)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            return Err(self.error("Expected identifier"));
        }
        self.at += 1;
        while self
            .input
            .get(self.at)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.at += 1;
        }
        Ok(String::from_utf8(self.input[start..self.at].to_vec()).unwrap())
    }
    fn string(&mut self) -> Result<String, String> {
        let quote = self.input[self.at];
        self.at += 1;
        let mut bytes = Vec::new();
        loop {
            let c = *self
                .input
                .get(self.at)
                .ok_or_else(|| self.error("Unterminated string"))?;
            self.at += 1;
            if c == quote {
                break;
            }
            if c != b'\\' {
                if c == b'\n' || c == b'\r' {
                    return Err(self.error("Unescaped newline in string"));
                }
                bytes.push(c);
                continue;
            }
            let escaped = *self
                .input
                .get(self.at)
                .ok_or_else(|| self.error("Unterminated escape"))?;
            self.at += 1;
            match escaped {
                b'a' => bytes.push(7),
                b'b' => bytes.push(8),
                b'f' => bytes.push(12),
                b'n' => bytes.push(b'\n'),
                b'r' => bytes.push(b'\r'),
                b't' => bytes.push(b'\t'),
                b'v' => bytes.push(11),
                b'\\' | b'\'' | b'"' => bytes.push(escaped),
                b'\n' => bytes.push(b'\n'),
                b'\r' => {
                    if self.input.get(self.at) == Some(&b'\n') {
                        self.at += 1;
                    }
                    bytes.push(b'\n');
                }
                b'z' => {
                    while self.input.get(self.at).is_some_and(u8::is_ascii_whitespace) {
                        self.at += 1;
                    }
                }
                b'x' => {
                    let text = self
                        .input
                        .get(self.at..self.at + 2)
                        .and_then(|s| std::str::from_utf8(s).ok())
                        .ok_or_else(|| self.error("Invalid hexadecimal escape"))?;
                    let n = u8::from_str_radix(text, 16)
                        .map_err(|_| self.error("Invalid hexadecimal escape"))?;
                    bytes.push(n);
                    self.at += 2;
                }
                b'0'..=b'9' => {
                    let mut n = u16::from(escaped - b'0');
                    for _ in 0..2 {
                        if let Some(d) = self.input.get(self.at).filter(|c| c.is_ascii_digit()) {
                            n = n * 10 + u16::from(*d - b'0');
                            self.at += 1;
                        } else {
                            break;
                        }
                    }
                    if n > 255 {
                        return Err(self.error("Invalid decimal escape"));
                    }
                    bytes.push(n as u8);
                }
                _ => return Err(self.error("Unsupported string escape")),
            }
        }
        String::from_utf8(bytes).map_err(|_| self.error("String is not UTF-8"))
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self
            .input
            .get(self.at)
            .is_some_and(|c| *c == b'-' || *c == b'+')
        {
            self.at += 1;
        }
        while self.input.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.input.get(self.at) == Some(&b'.') {
            self.at += 1;
            while self.input.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        if self
            .input
            .get(self.at)
            .is_some_and(|c| *c == b'e' || *c == b'E')
        {
            self.at += 1;
            if self
                .input
                .get(self.at)
                .is_some_and(|c| *c == b'-' || *c == b'+')
            {
                self.at += 1;
            }
            while self.input.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        let text = std::str::from_utf8(&self.input[start..self.at])
            .map_err(|_| self.error("Invalid number"))?;
        let n = text
            .parse::<f64>()
            .map_err(|_| self.error("Invalid number"))?;
        if !n.is_finite() {
            return Err(self.error("Non-finite number"));
        }
        Ok(Value::Number(n))
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        self.skip()?;
        self.nodes += 1;
        if self.nodes > 250_000 {
            return Err(self.error("Too many values"));
        }
        match self.input.get(self.at).copied() {
            Some(b'{') => {
                if depth >= 64 {
                    return Err(self.error("Table nesting exceeds 64"));
                }
                self.at += 1;
                let mut entries = Vec::new();
                let mut keys = HashSet::new();
                let mut index = 1;
                loop {
                    self.skip()?;
                    if self.input.get(self.at) == Some(&b'}') {
                        self.at += 1;
                        break;
                    }
                    let start = self.at;
                    let named = if self
                        .input
                        .get(self.at)
                        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                    {
                        let name = self.ident()?;
                        self.skip()?;
                        if self.input.get(self.at) == Some(&b'=') {
                            self.at += 1;
                            Some(Key::Name(name))
                        } else {
                            self.at = start;
                            None
                        }
                    } else if self.input.get(self.at) == Some(&b'[') {
                        self.at += 1;
                        let value = self.value(depth + 1)?;
                        self.take(b']')?;
                        self.take(b'=')?;
                        Some(match value {
                            Value::String(s) => Key::Name(s),
                            Value::Number(n)
                                if n.fract() == 0. && n.abs() < 9_007_199_254_740_992. =>
                            {
                                Key::Index(n as i64)
                            }
                            _ => return Err(self.error("Invalid table key")),
                        })
                    } else {
                        None
                    };
                    let key = named.unwrap_or_else(|| {
                        let k = Key::Index(index);
                        index += 1;
                        k
                    });
                    if !keys.insert(match &key {
                        Key::Name(n) => format!("s{n}"),
                        Key::Index(i) => format!("i{i}"),
                    }) {
                        return Err(self.error("Duplicate table key"));
                    }
                    entries.push((key, self.value(depth + 1)?));
                    self.skip()?;
                    match self.input.get(self.at) {
                        Some(b',') | Some(b';') => self.at += 1,
                        Some(b'}') => {}
                        _ => return Err(self.error("Expected table separator")),
                    }
                }
                Ok(Value::Table(entries))
            }
            Some(b'\'' | b'"') => Ok(Value::String(self.string()?)),
            Some(b'-' | b'+' | b'.' | b'0'..=b'9') => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => match self.ident()?.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "nil" => Ok(Value::Nil),
                _ => Err(self.error("Executable expressions are not allowed")),
            },
            _ => Err(self.error("Expected a literal value")),
        }
    }
}
fn required<'a>(v: &'a Value, k: &str) -> Result<&'a Value, String> {
    v.get(k).ok_or_else(|| format!("Missing {k}"))
}
fn number(v: &Value, k: &str, default: f64) -> Result<f64, String> {
    v.get(k).map_or(Ok(default), Value::number)
}
fn integer(n: f64) -> Result<u32, String> {
    if n.fract() != 0. || n < 0. || n > f64::from(u32::MAX) {
        Err("Invalid nonnegative integer".into())
    } else {
        Ok(n as u32)
    }
}
fn text(v: &Value, k: &str, default: &str) -> Result<String, String> {
    v.get(k)
        .map_or(Ok(default.into()), |v| v.text().map(str::to_owned))
}
fn boolean(v: &Value, k: &str, default: bool) -> Result<bool, String> {
    v.get(k).map_or(Ok(default), Value::boolean)
}
fn list<'a>(v: &'a Value, k: &str) -> Result<Vec<&'a Value>, String> {
    v.get(k).map_or(Ok(vec![]), Value::array)
}
fn bulges(v: &Value, k: &str) -> Result<Vec<Bulge>, String> {
    list(v, k)?
        .into_iter()
        .map(|v| {
            let pair = v.array()?;
            if pair.len() != 2 {
                return Err("Invalid bulge pair".into());
            }
            Ok(Bulge {
                t: pair[0].number()? as f32,
                d: pair[1].number()? as f32,
            })
        })
        .collect()
}
fn easing(name: &str) -> Result<Ease, String> {
    match name {
        "Linear" => Ok(Ease::Linear),
        "Ease" => Ok(Ease::Ease),
        "Ease in" => Ok(Ease::EaseIn),
        "Ease out" => Ok(Ease::EaseOut),
        "Back" => Ok(Ease::Back),
        "Bounce" => Ok(Ease::Bounce),
        "Step" => Ok(Ease::Step),
        _ => Err("Unknown keyframe easing".into()),
    }
}

pub fn import(input: &str) -> Result<(Rig, Vec<String>), String> {
    if input.len() > 16 * 1024 * 1024 {
        return Err("Legacy rig exceeds 16 MiB".into());
    }
    let mut parser = Parser {
        input: input.as_bytes(),
        at: 0,
        nodes: 0,
    };
    parser.skip()?;
    if parser
        .input
        .get(parser.at)
        .is_some_and(u8::is_ascii_alphabetic)
        && parser.ident()? != "return"
    {
        return Err("Expected a literal rig or return statement".into());
    }
    let value = parser.value(0)?;
    parser.skip()?;
    if parser.at != input.len() {
        return Err(parser.error("Trailing executable syntax is not allowed"));
    }
    if number(&value, "version", 1.)? > 1. {
        return Err("Unsupported Bonebase version".into());
    }
    let canvas = required(&value, "canvas")?;
    let mut rig = Rig::new(
        text(&value, "libName", "Imported Bonebase")?,
        integer(number(canvas, "w", 64.)?)?,
        integer(number(canvas, "h", 64.)?)?,
    );
    let bones = required(&value, "bones")?.array()?;
    let parts = required(&value, "parts")?.array()?;
    if bones.len() > 2048 || parts.len() > 256 {
        return Err("Too many bones or parts".into());
    }
    for p in parts {
        let colors = list(p, "color")?;
        let mut color = [200, 200, 200, 255];
        for (i, v) in colors.into_iter().take(4).enumerate() {
            let n = integer(v.number()?)?;
            color[i] = u8::try_from(n).map_err(|_| "Color outside byte range")?;
        }
        let z = number(p, "z", 0.)?;
        if z.fract() != 0. || z.abs() > i32::MAX as f64 {
            return Err("Invalid part stacking order".into());
        }
        rig.parts.push(Part {
            id: integer(required(p, "id")?.number()?)?,
            name: text(p, "name", "Group")?,
            color,
            z: z as i32,
            blend: number(p, "blend", 2.)? as f32,
            ..Default::default()
        });
    }
    for b in bones {
        let parent = integer(number(b, "parent", 0.)?)?;
        let twin = integer(number(b, "twin", 0.)?)?;
        rig.bones.push(Bone {
            id: integer(required(b, "id")?.number()?)?,
            name: text(b, "name", "Bone")?,
            parent: (parent != 0).then_some(parent),
            connected: boolean(b, "connected", false)?,
            x: number(b, "x", 0.)? as f32,
            y: number(b, "y", 0.)? as f32,
            length: required(b, "len")?.number()? as f32,
            angle: required(b, "angle")?.number()? as f32,
            radius_head: number(b, "rHead", 4.)? as f32,
            radius_tail: number(b, "rTail", 4.)? as f32,
            part: integer(number(b, "part", 0.)?)?,
            twin: (twin != 0).then_some(twin),
            bulges_left: bulges(b, "bulgeL")?,
            bulges_right: bulges(b, "bulgeR")?,
            ..Default::default()
        });
    }
    if let Some(m) = value.get("mirror") {
        rig.mirror = Mirror {
            on: boolean(m, "on", false)?,
            x: number(m, "x", f64::from(rig.width) / 2.)? as f32,
        };
    }
    let mut warnings = Vec::new();
    for (field, message) in [
        (
            "ik",
            "Live IK controllers are not imported; FK keys remain available.",
        ),
        ("images", "Embedded reference images are not imported."),
        ("poses", "Saved pose-library entries are not imported."),
        (
            "rootPos",
            "Custom rig-root handle position is not imported.",
        ),
    ] {
        if value.get(field).is_some_and(Value::nonempty) {
            warnings.push(message.into());
        }
    }
    if value
        .get("ground")
        .and_then(|g| g.get("on"))
        .is_some_and(|v| matches!(v, Value::Bool(true)))
    {
        warnings.push("Ground constraints are not imported.".into());
    }
    if let Some(bounds) = value.get("bounds")
        && (number(bounds, "x", 0.)? != 0.
            || number(bounds, "y", 0.)? != 0.
            || number(bounds, "w", f64::from(rig.width))? != f64::from(rig.width)
            || number(bounds, "h", f64::from(rig.height))? != f64::from(rig.height))
    {
        warnings.push("Export subregion is not imported; exports use the whole canvas.".into());
    }
    let mut clips = list(&value, "clips")?;
    if clips.is_empty()
        && let Some(anim) = value.get("anim")
    {
        clips.push(anim);
    }
    if clips.len() > 32 {
        return Err("Too many clips".into());
    }
    if !clips.is_empty() {
        rig.clips.clear();
    }
    for c in clips {
        let ms = number(c, "frameMs", 100.)?;
        if !(10. ..=60000.).contains(&ms) {
            return Err("Invalid frame duration".into());
        }
        let mut clip = Clip {
            name: text(c, "name", "Animation")?,
            fps: (1000. / ms) as f32,
            length: integer(number(c, "frames", 12.)?)?,
            looped: boolean(c, "loop", true)?,
            tracks: vec![],
            ..Default::default()
        };
        if boolean(c, "pingpong", false)? {
            clip.playback = crate::animation::Playback::PingPong;
        }
        if c.get("ref").is_some_and(Value::nonempty) {
            warnings.push("Clip reference-image settings are not imported.".into());
        }
        for pair in list(c, "durations")? {
            let values = pair.array()?;
            if values.len() != 2 {
                return Err("Frame durations require frame/ms pairs".into());
            }
            let frame = integer(values[0].number()?)?;
            let duration = integer(values[1].number()?)?;
            if frame == 0 || frame > clip.length || !(1..=60000).contains(&duration) {
                return Err("Invalid per-frame duration".into());
            }
            clip.durations.insert(frame, duration);
        }
        let tracks = list(c, "tracks")?;
        if tracks.len() > 16384 {
            return Err("Too many tracks".into());
        }
        for t in tracks {
            let owner = required(t, "bone")?.number()?;
            let ch = required(t, "ch")?.text()?;
            let channel = match ch {
                "rot" => Some(Channel::Rotation),
                "x" => Some(Channel::X),
                "y" => Some(Channel::Y),
                "s" => Some(Channel::Scale),
                "z" => Some(Channel::Z),
                "vis" => Some(Channel::Visibility),
                _ => None,
            };
            let is_bone = owner > 0. && rig.bones.iter().any(|b| f64::from(b.id) == owner);
            let is_part = owner > 0. && rig.parts.iter().any(|p| f64::from(p.id) == owner);
            if channel.is_none()
                || !(is_bone || is_part)
                || (is_part && !matches!(channel, Some(Channel::Z | Channel::Visibility)))
                || (is_bone && matches!(channel, Some(Channel::Z | Channel::Visibility)))
            {
                warnings.push("Unsupported IK, reference, global-root or unknown animation tracks were omitted.".into());
                continue;
            }
            let mut track = Track {
                bone: integer(owner)?,
                channel: channel.unwrap(),
                keys: vec![],
            };
            let keys = list(t, "keys")?;
            if keys.len() > 240 {
                return Err("Too many keys in track".into());
            }
            for k in keys {
                let fields = k.array()?;
                if !(2..=3).contains(&fields.len()) {
                    return Err("Malformed keyframe tuple".into());
                }
                track.keys.push(Keyframe {
                    frame: fields[0].number()? as f32,
                    value: fields[1].number()? as f32,
                    easing: easing(
                        fields
                            .get(2)
                            .map_or_else(
                                || text(c, "ease", "Ease"),
                                |v| v.text().map(str::to_owned),
                            )?
                            .as_str(),
                    )?,
                    tangent_in: 0.,
                    tangent_out: 0.,
                });
            }
            track.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
            clip.tracks.push(track);
        }
        rig.clips.push(clip);
    }
    let active = integer(number(&value, "clipIndex", 1.)?)?;
    rig.active_clip = active.saturating_sub(1) as usize;
    rig.validate()?;
    warnings.sort();
    warnings.dedup();
    Ok((rig, warnings))
}
