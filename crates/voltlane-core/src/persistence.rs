use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use tracing::{debug, info, instrument};

use crate::model::Project;

#[instrument(skip(project), fields(project_id = %project.id, path = %path.display()))]
pub fn save_project(path: &Path, project: &Project) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let json = serde_json::to_vec_pretty(project).context("failed to serialize project")?;
    let mut temp_file = tempfile::NamedTempFile::new_in(
        path.parent()
            .map_or_else(|| Path::new(".").to_path_buf(), Path::to_path_buf),
    )
    .context("failed to create temp project file")?;

    use std::io::Write;
    temp_file
        .write_all(&json)
        .context("failed to write temp project file")?;
    temp_file
        .persist(path)
        .map_err(|error| anyhow::anyhow!(error.error))
        .with_context(|| format!("failed to persist project: {}", path.display()))?;

    info!("project saved");
    Ok(())
}

#[instrument(fields(path = %path.display()))]
pub fn load_project(path: &Path) -> Result<Project> {
    let content =
        fs::read(path).with_context(|| format!("failed to read project: {}", path.display()))?;
    let project: Project = serde_json::from_slice(&content).context("invalid project json")?;
    info!(project_id = %project.id, "project loaded");
    Ok(project)
}

#[instrument(skip(project), fields(project_id = %project.id, autosave_dir = %autosave_dir.display()))]
pub fn autosave_project(project: &Project, autosave_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(autosave_dir).with_context(|| {
        format!(
            "failed to create autosave directory: {}",
            autosave_dir.display()
        )
    })?;

    let file_name = format!("{}.autosave.voltlane.json", project.id);
    let autosave_path = autosave_dir.join(file_name);
    save_project(&autosave_path, project)?;

    debug!(path = %autosave_path.display(), "autosave complete");
    Ok(autosave_path)
}
