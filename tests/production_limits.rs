use rebonebase::{
    model::Rig,
    production::{
        Attachment, SceneActor, SpriteAsset,
        engine_export::{self, EngineTarget},
    },
    project::{Project, Reference},
};
use std::sync::Arc;
fn image(size: u32) -> Reference {
    Reference {
        width: size,
        height: size,
        pixels: Arc::new(vec![0; size as usize * size as usize * 4]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 1.,
    }
}
#[test]
fn per_frame_artwork_work_is_bounded_before_rendering() {
    let mut p = Project::new(Rig::new("Dense", 1024, 1024));
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "Art".into(),
        image: image(1024),
    });
    for i in 0..128 {
        p.extras.production.attachments.push(Attachment {
            id: i + 2,
            asset: 1,
            ..Default::default()
        });
    }
    assert!(
        p.validate().is_err(),
        "overlapping full-canvas props must be bounded before the UI renders"
    );
}
#[test]
fn embedded_actor_images_share_a_total_serialization_budget() {
    let mut p = Project::new(Rig::new("Scene", 16, 16));
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "Art".into(),
        image: image(2048),
    });
    for i in 0..4 {
        let mut child = Project::new(Rig::new("Actor", 16, 16));
        child.extras.production.assets = p.extras.production.assets.clone();
        p.extras.production.actors.push(SceneActor {
            id: i + 2,
            project: Box::new(child),
            ..Default::default()
        });
    }
    assert!(
        p.validate().is_err(),
        "embedded actor resources must be included in the project size budget"
    );
}
#[test]
fn package_preflight_uses_actor_canvas_dimensions_across_all_clips() {
    let mut p = Project::new(Rig::new("Small viewport", 2, 2));
    p.rig.clips[0].length = 240;
    p.rig.clips = vec![p.rig.clips[0].clone(); 32];
    p.extras.production.actors.push(SceneActor {
        id: 1,
        project: Box::new(Project::new(Rig::new("Large actor", 1024, 1024))),
        ..Default::default()
    });
    let root = std::env::temp_dir().join(format!(
        "rebone-package-limit-{}",
        rebonebase::storage::unique()
    ));
    let result = engine_export::export_package(&p, &root, EngineTarget::Generic, false);
    assert!(result.is_err());
    assert!(
        !root.exists(),
        "oversized packages must reject before creating output"
    );
}
