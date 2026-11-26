#![cfg(feature = "test-worker")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rchamber_ext::{
    HostCall, HostContext, Manifest, ProcessSpawn, WorkerEvent, WorkerHandle, WorkerRequest,
    spawn_process,
};
use serde_json::{Value as Json, json};
use tokio::sync::mpsc::UnboundedReceiver;

const STEP: Duration = Duration::from_secs(20);

fn package(dir: &Path) -> (Manifest, PathBuf) {
    let root = dir.join("probe");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("rchamber.json"),
        json!({
            "apiVersion": 1,
            "id": "probe",
            "name": "Probe",
            "version": "1.0.0",
            "capabilities": ["storage", "files"],
            "contributes": { "statusSection": { "title": "Probe" } }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("main.lua"),
        r#"
function on_load()
  host.log("loaded")
  host.storage_set("first", 7)
end

function status()
  host.log({ level = "warn", message = "status called" })
  local info = host.stat("probe")
  host.storage_set("last", info.kind)
  return { { kind = "stat", label = "Kind", value = info.kind } }
end
"#,
    )
    .unwrap();
    (Manifest::load(&root).unwrap(), root)
}

fn start(dir: &Path) -> (WorkerHandle, UnboundedReceiver<WorkerEvent>, PathBuf) {
    let (manifest, root) = package(dir);
    let storage = dir.join("storage.json");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = spawn_process(ProcessSpawn {
        command: PathBuf::from(env!("CARGO_BIN_EXE_ext-worker")),
        manifest,
        root,
        grants: vec!["storage".into(), "files".into()],
        storage: storage.clone(),
        context: HostContext::default(),
        event_tx: tx,
        cancel: Arc::new(AtomicBool::new(false)),
    })
    .unwrap();
    (handle, rx, storage)
}

async fn next(rx: &mut UnboundedReceiver<WorkerEvent>) -> WorkerEvent {
    tokio::time::timeout(STEP, rx.recv())
        .await
        .expect("the worker went quiet")
        .expect("the worker channel closed")
}

#[test]
fn a_process_worker_runs_lua_and_round_trips_host_calls() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (handle, mut rx, storage) = start(dir.path());
        let mut ready = false;
        let mut log = Vec::new();
        let value = loop {
            match next(&mut rx).await {
                WorkerEvent::Ready { .. } => {
                    ready = true;
                    assert!(
                        handle
                            .requests
                            .send(WorkerRequest::Call {
                                call_id: "status".into(),
                                method: "status".into(),
                                args: Vec::new(),
                                context: HostContext::default(),
                            })
                            .is_ok()
                    );
                }
                WorkerEvent::Log { level, message, .. } => {
                    log.push((level, message));
                }
                WorkerEvent::HostCall {
                    request_id,
                    call,
                    reply,
                    ..
                } => {
                    assert!(matches!(call, HostCall::Stat { .. }), "{call:?}");
                    reply
                        .send(WorkerRequest::Reply {
                            request_id,
                            result: Ok(json!({ "kind": "file", "size": 3 })),
                        })
                        .unwrap();
                }
                WorkerEvent::Error { message, .. } => panic!("{message}"),
                WorkerEvent::Result { call_id, value, .. } => {
                    assert_eq!(call_id, "status");
                    break value;
                }
                other => panic!("unexpected event: {other:?}"),
            }
        };
        assert!(ready);
        assert!(
            log.iter()
                .any(|(level, message)| level == "info" && message == "loaded"),
            "{log:?}"
        );
        assert!(
            log.iter()
                .any(|(level, message)| level == "warn" && message == "status called"),
            "{log:?}"
        );
        assert_eq!(value[0]["value"], Json::String("file".into()));

        let stored: Json = serde_json::from_slice(&std::fs::read(&storage).unwrap()).unwrap();
        assert_eq!(stored["values"]["last"], Json::String("file".into()));
        assert_eq!(stored["values"]["first"], json!(7));
        handle.stop();
    });
}

#[test]
fn a_process_worker_reports_a_lua_error_without_dying() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (handle, mut rx, _storage) = start(dir.path());
        loop {
            match next(&mut rx).await {
                WorkerEvent::Ready { .. } => break,
                WorkerEvent::Error { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        assert!(
            handle
                .requests
                .send(WorkerRequest::Call {
                    call_id: "missing".into(),
                    method: "no_such_method".into(),
                    args: Vec::new(),
                    context: HostContext::default(),
                })
                .is_ok()
        );
        let event = next(&mut rx).await;
        match event {
            WorkerEvent::Result { value, .. } => assert_eq!(value, Json::Null),
            other => panic!("a missing method should answer null, got {other:?}"),
        }
        handle.stop();
    });
}
