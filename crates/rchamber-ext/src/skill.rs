pub const AUTHORING_SKILL: &str = include_str!("../assets/authoring-skill.md");

pub fn materialize_authoring_skill() -> std::io::Result<std::path::PathBuf> {
    let base = directories::BaseDirs::new()
        .map(|base| base.config_dir().join("rustchamber"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let directory = base.join("skills").join("rchamber-extensions");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("SKILL.md");
    std::fs::write(&path, AUTHORING_SKILL)?;
    Ok(path)
}
