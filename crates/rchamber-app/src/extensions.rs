use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use rchamber_ext::{
    ExtensionStorage, HostBridge, HostCall, HostContext, HostEffect, InstallSource,
    InstalledExtension, Registry, Widget, WorkerEvent, WorkerHandle, WorkerRequest,
    spawn_with_events,
};
use serde_json::Value as Json;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::action::BackendEvent;

#[derive(Clone, Debug)]
pub struct MentionRow {
    pub extension_id: String,
    pub extension_name: String,
    pub label: String,
    pub detail: String,
    pub text: String,
}

#[derive(Debug)]
pub enum BridgeMessage {
    Call {
        extension_id: String,
        call: HostCall,
        reply: Sender<Result<Json, String>>,
    },
    Effect {
        extension_id: String,
        effect: HostEffect,
    },
}

struct ExtensionBridge {
    id: String,
    tx: UnboundedSender<BridgeMessage>,
    storage: RefCell<ExtensionStorage>,
    path: PathBuf,
    cancel: Arc<AtomicBool>,
}

impl ExtensionBridge {
    fn new(
        id: String,
        tx: UnboundedSender<BridgeMessage>,
        path: PathBuf,
        storage: ExtensionStorage,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            id,
            tx,
            storage: RefCell::new(storage),
            path,
            cancel,
        }
    }
}

impl HostBridge for ExtensionBridge {
    fn call(&mut self, call: HostCall) -> Result<Json, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("the extension was disabled".into());
        }
        let (reply, receiver) = channel();
        self.tx
            .send(BridgeMessage::Call {
                extension_id: self.id.clone(),
                call,
                reply,
            })
            .map_err(|_| "the extension host is not running".to_string())?;
        receiver
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| "the extension host did not answer in time".to_string())?
    }

    fn effect(&mut self, effect: HostEffect) {
        if self.cancel.load(Ordering::Relaxed) {
            return;
        }
        let _ = self.tx.send(BridgeMessage::Effect {
            extension_id: self.id.clone(),
            effect,
        });
    }

    fn storage_get(&self, key: &str) -> Option<Json> {
        self.storage.borrow().get(key).cloned()
    }

    fn storage_set(&mut self, key: &str, value: Json) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("the extension was disabled".into());
        }
        self.storage
            .borrow_mut()
            .set(key, value)
            .map_err(|error| error.to_string())?;
        self.storage
            .borrow()
            .save(&self.path)
            .map_err(|error| error.to_string())
    }

    fn storage_delete(&mut self, key: &str) -> Result<bool, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("the extension was disabled".into());
        }
        let removed = self.storage.borrow_mut().delete(key);
        if removed {
            self.storage
                .borrow()
                .save(&self.path)
                .map_err(|error| error.to_string())?;
        }
        Ok(removed)
    }

    fn storage_keys(&self) -> Vec<String> {
        self.storage.borrow().keys()
    }
}

pub struct ExtensionHost {
    registry: Registry,
    workers: HashMap<String, WorkerHandle>,
    event_tx: UnboundedSender<WorkerEvent>,
    pub events: UnboundedReceiver<WorkerEvent>,
    bridge_tx: UnboundedSender<BridgeMessage>,
    pub bridge_rx: UnboundedReceiver<BridgeMessage>,
    blocks: HashMap<String, Vec<Widget>>,
    pending_blocks: HashSet<String>,
    pending_calls: HashSet<String>,
    failed: HashSet<String>,
    context: HostContext,
}

impl ExtensionHost {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let registry = Registry::load(root);
        let (event_tx, events) = unbounded_channel();
        let (bridge_tx, bridge_rx) = unbounded_channel();
        Self {
            registry,
            workers: HashMap::new(),
            event_tx,
            events,
            bridge_tx,
            bridge_rx,
            blocks: HashMap::new(),
            pending_blocks: HashSet::new(),
            pending_calls: HashSet::new(),
            failed: HashSet::new(),
            context: HostContext::default(),
        }
    }

    pub fn catalog(&self) -> Vec<InstalledExtension> {
        self.registry.list().to_vec()
    }

    pub fn root(&self) -> &Path {
        self.registry.root()
    }

    pub fn active(&self) -> Vec<InstalledExtension> {
        self.registry.active().cloned().collect()
    }

    pub fn set_context(&mut self, context: HostContext) {
        self.context = context;
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.registry
            .get(id)
            .is_some_and(InstalledExtension::is_active)
    }

    pub fn capability(&self, id: &str, name: &str) -> bool {
        self.registry
            .get(id)
            .is_some_and(|extension| extension.capability(name))
    }

    pub fn ensure_workers(&mut self) {
        let active: HashMap<String, InstalledExtension> = self
            .registry
            .active()
            .map(|extension| (extension.manifest.id.clone(), extension.clone()))
            .collect();
        let stale: Vec<String> = self
            .workers
            .keys()
            .filter(|id| !active.contains_key(*id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(worker) = self.workers.remove(&id) {
                worker.cancel.store(true, Ordering::Relaxed);
                let _ = worker.requests.send(WorkerRequest::Shutdown);
            }
        }
        for (id, extension) in active {
            if self.workers.contains_key(&id) || self.failed.contains(&id) {
                continue;
            }
            let storage_path = extension.storage_path(self.registry.root());
            let storage = ExtensionStorage::load(&storage_path).unwrap_or_default();
            let cancel = Arc::new(AtomicBool::new(false));
            let bridge = Box::new(ExtensionBridge::new(
                id.clone(),
                self.bridge_tx.clone(),
                storage_path,
                storage,
                cancel.clone(),
            ));
            let worker = spawn_with_events(
                extension.manifest.clone(),
                extension.root.clone(),
                extension.grants.clone(),
                bridge,
                self.context.clone(),
                self.event_tx.clone(),
                cancel,
            );
            self.workers.insert(id, worker);
        }
    }

    pub fn shutdown(&mut self) {
        for worker in self.workers.drain() {
            worker.1.cancel.store(true, Ordering::Relaxed);
            let _ = worker.1.requests.send(WorkerRequest::Shutdown);
        }
        self.pending_calls.clear();
    }

    pub fn call(&mut self, id: &str, call_id: &str, method: &str, args: Vec<Json>) -> bool {
        let Some(worker) = self.workers.get(id) else {
            return false;
        };
        let key = format!("{id}:{call_id}");
        if matches!(method, "status" | "panel") && !self.pending_calls.insert(key.clone()) {
            return true;
        }
        let sent = worker
            .requests
            .send(WorkerRequest::Call {
                call_id: call_id.to_string(),
                method: method.to_string(),
                args,
                context: self.context.clone(),
            })
            .is_ok();
        if !sent {
            self.pending_calls.remove(&key);
        }
        sent
    }

    pub fn broadcast_event(&mut self, name: &str, payload: Json) {
        let listeners: Vec<String> = self
            .registry
            .active()
            .filter(|extension| {
                extension
                    .manifest
                    .contributes
                    .events
                    .iter()
                    .any(|event| event == name || event == "*")
            })
            .map(|extension| extension.manifest.id.clone())
            .collect();
        for id in listeners {
            self.call(
                &id,
                "event",
                "on_event",
                vec![Json::String(name.to_string()), payload.clone()],
            );
        }
    }

    pub fn request_mentions(&mut self, query: &str) {
        let targets: Vec<String> = self
            .registry
            .active()
            .filter(|extension| extension.manifest.contributes.mentions.is_some())
            .map(|extension| extension.manifest.id.clone())
            .collect();
        for id in targets {
            self.call(
                &id,
                "mentions",
                "mentions",
                vec![Json::String(query.to_string())],
            );
        }
    }

    pub fn request_block(&mut self, name: &str, args: &[String]) -> bool {
        let key = block_key(name, args);
        if self.blocks.contains_key(&key) {
            return true;
        }
        if !self.pending_blocks.insert(key.clone()) {
            return true;
        }
        let Some(extension) = self.registry.active().find(|extension| {
            extension
                .manifest
                .contributes
                .blocks
                .iter()
                .any(|block| block.name == name)
        }) else {
            self.pending_blocks.remove(&key);
            return false;
        };
        let id = extension.manifest.id.clone();
        let args_json = serde_json::to_value(args).unwrap_or(Json::Array(Vec::new()));
        if self.call(
            &id,
            &format!("block:{key}"),
            "block",
            vec![Json::String(name.to_string()), args_json],
        ) {
            true
        } else {
            self.pending_blocks.remove(&key);
            false
        }
    }

    pub fn block(&self, name: &str, args: &[String]) -> Option<&[Widget]> {
        self.blocks.get(&block_key(name, args)).map(Vec::as_slice)
    }

    pub fn worker_event(&mut self, event: WorkerEvent) -> Option<BackendEvent> {
        match event {
            WorkerEvent::Ready { .. } => None,
            WorkerEvent::Result {
                call_id,
                id,
                method,
                value,
            } => {
                self.pending_calls.remove(&format!("{id}:{call_id}"));
                if method == "mentions" {
                    let rows = mention_rows(&id, &self.name_of(&id), &value);
                    return Some(BackendEvent::ExtensionMentions {
                        extension_id: id,
                        rows,
                    });
                }
                if let Some(key) = call_id.strip_prefix("block:") {
                    self.pending_blocks.remove(key);
                    let widgets = rchamber_ext::manifest::root_widget(&value).unwrap_or_default();
                    self.blocks.insert(key.to_string(), widgets.clone());
                    return Some(BackendEvent::ExtensionBlock {
                        key: key.to_string(),
                        widgets,
                    });
                }
                if call_id.starts_with("file:") {
                    let widgets = rchamber_ext::manifest::root_widget(&value).unwrap_or_default();
                    self.blocks.insert(call_id.clone(), widgets.clone());
                    return Some(BackendEvent::ExtensionBlock {
                        key: call_id,
                        widgets,
                    });
                }
                Some(BackendEvent::ExtensionResult {
                    call_id,
                    extension_id: id,
                    method,
                    value,
                })
            }
            WorkerEvent::Error {
                call_id,
                id,
                method,
                message,
            } => {
                tracing::warn!(target: "rchamber_ext", extension = %id, %call_id, %method, %message, "extension call failed");
                self.pending_calls.remove(&format!("{id}:{call_id}"));
                if method == "load"
                    || call_id == "load"
                    || message.contains("time limit")
                    || message.contains("timed out")
                {
                    self.failed.insert(id.clone());
                    self.workers.remove(&id);
                }
                if let Some(key) = call_id.strip_prefix("block:") {
                    self.pending_blocks.remove(key);
                }
                Some(BackendEvent::ExtensionError {
                    call_id,
                    extension_id: id,
                    method,
                    message,
                })
            }
        }
    }

    pub fn install_path(&mut self, path: &Path) -> Result<InstalledExtension, String> {
        let extension = self
            .registry
            .install_path(path)
            .map_err(|error| error.to_string())?;
        self.ensure_workers();
        Ok(extension)
    }

    pub fn install_zip(&mut self, path: &Path) -> Result<InstalledExtension, String> {
        let extension = self
            .registry
            .install_zip_path(path)
            .map_err(|error| error.to_string())?;
        self.ensure_workers();
        Ok(extension)
    }

    pub fn approve(&mut self, id: &str, grants: Vec<String>) -> Result<(), String> {
        self.registry
            .approve(id, grants)
            .map_err(|error| error.to_string())?;
        self.failed.remove(id);
        self.ensure_workers();
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        self.registry
            .set_enabled(id, enabled)
            .map_err(|error| error.to_string())?;
        if enabled {
            self.failed.remove(id);
        }
        self.ensure_workers();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        self.failed.remove(id);
        if let Some(worker) = self.workers.remove(id) {
            worker
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = worker.requests.send(WorkerRequest::Shutdown);
        }
        self.registry
            .remove(id)
            .map_err(|error| error.to_string())?;
        self.blocks
            .retain(|key, _| !key.starts_with(&format!("{id}:")));
        self.ensure_workers();
        Ok(())
    }

    pub fn reload(&mut self, id: &str) -> Result<(), String> {
        self.failed.remove(id);
        if let Some(worker) = self.workers.remove(id) {
            worker
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = worker.requests.send(WorkerRequest::Shutdown);
        }
        self.registry
            .reload(id)
            .map_err(|error| error.to_string())?;
        self.ensure_workers();
        Ok(())
    }

    fn name_of(&self, id: &str) -> String {
        self.registry
            .get(id)
            .map(|extension| extension.manifest.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn status_extensions(&self) -> Vec<(String, String, u64)> {
        self.registry
            .active()
            .filter_map(|extension| {
                extension
                    .manifest
                    .contributes
                    .status_section
                    .as_ref()
                    .map(|status| {
                        (
                            extension.manifest.id.clone(),
                            status.method.clone(),
                            status.interval,
                        )
                    })
            })
            .collect()
    }
}

fn block_key(name: &str, args: &[String]) -> String {
    format!("{name}:{}", args.join("\u{1f}"))
}

fn mention_rows(extension_id: &str, extension_name: &str, value: &Json) -> Vec<MentionRow> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            let text = item
                .get("text")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            Some(MentionRow {
                extension_id: extension_id.to_string(),
                extension_name: extension_name.to_string(),
                label,
                detail: item
                    .get("detail")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                text,
            })
        })
        .collect()
}

pub fn install_source_for(path: &Path) -> InstallSource {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        InstallSource::Zip
    } else {
        InstallSource::Path {
            path: path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_approves_and_calls_status() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("demo");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("rchamber.json"),
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "version": "1.0.0",
                "capabilities": ["storage"],
                "contributes": { "statusSection": { "title": "Demo" } }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            package.join("main.lua"),
            r#"
function on_load()
  host.storage_set("loaded", 1)
end

function status()
  host.storage_set("runs", 1)
  host.toast({ kind = "info", message = "status called" })
  return {
    { kind = "stat", label = "Running", value = "1", tone = "success" },
    { kind = "table", columns = {"a"}, rows = {{"b"}} },
    { kind = "table", columns = {"empty"}, rows = {} }
  }
end
"#,
        )
        .unwrap();
        let root = dir.path().join("registry");
        let mut host = ExtensionHost::new(&root);
        host.install_path(&package).unwrap();
        host.approve("demo", vec!["storage".into()]).unwrap();
        host.ensure_workers();
        let ready = host.events.blocking_recv().expect("worker event");
        assert!(matches!(ready, WorkerEvent::Ready { .. }));
        assert!(host.call("demo", "status", "status", Vec::new()));
        let result = host.events.blocking_recv().expect("status result");
        match result {
            WorkerEvent::Result { value, .. } => {
                let widgets = rchamber_ext::manifest::root_widget(&value).unwrap();
                assert_eq!(widgets.len(), 3);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        let effect = host.bridge_rx.blocking_recv().expect("bridge effect");
        assert!(matches!(effect, BridgeMessage::Effect { .. }));
        let storage = ExtensionStorage::load(
            &host
                .registry
                .get("demo")
                .unwrap()
                .storage_path(host.registry.root()),
        )
        .unwrap();
        assert_eq!(storage.get("runs"), Some(&serde_json::json!(1)));
        assert_eq!(storage.get("loaded"), Some(&serde_json::json!(1)));
        host.shutdown();
    }
}
