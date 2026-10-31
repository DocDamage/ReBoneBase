//! Versioned native project loading. Unsupported data is rejected before it can
//! be silently discarded by serde and subsequently overwritten by a save.
use crate::project::Project;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CURRENT_VERSION: u32 = 2;
pub const MAX_PROJECT_BYTES: u64 = 96 * 1024 * 1024;
const SUPPORTED_FEATURES: &[&str] = &["editable-clip-windows"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Compatibility {
    pub minimum_reader: u32,
    pub writer_version: String,
    #[serde(default)]
    pub required_features: Vec<String>,
}

impl Default for Compatibility {
    fn default() -> Self {
        Self {
            minimum_reader: CURRENT_VERSION,
            writer_version: env!("CARGO_PKG_VERSION").into(),
            required_features: vec!["editable-clip-windows".into()],
        }
    }
}

impl Compatibility {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.minimum_reader > 0 && self.minimum_reader <= CURRENT_VERSION,
            "This project requires a newer ReBoneBase reader (schema {}); this build supports schema {}. Open it with a compatible build; the original file has not been changed.",
            self.minimum_reader,
            CURRENT_VERSION
        );
        ensure!(
            !self.writer_version.is_empty() && self.writer_version.len() <= 128,
            "Invalid project writer version"
        );
        ensure!(
            self.required_features.len() <= 32,
            "Too many required project features"
        );
        for feature in &self.required_features {
            ensure!(
                SUPPORTED_FEATURES.contains(&feature.as_str()),
                "This project requires unsupported feature '{feature}'. Use a newer compatible build; the original file has not been changed."
            );
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct LoadReport {
    pub project: Project,
    pub migrated_projects: usize,
}

pub fn decode(bytes: &[u8]) -> Result<Project> {
    Ok(decode_with_report(bytes)?.project)
}

pub fn decode_with_report(bytes: &[u8]) -> Result<LoadReport> {
    ensure!(
        bytes.len() as u64 <= MAX_PROJECT_BYTES,
        "Project exceeds 96 MiB"
    );
    let mut document: Value = serde_json::from_slice(bytes).context("Invalid project JSON")?;
    let migrated_projects = migrate_document(&mut document, 0)?;
    let project = Project::deserialize(&document).context("Invalid project data")?;
    project.validate()?;
    // Serde normally ignores unknown fields. Compare its understood shape with
    // the input at this boundary, including nested objects, before allowing an
    // unsupported field to disappear on the next save.
    let understood = serde_json::to_value(&project)?;
    reject_unknown_fields(&document, &understood, "project")?;
    Ok(LoadReport {
        project,
        migrated_projects,
    })
}

fn migrate_document(document: &mut Value, depth: usize) -> Result<usize> {
    ensure!(
        depth <= 8,
        "Project actor nesting exceeds the migration limit"
    );
    let object = document
        .as_object_mut()
        .context("Project must be an object")?;
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .context("Project is missing a valid schema version")?;
    ensure!(
        (1..=u64::from(CURRENT_VERSION)).contains(&version),
        "Unsupported project schema {version}; this build reads schemas 1 through {CURRENT_VERSION}. Use a compatible build; the original file has not been changed."
    );
    if let Some(header) = object.get("compatibility") {
        let compatibility = Compatibility::deserialize(header)
            .context("Invalid project compatibility information")?;
        compatibility.validate()?;
        reject_unknown_fields(
            header,
            &serde_json::to_value(compatibility)?,
            "project.compatibility",
        )?;
    } else {
        ensure!(
            version == 1,
            "Schema 2 project is missing compatibility information"
        );
    }
    let mut migrated = 0;
    if version == 1 {
        object.insert("version".into(), Value::from(CURRENT_VERSION));
        // A v1 document has no native representation for editable source windows.
        // It becomes a normal v2 document without changing its rig or artwork.
        object.insert(
            "compatibility".into(),
            serde_json::to_value(Compatibility::default())?,
        );
        migrated += 1;
    }
    if let Some(clips) = object
        .get_mut("rig")
        .and_then(|rig| rig.get_mut("clips"))
        .and_then(Value::as_array_mut)
    {
        for clip in clips {
            if let Some(clip) = clip.as_object_mut() {
                if let Some(looped) = clip.remove("looped") {
                    ensure!(
                        !clip.contains_key("loop"),
                        "Clip contains both 'loop' and 'looped'"
                    );
                    clip.insert("loop".into(), looped);
                }
                // This known optional field is omitted when serialized as None.
                // Normalize only that field; unknown nulls may carry semantics.
                if clip.get("source").is_some_and(Value::is_null) {
                    clip.remove("source");
                }
            }
        }
    }
    if let Some(actors) = object
        .get_mut("extras")
        .and_then(|extras| extras.get_mut("production"))
        .and_then(|production| production.get_mut("actors"))
        .and_then(Value::as_array_mut)
    {
        for actor in actors {
            let child = actor
                .get_mut("project")
                .context("Scene actor is missing its project")?;
            migrated += migrate_document(child, depth + 1)?;
        }
    }
    Ok(migrated)
}

fn reject_unknown_fields(input: &Value, understood: &Value, path: &str) -> Result<()> {
    match (input, understood) {
        (Value::Object(input), Value::Object(known)) => {
            for (key, value) in input {
                if let Some(target) = known.get(key) {
                    reject_unknown_fields(value, target, &format!("{path}.{key}"))?;
                } else {
                    bail!(
                        "Unsupported project field '{path}.{key}'. Loading was stopped to avoid losing data on save; use a compatible build."
                    );
                }
            }
        }
        (Value::Array(input), Value::Array(known)) => {
            ensure!(
                input.len() == known.len(),
                "Unsupported project collection at '{path}'"
            );
            for (index, (value, target)) in input.iter().zip(known).enumerate() {
                reject_unknown_fields(value, target, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}
