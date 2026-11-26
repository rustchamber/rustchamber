use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::manifest::Manifest;
use crate::runtime::{HostBridge, HostCall, HostContext, HostEffect, LuaRuntime};
use crate::storage::ExtensionStorage;

pub const WORKER_FLAG: &str = "--ext-worker";
const MAX_FRAME: usize = 4 * 1024 * 1024;
const REAP_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub enum WorkerRequest {
    Call {
        call_id: String,
        method: String,
        args: Vec<Json>,
        context: HostContext,
    },
    Reply {
        request_id: u64,
        result: Result<Json, String>,
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
    Log {
        id: String,
        level: String,
        message: String,
    },
    Effect {
        id: String,
        effect: HostEffect,
    },
    HostCall {
        id: String,
        request_id: u64,
        call: HostCall,
        reply: UnboundedSender<WorkerRequest>,
    },
}

pub struct WorkerHandle {
    pub id: String,
    pub requests: UnboundedSender<WorkerRequest>,
    pub cancel: Arc<AtomicBool>,
}

impl WorkerHandle {
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.requests.send(WorkerRequest::Shutdown);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
enum Frame {
    Init {
        manifest: Box<Manifest>,
        root: PathBuf,
        grants: Vec<String>,
        storage: Option<PathBuf>,
        context: HostContext,
    },
    Call {
        call_id: String,
        method: String,
        args: Vec<Json>,
        context: HostContext,
    },
    Reply {
        request_id: u64,
        #[serde(default)]
        value: Option<Json>,
        #[serde(default)]
        error: Option<String>,
    },
    Shutdown,
    Ready,
    Result {
        call_id: String,
        method: String,
        value: Json,
    },
    Error {
        call_id: String,
        method: String,
        message: String,
    },
    HostCall {
        request_id: u64,
        call: HostCall,
    },
    Effect {
        effect: HostEffect,
    },
    Log {
        level: String,
        message: String,
    },
}

pub fn spawn_thread(
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
                    WorkerRequest::Reply { .. } => {}
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

pub struct ProcessSpawn {
    pub command: PathBuf,
    pub manifest: Manifest,
    pub root: PathBuf,
    pub grants: Vec<String>,
    pub storage: PathBuf,
    pub context: HostContext,
    pub event_tx: UnboundedSender<WorkerEvent>,
    pub cancel: Arc<AtomicBool>,
}

pub fn spawn_process(spawn: ProcessSpawn) -> std::io::Result<WorkerHandle> {
    use std::process::{Command, Stdio};

    let ProcessSpawn {
        command,
        manifest,
        root,
        grants,
        storage,
        context,
        event_tx,
        cancel,
    } = spawn;
    let id = manifest.id.clone();
    let mut child = Command::new(&command)
        .arg(WORKER_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the worker has no stdin")
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (request_tx, request_rx) = unbounded_channel::<WorkerRequest>();

    if let Err(error) = write_frame(
        &mut stdin,
        &Frame::Init {
            manifest: Box::new(manifest),
            root,
            grants,
            storage: Some(storage),
            context,
        },
    ) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }

    if let Some(stderr) = stderr {
        let log_tx = event_tx.clone();
        let log_id = id.clone();
        thread::Builder::new()
            .name(format!("rchamber-ext-log-{log_id}"))
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let _ = log_tx.send(WorkerEvent::Log {
                        id: log_id.clone(),
                        level: "error".into(),
                        message: line,
                    });
                }
            })?;
    }

    if let Some(stdout) = stdout {
        let read_tx = event_tx.clone();
        let read_id = id.clone();
        let read_requests = request_tx.clone();
        let read_cancel = cancel.clone();
        thread::Builder::new()
            .name(format!("rchamber-ext-out-{read_id}"))
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                let mut errored = false;
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    if line.len() > MAX_FRAME {
                        continue;
                    }
                    let Ok(frame) = serde_json::from_str::<Frame>(line.trim_end()) else {
                        continue;
                    };
                    let event = match frame {
                        Frame::Ready => WorkerEvent::Ready {
                            id: read_id.clone(),
                        },
                        Frame::Result {
                            call_id,
                            method,
                            value,
                        } => WorkerEvent::Result {
                            call_id,
                            id: read_id.clone(),
                            method,
                            value,
                        },
                        Frame::Error {
                            call_id,
                            method,
                            message,
                        } => {
                            errored = true;
                            WorkerEvent::Error {
                                call_id,
                                id: read_id.clone(),
                                method,
                                message,
                            }
                        }
                        Frame::Log { level, message } => WorkerEvent::Log {
                            id: read_id.clone(),
                            level,
                            message,
                        },
                        Frame::HostCall { request_id, call } => WorkerEvent::HostCall {
                            id: read_id.clone(),
                            request_id,
                            call,
                            reply: read_requests.clone(),
                        },
                        Frame::Effect { effect } => WorkerEvent::Effect {
                            id: read_id.clone(),
                            effect,
                        },
                        _ => continue,
                    };
                    if read_tx.send(event).is_err() {
                        break;
                    }
                }
                if !errored && !read_cancel.load(Ordering::Relaxed) {
                    let _ = read_tx.send(WorkerEvent::Error {
                        call_id: "exit".into(),
                        id: read_id,
                        method: "exit".into(),
                        message: "the extension process stopped".into(),
                    });
                }
            })?;
    }

    {
        let mut writer = stdin;
        let mut request_rx: UnboundedReceiver<WorkerRequest> = request_rx;
        let writer_cancel = cancel.clone();
        let writer_id = id.clone();
        thread::Builder::new()
            .name(format!("rchamber-ext-in-{writer_id}"))
            .spawn(move || {
                while let Some(request) = request_rx.blocking_recv() {
                    let frame = match request {
                        WorkerRequest::Call {
                            call_id,
                            method,
                            args,
                            context,
                        } => Frame::Call {
                            call_id,
                            method,
                            args,
                            context,
                        },
                        WorkerRequest::Reply { request_id, result } => match result {
                            Ok(value) => Frame::Reply {
                                request_id,
                                value: Some(value),
                                error: None,
                            },
                            Err(error) => Frame::Reply {
                                request_id,
                                value: None,
                                error: Some(error),
                            },
                        },
                        WorkerRequest::Shutdown => {
                            let _ = write_frame(&mut writer, &Frame::Shutdown);
                            break;
                        }
                    };
                    if write_frame(&mut writer, &frame).is_err() {
                        break;
                    }
                }
                let _ = writer.flush();
                if writer_cancel.load(Ordering::Relaxed) {
                    let _ = write_frame(&mut writer, &Frame::Shutdown);
                }
            })?;
    }

    {
        let mut child = child;
        let reap_cancel = cancel.clone();
        thread::Builder::new()
            .name(format!("rchamber-ext-reap-{id}"))
            .spawn(move || {
                loop {
                    if reap_cancel.load(Ordering::Relaxed) {
                        let _ = child.kill();
                    }
                    match child.try_wait() {
                        Ok(Some(_)) | Err(_) => break,
                        Ok(None) => thread::sleep(REAP_INTERVAL),
                    }
                }
            })?;
    }

    Ok(WorkerHandle {
        id,
        requests: request_tx,
        cancel,
    })
}

fn write_frame(writer: &mut impl Write, frame: &Frame) -> std::io::Result<()> {
    let encoded = serde_json::to_string(frame)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    writer.write_all(encoded.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn read_frame(input: &Mutex<BufReader<std::io::Stdin>>) -> std::io::Result<Option<Frame>> {
    let mut line = String::new();
    {
        let mut input = input
            .lock()
            .map_err(|_| std::io::Error::other("the worker input is poisoned"))?;
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
    }
    if line.len() > MAX_FRAME {
        return Ok(None);
    }
    serde_json::from_str(line.trim_end())
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn send_frame(frame: &Frame) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    write_frame(&mut out, frame)
}

struct ChildBridge {
    storage: RefCell<ExtensionStorage>,
    storage_path: Option<PathBuf>,
    input: Arc<Mutex<BufReader<std::io::Stdin>>>,
    next_request: u64,
    stopped: Arc<AtomicBool>,
}

impl ChildBridge {
    fn request(&mut self, call: HostCall) -> Result<Json, String> {
        let request_id = self.next_request;
        self.next_request += 1;
        send_frame(&Frame::HostCall { request_id, call })
            .map_err(|error| format!("the extension host stopped: {error}"))?;
        loop {
            let frame = read_frame(&self.input)
                .map_err(|error| format!("could not reach the extension host: {error}"))?;
            match frame {
                None => {
                    self.stopped.store(true, Ordering::Relaxed);
                    return Err("the extension host stopped".into());
                }
                Some(Frame::Shutdown) => {
                    self.stopped.store(true, Ordering::Relaxed);
                    return Err("the extension was stopped".into());
                }
                Some(Frame::Reply {
                    request_id: answer,
                    value,
                    error,
                }) if answer == request_id => {
                    return match error {
                        Some(error) => Err(error),
                        None => Ok(value.unwrap_or(Json::Null)),
                    };
                }
                Some(_) => continue,
            }
        }
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.storage_path else {
            return Ok(());
        };
        self.storage
            .borrow()
            .save(path)
            .map_err(|error| error.to_string())
    }
}

impl HostBridge for ChildBridge {
    fn call(&mut self, call: HostCall) -> Result<Json, String> {
        self.request(call)
    }

    fn effect(&mut self, effect: HostEffect) {
        let _ = send_frame(&Frame::Effect { effect });
    }

    fn log(&mut self, level: &str, message: &str) {
        let _ = send_frame(&Frame::Log {
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    fn storage_get(&self, key: &str) -> Option<Json> {
        self.storage.borrow().get(key).cloned()
    }

    fn storage_set(&mut self, key: &str, value: Json) -> Result<(), String> {
        self.storage
            .borrow_mut()
            .set(key, value)
            .map_err(|error| error.to_string())?;
        self.save()
    }

    fn storage_delete(&mut self, key: &str) -> Result<bool, String> {
        let removed = self.storage.borrow_mut().delete(key);
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn storage_keys(&self) -> Vec<String> {
        self.storage.borrow().keys()
    }
}

pub fn serve_stdio() -> i32 {
    let input = Arc::new(Mutex::new(BufReader::new(std::io::stdin())));
    let init = match read_frame(&input) {
        Ok(Some(Frame::Init {
            manifest,
            root,
            grants,
            storage,
            context,
        })) => (*manifest, root, grants, storage, context),
        _ => return 2,
    };
    let (manifest, root, grants, storage_path, context) = init;
    let storage = storage_path
        .as_ref()
        .map(|path| ExtensionStorage::load(path).unwrap_or_default())
        .unwrap_or_default();
    let stopped = Arc::new(AtomicBool::new(false));
    let bridge = ChildBridge {
        storage: RefCell::new(storage),
        storage_path,
        input: input.clone(),
        next_request: 1,
        stopped: stopped.clone(),
    };
    let mut runtime = match LuaRuntime::new(
        &manifest,
        &grants,
        context,
        Box::new(bridge),
        Arc::new(AtomicBool::new(false)),
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = send_frame(&Frame::Error {
                call_id: "load".into(),
                method: "load".into(),
                message: error.to_string(),
            });
            return 1;
        }
    };
    if let Err(error) = runtime.load_from_root(&root, &manifest) {
        let _ = send_frame(&Frame::Error {
            call_id: "load".into(),
            method: "load".into(),
            message: error.to_string(),
        });
        return 1;
    }
    if send_frame(&Frame::Ready).is_err() {
        return 1;
    }
    loop {
        if stopped.load(Ordering::Relaxed) {
            break;
        }
        let frame = match read_frame(&input) {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(_) => break,
        };
        match frame {
            Frame::Shutdown => break,
            Frame::Call {
                call_id,
                method,
                args,
                context,
            } => {
                runtime.set_context(context);
                match runtime.call_multi(&method, args) {
                    Ok(value) => {
                        let _ = send_frame(&Frame::Result {
                            call_id,
                            method,
                            value: value.unwrap_or(Json::Null),
                        });
                    }
                    Err(error) => {
                        let timeout = matches!(error, crate::runtime::RuntimeError::Timeout);
                        let _ = send_frame(&Frame::Error {
                            call_id,
                            method,
                            message: error.to_string(),
                        });
                        if timeout {
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    0
}
