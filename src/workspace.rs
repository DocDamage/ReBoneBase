use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dock {
    #[default]
    Left,
    Right,
    Bottom,
    Floating,
    Hidden,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelLayout {
    pub dock: Dock,
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
}
impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            dock: Dock::Left,
            width: 230.,
            height: 240.,
            x: 50.,
            y: 90.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub name: String,
    pub skeleton: PanelLayout,
    pub inspector: PanelLayout,
    pub timeline: PanelLayout,
    pub production: PanelLayout,
    #[serde(default)]
    pub production_open: bool,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            name: "Animation".into(),
            skeleton: Default::default(),
            inspector: PanelLayout {
                dock: Dock::Right,
                width: 270.,
                ..Default::default()
            },
            timeline: PanelLayout {
                dock: Dock::Bottom,
                height: 280.,
                ..Default::default()
            },
            production: PanelLayout {
                dock: Dock::Floating,
                width: 940.,
                height: 620.,
                x: 240.,
                y: 70.,
            },
            production_open: false,
        }
    }
}
impl Workspace {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.name.len() <= 256, "Workspace name is too long");
        for p in [
            &self.skeleton,
            &self.inspector,
            &self.timeline,
            &self.production,
        ] {
            ensure!(
                [p.width, p.height, p.x, p.y].iter().all(|v| v.is_finite())
                    && (160. ..=2400.).contains(&p.width)
                    && (120. ..=1800.).contains(&p.height)
                    && p.x.abs() <= 10000.
                    && p.y.abs() <= 10000.,
                "Invalid panel layout"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePreset {
    Rigging,
    Animation,
    Scene,
}
impl WorkspacePreset {
    pub const ALL: [Self; 3] = [Self::Rigging, Self::Animation, Self::Scene];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rigging => "Rigging",
            Self::Animation => "Animation",
            Self::Scene => "Scene",
        }
    }

    pub fn layout(self) -> Workspace {
        let mut layout = Workspace {
            name: self.label().into(),
            ..Workspace::default()
        };
        match self {
            Self::Rigging => {
                layout.skeleton.width = 250.;
                layout.inspector.width = 310.;
                layout.timeline.dock = Dock::Hidden;
            }
            Self::Animation => {
                layout.inspector.width = 290.;
                layout.timeline.height = 300.;
            }
            Self::Scene => {
                layout.skeleton.dock = Dock::Hidden;
                layout.inspector.dock = Dock::Hidden;
                layout.timeline.height = 230.;
                layout.production.dock = Dock::Left;
                layout.production.width = 380.;
                layout.production_open = true;
            }
        }
        layout
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Play,
    Delete,
    Fit,
    Bone,
    Select,
    MoveRig,
    IK,
    Radius,
    NextFrame,
    PreviousFrame,
    FirstFrame,
    LastFrame,
    NextKey,
    PreviousKey,
    KeyPose,
    Cancel,
    CopyKeys,
    PasteKeys,
    CyclePick,
    Studio,
    Production,
    CommandPalette,
    Paint,
    Boxes,
    ShowSkeleton,
    ShowInspector,
    ShowTimeline,
    RecoverPanels,
    ResetLayout,
    RiggingWorkspace,
    AnimationWorkspace,
    SceneWorkspace,
    RigTools,
    AnimationTools,
    AppearanceTools,
    AssetLibrary,
    ReferenceTools,
    RecoveryTools,
    BatchExport,
    MotionTools,
    CharacterTools,
    PropTools,
    MeshTools,
    OutfitTools,
    FaceTools,
    SceneTools,
    AudioTools,
    PixelBoxTools,
    GameExport,
    WorkspaceSettings,
}
impl Action {
    pub const ALL: [Self; 56] = [
        Self::New,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::Undo,
        Self::Redo,
        Self::Play,
        Self::Delete,
        Self::Fit,
        Self::Bone,
        Self::Select,
        Self::MoveRig,
        Self::IK,
        Self::Radius,
        Self::NextFrame,
        Self::PreviousFrame,
        Self::FirstFrame,
        Self::LastFrame,
        Self::NextKey,
        Self::PreviousKey,
        Self::KeyPose,
        Self::Cancel,
        Self::CopyKeys,
        Self::PasteKeys,
        Self::CyclePick,
        Self::Studio,
        Self::Production,
        Self::CommandPalette,
        Self::Paint,
        Self::Boxes,
        Self::ShowSkeleton,
        Self::ShowInspector,
        Self::ShowTimeline,
        Self::RecoverPanels,
        Self::ResetLayout,
        Self::RiggingWorkspace,
        Self::AnimationWorkspace,
        Self::SceneWorkspace,
        Self::RigTools,
        Self::AnimationTools,
        Self::AppearanceTools,
        Self::AssetLibrary,
        Self::ReferenceTools,
        Self::RecoveryTools,
        Self::BatchExport,
        Self::MotionTools,
        Self::CharacterTools,
        Self::PropTools,
        Self::MeshTools,
        Self::OutfitTools,
        Self::FaceTools,
        Self::SceneTools,
        Self::AudioTools,
        Self::PixelBoxTools,
        Self::GameExport,
        Self::WorkspaceSettings,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "New project",
            Self::Open => "Open project",
            Self::Save => "Save project",
            Self::SaveAs => "Save project as",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Play => "Play / pause",
            Self::Delete => "Delete selection",
            Self::Fit => "Fit canvas",
            Self::Bone => "Draw bone",
            Self::Select => "Select bones",
            Self::MoveRig => "Move Rig",
            Self::IK => "IK tool",
            Self::Radius => "Radius tool",
            Self::NextFrame => "Next frame",
            Self::PreviousFrame => "Previous frame",
            Self::FirstFrame => "First frame",
            Self::LastFrame => "Last frame",
            Self::NextKey => "Next keyed frame",
            Self::PreviousKey => "Previous keyed frame",
            Self::KeyPose => "Key selected bone",
            Self::Cancel => "Cancel gesture / playback",
            Self::CopyKeys => "Copy selected keys",
            Self::PasteKeys => "Paste keys at playhead",
            Self::CyclePick => "Cycle overlapping bones",
            Self::Studio => "Open Studio tools",
            Self::Production => "Open Production tools",
            Self::CommandPalette => "Search commands",
            Self::Paint => "Pixel correction brush",
            Self::Boxes => "Draw collision region",
            Self::ShowSkeleton => "Show skeleton panel",
            Self::ShowInspector => "Show inspector panel",
            Self::ShowTimeline => "Show timeline panel",
            Self::RecoverPanels => "Recover hidden and floating panels",
            Self::ResetLayout => "Reset panel layout",
            Self::RiggingWorkspace => "Switch workspace to Rigging",
            Self::AnimationWorkspace => "Switch workspace to Animation",
            Self::SceneWorkspace => "Switch workspace to Scene",
            Self::RigTools => "Studio: Rig and pins",
            Self::AnimationTools => "Studio: Animation and curves",
            Self::AppearanceTools => "Studio: Appearance",
            Self::AssetLibrary => "Studio: Asset library",
            Self::ReferenceTools => "Studio: References",
            Self::RecoveryTools => "Studio: Recovery and files",
            Self::BatchExport => "Studio: Batch export",
            Self::MotionTools => "Production: Motion and animation templates",
            Self::CharacterTools => "Production: 3D characters",
            Self::PropTools => "Production: Props",
            Self::MeshTools => "Production: Meshes",
            Self::OutfitTools => "Production: Outfits",
            Self::FaceTools => "Production: Face",
            Self::SceneTools => "Production: Scene",
            Self::AudioTools => "Production: Audio and events",
            Self::PixelBoxTools => "Production: Pixels and boxes",
            Self::GameExport => "Production: Game export",
            Self::WorkspaceSettings => "Workspace settings and shortcuts",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub action: Action,
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
impl Binding {
    pub fn chord(&self) -> String {
        format!(
            "{}{}{}{}",
            if self.ctrl { "Ctrl+" } else { "" },
            if self.shift { "Shift+" } else { "" },
            if self.alt { "Alt+" } else { "" },
            self.key
        )
    }
    pub fn matches(&self, key: eframe::egui::Key, m: eframe::egui::Modifiers) -> bool {
        eframe::egui::Key::from_name(&self.key) == Some(key)
            && self.ctrl == (m.command || m.ctrl)
            && self.shift == m.shift
            && self.alt == m.alt
    }
}
pub fn default_bindings() -> Vec<Binding> {
    use Action::*;
    [
        (New, "N", true, false),
        (Open, "O", true, false),
        (Save, "S", true, false),
        (SaveAs, "S", true, true),
        (Undo, "Z", true, false),
        (Redo, "Z", true, true),
        (Redo, "Y", true, false),
        (Play, "Space", false, false),
        (Delete, "Delete", false, false),
        (Fit, "F", false, false),
        (Fit, "0", false, false),
        (Bone, "B", false, false),
        (Select, "V", false, false),
        (MoveRig, "M", false, false),
        (IK, "I", false, false),
        (Radius, "R", false, false),
        (NextFrame, "ArrowRight", false, false),
        (PreviousFrame, "ArrowLeft", false, false),
        (FirstFrame, "Home", false, false),
        (LastFrame, "End", false, false),
        (NextKey, "ArrowRight", false, true),
        (PreviousKey, "ArrowLeft", false, true),
        (KeyPose, "K", false, false),
        (Cancel, "Escape", false, false),
        (CopyKeys, "C", true, false),
        (PasteKeys, "V", true, false),
        (CyclePick, "Tab", false, false),
        (Studio, "F2", false, false),
        (Production, "F3", false, false),
        (CommandPalette, "P", true, false),
        (Paint, "P", false, false),
        (Boxes, "H", false, false),
    ]
    .into_iter()
    .map(|(action, key, ctrl, shift)| Binding {
        action,
        key: key.into(),
        ctrl,
        shift,
        alt: false,
    })
    .collect()
}

/// Add newly introduced transport shortcuts without replacing a user's bindings.
/// An occupied default chord or an already configured action is left untouched.
pub fn upgrade_navigation_bindings(bindings: &mut Vec<Binding>) -> bool {
    let previous_len = bindings.len();
    for binding in default_bindings().into_iter().filter(|binding| {
        matches!(
            binding.action,
            Action::FirstFrame | Action::LastFrame | Action::NextKey | Action::PreviousKey
        )
    }) {
        if bindings.len() >= 128 {
            break;
        }
        let key = eframe::egui::Key::from_name(&binding.key);
        let occupied = bindings.iter().any(|existing| {
            existing.action == binding.action
                || (eframe::egui::Key::from_name(&existing.key) == key
                    && existing.ctrl == binding.ctrl
                    && existing.shift == binding.shift
                    && existing.alt == binding.alt)
        });
        if !occupied {
            bindings.push(binding);
        }
    }
    bindings.len() != previous_len
}

pub fn validate_bindings(bindings: &[Binding]) -> Result<()> {
    ensure!(bindings.len() <= 128, "Too many shortcuts");
    let mut chords = std::collections::HashSet::new();
    for b in bindings {
        let key = eframe::egui::Key::from_name(&b.key)
            .ok_or_else(|| anyhow::anyhow!("Unknown shortcut key: {}", b.key))?;
        ensure!(
            chords.insert((key, b.ctrl, b.shift, b.alt)),
            "Two commands use {}",
            b.chord()
        );
    }
    Ok(())
}
