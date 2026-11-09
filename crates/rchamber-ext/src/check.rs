use std::path::Path;

use serde_json::Value as Json;

use crate::manifest::Manifest;
use crate::runtime::{HostBridge, HostCall, HostContext, HostEffect, LuaRuntime};

#[derive(Clone, Debug, Default)]
pub struct CheckReport {
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub methods: Vec<String>,
    pub warnings: Vec<String>,
}

struct CheckBridge;

impl HostBridge for CheckBridge {
    fn call(&mut self, _call: HostCall) -> Result<Json, String> {
        Err("host calls are disabled during check".into())
    }

    fn effect(&mut self, _effect: HostEffect) {}

    fn storage_get(&self, _key: &str) -> Option<Json> {
        None
    }

    fn storage_set(&mut self, _key: &str, _value: Json) -> Result<(), String> {
        Ok(())
    }

    fn storage_delete(&mut self, _key: &str) -> Result<bool, String> {
        Ok(false)
    }

    fn storage_keys(&self) -> Vec<String> {
        Vec::new()
    }
}

pub fn check_syntax(root: &Path, manifest: &Manifest) -> Result<(), String> {
    if manifest.runtime.kind != crate::manifest::RuntimeKind::Lua {
        return Err("only Lua extensions run in this build".into());
    }
    let entry = crate::manifest::safe_relative(&manifest.runtime.entry)
        .ok_or_else(|| "runtime.entry must stay inside the package".to_string())?;
    let path = root.join(entry);
    let source = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let grants = manifest.requested_capabilities();
    let mut runtime = LuaRuntime::new(
        manifest,
        &grants,
        HostContext::default(),
        Box::new(CheckBridge),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .map_err(|error| error.to_string())?;
    runtime
        .load_source(&source, &manifest.runtime.entry)
        .map_err(|error| error.to_string())
}

pub fn check_package(root: &Path) -> Result<CheckReport, String> {
    let manifest = Manifest::load(root).map_err(|error| error.to_string())?;
    if manifest.runtime.kind != crate::manifest::RuntimeKind::Lua {
        return Err("only Lua extensions run in this build".into());
    }
    let entry = crate::manifest::safe_relative(&manifest.runtime.entry)
        .ok_or_else(|| "runtime.entry must stay inside the package".to_string())?;
    let path = root.join(entry);
    let source = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let grants = manifest.requested_capabilities();
    let mut runtime = LuaRuntime::new(
        &manifest,
        &grants,
        HostContext::default(),
        Box::new(CheckBridge),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .map_err(|error| error.to_string())?;
    runtime
        .validate(&source, &manifest.runtime.entry)
        .map_err(|error| error.to_string())?;
    let mut methods = Vec::new();
    for method in [
        "status", "panel", "mentions", "command", "action", "block", "on_event",
    ] {
        if runtime.has_method(method) {
            methods.push(method.to_string());
        }
    }
    let mut warnings = Vec::new();
    let capabilities = manifest.requested_capabilities();
    if capabilities.iter().any(|capability| capability == "exec") {
        warnings.push("this extension can run commands on your computer".into());
    }
    Ok(CheckReport {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        capabilities,
        methods,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_a_minimal_package() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rchamber.json"),
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "version": "1.0.0",
                "contributes": { "statusSection": { "title": "Demo" } }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main.lua"),
            "function status() return {{ kind = \"text\", markdown = \"ok\" }} end",
        )
        .unwrap();
        let report = check_package(dir.path()).unwrap();
        assert_eq!(report.id, "demo");
        assert!(report.methods.contains(&"status".to_string()));
    }
}
