use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use serde_json::Value as Json;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::manifest::Manifest;
use crate::runtime::{HostBridge, HostContext, LuaRuntime};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WorkerRequest {
    Call {
        call_id: String,
        method: String,
        args: Vec<Json>,
        context: HostContext,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Ready {
        id: String,
    },
    Result {
        call_id: String,
        id: String,
        method: String,
        value: Json,
    },
    Error {
        call_id: String,
        id: String,
        method: String,
        message: String,
    },
}

pub struct WorkerHandle {
    pub id: String,
    pub requests: UnboundedSender<WorkerRequest>,
    pub cancel: Arc<AtomicBool>,
}

pub fn spawn(
    manifest: Manifest,
    root: PathBuf,
    grants: Vec<String>,
    bridge: Box<dyn HostBridge>,
    context: HostContext,
) -> (WorkerHandle, UnboundedReceiver<WorkerEvent>) {
    let (event_tx, event_rx) = unbounded_channel();
    let handle = spawn_with_events(
        manifest,
        root,
        grants,
        bridge,
        context,
        event_tx,
        Arc::new(AtomicBool::new(false)),
    );
    (handle, event_rx)
}

pub fn spawn_with_events(
    manifest: Manifest,
    root: PathBuf,
    grants: Vec<String>,
    bridge: Box<dyn HostBridge>,
    context: HostContext,
    event_tx: UnboundedSender<WorkerEvent>,
    cancel: Arc<AtomicBool>,
) -> WorkerHandle {
    let id = manifest.id.clone();
    let handle_id = id.clone();
    let thread_cancel = cancel.clone();
    let (request_tx, mut request_rx) = unbounded_channel::<WorkerRequest>();
    let thread_id = id.clone();
    thread::Builder::new()
        .name(format!("rchamber-ext-{thread_id}"))
        .spawn(move || {
            let runtime =
                LuaRuntime::new(&manifest, &grants, context, bridge, thread_cancel.clone());
            let mut runtime = match runtime {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = event_tx.send(WorkerEvent::Error {
                        call_id: "load".into(),
                        id,
                        method: "load".into(),
                        message: error.to_string(),
                    });
                    return;
                }
            };
            if let Err(error) = runtime.load_from_root(&root, &manifest) {
                let _ = event_tx.send(WorkerEvent::Error {
                    call_id: "load".into(),
                    id,
                    method: "load".into(),
                    message: error.to_string(),
                });
                return;
            }
            let _ = event_tx.send(WorkerEvent::Ready { id: id.clone() });
            while let Some(request) = request_rx.blocking_recv() {
                if thread_cancel.load(Ordering::Relaxed) {
                    break;
                }
                match request {
                    WorkerRequest::Shutdown => break,
                    WorkerRequest::Call {
                        call_id,
                        method,
                        args,
                        context,
                    } => {
                        runtime.set_context(context);
                        match runtime.call_multi(&method, args) {
                            Ok(value) => {
                                let _ = event_tx.send(WorkerEvent::Result {
                                    call_id,
                                    id: id.clone(),
                                    method,
                                    value: value.unwrap_or(Json::Null),
                                });
                            }
                            Err(error) => {
                                let timeout =
                                    matches!(error, crate::runtime::RuntimeError::Timeout);
                                let _ = event_tx.send(WorkerEvent::Error {
                                    call_id,
                                    id: id.clone(),
                                    method,
                                    message: error.to_string(),
                                });
                                if timeout {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        })
        .expect("spawn extension worker");
    WorkerHandle {
        id: handle_id,
        requests: request_tx,
        cancel,
    }
}
