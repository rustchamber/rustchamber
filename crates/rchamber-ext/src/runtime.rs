use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mlua::{HookTriggers, Lua, Table, Value, VmState};
use serde_json::{Map, Number, Value as Json};

use crate::manifest::{Manifest, RuntimeKind};
use crate::widget::Widget;

pub const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const HOOK_INSTRUCTIONS: u32 = 20_000;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct HostContext {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub session: Option<SessionContext>,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionContext {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Clone, Debug)]
pub enum HostCall {
    Http {
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Option<String>,
    },
    ReadFile {
        path: String,
    },
    WriteFile {
        path: String,
        content: String,
    },
    ListDir {
        path: String,
    },
    Stat {
        path: String,
    },
    SessionsList,
    SessionStart {
        project: Option<String>,
        title: Option<String>,
    },
    SessionOpen {
        id: String,
    },
    Prompt {
        session: Option<String>,
        text: String,
    },
    Generate {
        prompt: String,
        system: Option<String>,
    },
    Exec {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub enum HostEffect {
    Toast {
        kind: String,
        message: String,
    },
    Compose {
        text: String,
        send: bool,
    },
    Prompt {
        text: String,
        send: bool,
    },
    Attach {
        provider_id: String,
        id: String,
        title: String,
        url: Option<String>,
    },
    SetBadge {
        count: Option<i64>,
    },
    Answer {
        text: String,
    },
    OpenUrl {
        url: String,
    },
    Notify {
        title: String,
        body: String,
    },
    Clipboard(String),
}

pub trait HostBridge: Send {
    fn call(&mut self, call: HostCall) -> Result<Json, String>;
    fn effect(&mut self, effect: HostEffect);
    fn storage_get(&self, key: &str) -> Option<Json>;
    fn storage_set(&mut self, key: &str, value: Json) -> Result<(), String>;
    fn storage_delete(&mut self, key: &str) -> Result<bool, String>;
    fn storage_keys(&self) -> Vec<String>;
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Lua(#[from] mlua::Error),
    #[error("the extension timed out")]
    Timeout,
    #[error("{0}")]
    Invalid(String),
}

struct Policy {
    granted: BTreeSet<String>,
    origins: Vec<String>,
    timeout_ms: u64,
    extension_id: String,
    extension_name: String,
}

pub struct LuaRuntime {
    lua: Lua,
    deadline: Arc<AtomicU64>,
    timed_out: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    base: std::time::Instant,
    poisoned: bool,
    policy: Policy,
    context: Rc<RefCell<HostContext>>,
    bridge: Rc<RefCell<Box<dyn HostBridge>>>,
    loaded: bool,
}

impl LuaRuntime {
    pub fn new(
        manifest: &Manifest,
        grants: &[String],
        context: HostContext,
        bridge: Box<dyn HostBridge>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, RuntimeError> {
        if manifest.runtime.kind != RuntimeKind::Lua {
            return Err(RuntimeError::Invalid(
                "this runtime only runs Lua extensions".into(),
            ));
        }
        let lua = Lua::new();
        for name in [
            "load",
            "loadfile",
            "dofile",
            "require",
            "package",
            "io",
            "os",
            "debug",
            "pcall",
            "xpcall",
            "coroutine",
        ] {
            lua.globals().raw_set(name, Value::Nil)?;
        }
        lua.set_memory_limit(MEMORY_LIMIT)?;
        let deadline = Arc::new(AtomicU64::new(0));
        let timed_out = Arc::new(AtomicBool::new(false));
        let base = std::time::Instant::now();
        let hook_deadline = deadline.clone();
        let hook_timed_out = timed_out.clone();
        let hook_cancel = cancel.clone();
        let hook_base = base;
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(HOOK_INSTRUCTIONS),
            move |_lua, _debug| {
                if hook_cancel.load(Ordering::Relaxed) {
                    return Err(mlua::Error::RuntimeError(
                        "the extension was disabled".into(),
                    ));
                }
                let limit = hook_deadline.load(Ordering::Relaxed);
                let now = hook_base.elapsed().as_millis() as u64;
                if limit > 0 && now > limit {
                    hook_timed_out.store(true, Ordering::Relaxed);
                    Err(mlua::Error::RuntimeError(
                        "the extension exceeded its time limit".into(),
                    ))
                } else {
                    Ok(VmState::Continue)
                }
            },
        )?;
        let granted = grants
            .iter()
            .map(|capability| {
                capability
                    .split_once(':')
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| capability.clone())
            })
            .collect();
        let policy = Policy {
            granted,
            origins: grants
                .iter()
                .filter_map(|capability| capability.strip_prefix("network:"))
                .map(|origin| origin.trim_end_matches('/').to_string())
                .collect(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            extension_id: manifest.id.clone(),
            extension_name: manifest.name.clone(),
        };
        let context = Rc::new(RefCell::new(context));
        let bridge = Rc::new(RefCell::new(bridge));
        let mut runtime = Self {
            lua,
            deadline,
            timed_out,
            cancel,
            base,
            poisoned: false,
            policy,
            context,
            bridge,
            loaded: false,
        };
        runtime.install_host()?;
        Ok(runtime)
    }

    pub fn set_context(&self, context: HostContext) {
        *self.context.borrow_mut() = context;
    }

    pub fn has_method(&self, name: &str) -> bool {
        if self.begin_call().is_err() {
            return false;
        }
        let present = matches!(
            self.lua.globals().raw_get::<Value>(name),
            Ok(Value::Function(_))
        );
        self.end_call();
        present && !self.timed_out.load(Ordering::Relaxed)
    }

    pub fn load_from_root(
        &mut self,
        root: &std::path::Path,
        manifest: &Manifest,
    ) -> Result<(), RuntimeError> {
        let entry = crate::manifest::safe_relative(&manifest.runtime.entry).ok_or_else(|| {
            RuntimeError::Invalid("runtime.entry must stay inside the package".into())
        })?;
        let root = root.canonicalize().map_err(|error| {
            RuntimeError::Invalid(format!("could not open {}: {error}", root.display()))
        })?;
        let path = root.join(&entry).canonicalize().map_err(|error| {
            RuntimeError::Invalid(format!(
                "could not read {}: {error}",
                root.join(entry).display()
            ))
        })?;
        if !path.starts_with(&root) {
            return Err(RuntimeError::Invalid(
                "runtime.entry must stay inside the package".into(),
            ));
        }
        let source = std::fs::read_to_string(&path).map_err(|error| {
            RuntimeError::Invalid(format!("could not read {}: {error}", path.display()))
        })?;
        self.load_source(&source, &manifest.runtime.entry)?;
        let _ = self.call_multi("on_load", Vec::new())?;
        Ok(())
    }

    pub fn load_source(&mut self, source: &str, name: &str) -> Result<(), RuntimeError> {
        self.begin_call()?;
        let result = self.lua.load(source).set_name(name).exec();
        self.end_call();
        if self.timed_out.load(Ordering::Relaxed) {
            self.poisoned = true;
            return Err(RuntimeError::Timeout);
        }
        result.map_err(RuntimeError::Lua)?;
        self.loaded = true;
        Ok(())
    }

    pub fn call(&mut self, method: &str, args: Json) -> Result<Option<Json>, RuntimeError> {
        self.call_multi(method, vec![args])
    }

    pub fn call_multi(
        &mut self,
        method: &str,
        args: Vec<Json>,
    ) -> Result<Option<Json>, RuntimeError> {
        if !self.loaded {
            return Err(RuntimeError::Invalid("the extension is not loaded".into()));
        }
        if self.poisoned {
            return Err(RuntimeError::Invalid(
                "the extension was stopped after exceeding its time limit".into(),
            ));
        }
        self.begin_call()?;
        let function: Option<mlua::Function> = self.lua.globals().raw_get(method)?;
        let Some(function) = function else {
            self.end_call();
            return Ok(None);
        };
        let mut lua_args = Vec::with_capacity(args.len());
        for arg in &args {
            lua_args.push(json_to_lua(&self.lua, arg, 0)?);
        }
        let args_value = match args.len() {
            0 => Value::Nil,
            1 => lua_args[0].clone(),
            _ => json_to_lua(&self.lua, &Json::Array(args.clone()), 0)?,
        };
        self.lua.globals().raw_set("RCHAMBER_ARGS", args_value)?;
        let result = function.call::<Value>(mlua::Variadic::from_iter(lua_args));
        self.end_call();
        if self.timed_out.load(Ordering::Relaxed) {
            self.poisoned = true;
            return Err(RuntimeError::Timeout);
        }
        match result {
            Ok(Value::Nil) => Ok(None),
            Ok(value) => {
                let json = lua_to_json(value, 0);
                let encoded = serde_json::to_string(&json).unwrap_or_default();
                if encoded.len() > 1_000_000 {
                    return Err(RuntimeError::Invalid(
                        "the extension returned more than 1 MB of data".into(),
                    ));
                }
                Ok(Some(json))
            }
            Err(error) => Err(RuntimeError::Lua(error)),
        }
    }

    pub fn call_widget(
        &mut self,
        method: &str,
        args: Json,
    ) -> Result<Option<Vec<Widget>>, RuntimeError> {
        let Some(value) = self.call(method, args)? else {
            return Ok(None);
        };
        crate::manifest::root_widget(&value)
            .map(Some)
            .ok_or_else(|| RuntimeError::Invalid(format!("{method} did not return a widget tree")))
    }

    pub fn validate(&mut self, source: &str, name: &str) -> Result<(), RuntimeError> {
        self.load_source(source, name)?;
        let _ = self.call_multi("on_load", Vec::new())?;
        Ok(())
    }

    fn begin_call(&self) -> Result<(), RuntimeError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(RuntimeError::Invalid("the extension was disabled".into()));
        }
        if self.poisoned {
            return Err(RuntimeError::Invalid(
                "the extension was stopped after exceeding its time limit".into(),
            ));
        }
        self.timed_out.store(false, Ordering::Relaxed);
        let now = self.base.elapsed().as_millis() as u64;
        self.deadline
            .store(now + self.policy.timeout_ms, Ordering::Relaxed);
        Ok(())
    }

    fn end_call(&self) {
        self.deadline.store(0, Ordering::Relaxed);
    }

    fn install_host(&mut self) -> Result<(), RuntimeError> {
        let host = self.lua.create_table()?;
        let bridge = self.bridge.clone();
        let policy = Rc::new(PolicyView {
            granted: self.policy.granted.clone(),
            origins: self.policy.origins.clone(),
        });
        let context = self.context.clone();
        let extension_id = self.policy.extension_id.clone();
        let extension_name = self.policy.extension_name.clone();

        {
            let bridge = bridge.clone();
            host.set(
                "toast",
                self.lua.create_function(move |_, value: Table| {
                    let message = value.get::<String>("message")?;
                    let kind = value
                        .get::<Option<String>>("kind")?
                        .unwrap_or_else(|| "neutral".into());
                    bridge
                        .borrow_mut()
                        .effect(HostEffect::Toast { kind, message });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = Rc::clone(&policy);
            host.set(
                "compose",
                self.lua.create_function(move |_, value: Table| {
                    let text = value.get::<String>("text")?;
                    let send = value.get::<Option<bool>>("send")?.unwrap_or(false);
                    if send {
                        required.require("prompt")?;
                    }
                    bridge
                        .borrow_mut()
                        .effect(HostEffect::Compose { text, send });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = Rc::clone(&policy);
            host.set(
                "prompt",
                self.lua.create_function(move |_, value: Table| {
                    required.require("prompt")?;
                    let text = value.get::<String>("text")?;
                    let send = value.get::<Option<bool>>("send")?.unwrap_or(false);
                    bridge
                        .borrow_mut()
                        .effect(HostEffect::Prompt { text, send });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = Rc::clone(&policy);
            host.set(
                "attach",
                self.lua.create_function(move |_, value: Table| {
                    required.require("prompt")?;
                    bridge.borrow_mut().effect(HostEffect::Attach {
                        provider_id: value.get::<String>("provider_id")?,
                        id: value.get::<String>("id")?,
                        title: value.get::<String>("title")?,
                        url: value.get::<Option<String>>("url")?,
                    });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            host.set(
                "set_badge",
                self.lua.create_function(move |_, count: Option<i64>| {
                    bridge.borrow_mut().effect(HostEffect::SetBadge { count });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = Rc::clone(&policy);
            host.set(
                "answer",
                self.lua.create_function(move |_, text: String| {
                    required.require("prompt")?;
                    bridge.borrow_mut().effect(HostEffect::Answer { text });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = policy.clone();
            host.set(
                "open_url",
                self.lua.create_function(move |_, url: String| {
                    required.require("prompt")?;
                    if !(url.starts_with("https://")
                        || url.starts_with("http://")
                        || url.starts_with("mailto:"))
                    {
                        return Err(mlua::Error::RuntimeError(
                            "only http, https and mailto links can be opened".into(),
                        ));
                    }
                    bridge.borrow_mut().effect(HostEffect::OpenUrl { url });
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = Rc::clone(&policy);
            host.set(
                "clipboard",
                self.lua.create_function(move |_, text: String| {
                    required.require("clipboard")?;
                    bridge.borrow_mut().effect(HostEffect::Clipboard(text));
                    Ok(())
                })?,
            )?;
        }
        {
            let bridge = bridge.clone();
            let required = Rc::clone(&policy);
            let notify_name = extension_name.clone();
            host.set(
                "notify",
                self.lua.create_function(move |_, value: Table| {
                    required.require("notifications")?;
                    bridge.borrow_mut().effect(HostEffect::Notify {
                        title: value
                            .get::<Option<String>>("title")?
                            .unwrap_or_else(|| notify_name.clone()),
                        body: value.get::<String>("body")?,
                    });
                    Ok(())
                })?,
            )?;
        }
        {
            let context = context.clone();
            let extension_id = extension_id.clone();
            let extension_name = extension_name.clone();
            host.set(
                "context",
                self.lua.create_function(move |lua, _: ()| {
                    let value = serde_json::to_value(&*context.borrow())
                        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
                    let mut map = match value {
                        Json::Object(map) => map,
                        _ => Map::new(),
                    };
                    map.insert(
                        "extension".into(),
                        Json::Object(Map::from_iter([
                            ("id".into(), Json::String(extension_id.clone())),
                            ("name".into(), Json::String(extension_name.clone())),
                        ])),
                    );
                    json_to_lua(lua, &Json::Object(map), 0)
                })?,
            )?;
        }
        {
            host.set(
                "args",
                self.lua
                    .create_function(|lua, _: ()| lua.globals().get::<Value>("RCHAMBER_ARGS"))?,
            )?;
        }
        {
            host.set(
                "log",
                self.lua.create_function(move |_, message: String| {
                    tracing::info!(target: "rchamber_ext", "{message}");
                    Ok(())
                })?,
            )?;
        }

        self.install_storage(&host, Rc::clone(&policy))?;
        self.install_http(&host, Rc::clone(&policy))?;
        self.install_files(&host, Rc::clone(&policy))?;
        self.install_sessions(&host, Rc::clone(&policy))?;
        self.install_generate(&host, Rc::clone(&policy))?;
        self.install_exec(&host, Rc::clone(&policy))?;

        self.lua.globals().set("host", host)?;
        Ok(())
    }

    fn install_storage(
        &mut self,
        host: &Table,
        policy: Rc<PolicyView>,
    ) -> Result<(), RuntimeError> {
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "storage_get",
            self.lua.create_function(move |lua, key: String| {
                required.require("storage")?;
                let value = bridge.borrow().storage_get(&key).unwrap_or(Json::Null);
                json_to_lua(lua, &value, 0)
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "storage_set",
            self.lua
                .create_function(move |_, (key, value): (String, Value)| {
                    required.require("storage")?;
                    let json = lua_to_json(value, 0);
                    bridge
                        .borrow_mut()
                        .storage_set(&key, json)
                        .map_err(mlua::Error::RuntimeError)
                })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "storage_delete",
            self.lua.create_function(move |_, key: String| {
                required.require("storage")?;
                bridge
                    .borrow_mut()
                    .storage_delete(&key)
                    .map_err(mlua::Error::RuntimeError)
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "storage_keys",
            self.lua.create_function(move |lua, _: ()| {
                required.require("storage")?;
                let keys = bridge.borrow().storage_keys();
                json_to_lua(lua, &serde_json::to_value(keys).unwrap_or(Json::Null), 0)
            })?,
        )?;
        Ok(())
    }

    fn install_http(&mut self, host: &Table, policy: Rc<PolicyView>) -> Result<(), RuntimeError> {
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "http",
            self.lua.create_function(move |lua, value: Table| {
                required.require("network")?;
                let method = value
                    .get::<Option<String>>("method")?
                    .unwrap_or_else(|| "GET".into());
                let url = value.get::<String>("url")?;
                if !policy.allows_origin(&url) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "the origin of {url} is not approved"
                    )));
                }
                let headers = value
                    .get::<Option<Table>>("headers")?
                    .map(|table| {
                        table
                            .pairs::<String, String>()
                            .filter_map(Result::ok)
                            .collect()
                    })
                    .unwrap_or_default();
                let body = value.get::<Option<String>>("body")?;
                let answer = bridge
                    .borrow_mut()
                    .call(HostCall::Http {
                        method,
                        url,
                        headers,
                        body,
                    })
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &answer, 0)
            })?,
        )?;
        Ok(())
    }

    fn install_files(&mut self, host: &Table, policy: Rc<PolicyView>) -> Result<(), RuntimeError> {
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "read_file",
            self.lua.create_function(move |_, path: String| {
                required.require("files")?;
                let value = bridge
                    .borrow_mut()
                    .call(HostCall::ReadFile { path })
                    .map_err(mlua::Error::RuntimeError)?;
                value
                    .get("content")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| mlua::Error::RuntimeError("could not read the file".into()))
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "write_file",
            self.lua
                .create_function(move |_, (path, content): (String, String)| {
                    required.require("files")?;
                    bridge
                        .borrow_mut()
                        .call(HostCall::WriteFile { path, content })
                        .map_err(mlua::Error::RuntimeError)?;
                    Ok(())
                })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "list_dir",
            self.lua.create_function(move |lua, path: String| {
                required.require("files")?;
                let value = bridge
                    .borrow_mut()
                    .call(HostCall::ListDir { path })
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &value, 0)
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "stat",
            self.lua.create_function(move |lua, path: String| {
                required.require("files")?;
                let value = bridge
                    .borrow_mut()
                    .call(HostCall::Stat { path })
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &value, 0)
            })?,
        )?;
        Ok(())
    }

    fn install_sessions(
        &mut self,
        host: &Table,
        policy: Rc<PolicyView>,
    ) -> Result<(), RuntimeError> {
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "sessions",
            self.lua.create_function(move |lua, _: ()| {
                required.require("sessions")?;
                let value = bridge
                    .borrow_mut()
                    .call(HostCall::SessionsList)
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &value, 0)
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "start_session",
            self.lua.create_function(move |lua, value: Option<Table>| {
                required.require("sessions")?;
                let project = value
                    .as_ref()
                    .map(|table| table.get::<Option<String>>("project"))
                    .transpose()?
                    .flatten();
                let title = value
                    .as_ref()
                    .map(|table| table.get::<Option<String>>("title"))
                    .transpose()?
                    .flatten();
                let answer = bridge
                    .borrow_mut()
                    .call(HostCall::SessionStart { project, title })
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &answer, 0)
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "open_session",
            self.lua.create_function(move |_, id: String| {
                required.require("sessions")?;
                bridge
                    .borrow_mut()
                    .call(HostCall::SessionOpen { id })
                    .map_err(mlua::Error::RuntimeError)?;
                Ok(())
            })?,
        )?;
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "prompt_session",
            self.lua.create_function(move |_, value: Table| {
                required.require("prompt")?;
                let session = value.get::<Option<String>>("session")?;
                let text = value.get::<String>("text")?;
                bridge
                    .borrow_mut()
                    .call(HostCall::Prompt { session, text })
                    .map_err(mlua::Error::RuntimeError)?;
                Ok(())
            })?,
        )?;
        Ok(())
    }

    fn install_generate(
        &mut self,
        host: &Table,
        policy: Rc<PolicyView>,
    ) -> Result<(), RuntimeError> {
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "generate",
            self.lua.create_function(move |_, value: Table| {
                required.require("model")?;
                let prompt = value.get::<String>("prompt")?;
                let system = value.get::<Option<String>>("system")?;
                let answer = bridge
                    .borrow_mut()
                    .call(HostCall::Generate { prompt, system })
                    .map_err(mlua::Error::RuntimeError)?;
                answer
                    .get("text")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| mlua::Error::RuntimeError("the model returned no text".into()))
            })?,
        )?;
        Ok(())
    }

    fn install_exec(&mut self, host: &Table, policy: Rc<PolicyView>) -> Result<(), RuntimeError> {
        let bridge = self.bridge.clone();
        let required = Rc::clone(&policy);
        host.set(
            "exec",
            self.lua.create_function(move |lua, value: Table| {
                required.require("exec")?;
                let command = value.get::<String>("command")?;
                let args = value
                    .get::<Option<Table>>("args")?
                    .map(|table| {
                        table
                            .sequence_values::<String>()
                            .filter_map(Result::ok)
                            .collect()
                    })
                    .unwrap_or_default();
                let cwd = value.get::<Option<String>>("cwd")?;
                let answer = bridge
                    .borrow_mut()
                    .call(HostCall::Exec { command, args, cwd })
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &answer, 0)
            })?,
        )?;
        Ok(())
    }
}

struct PolicyView {
    granted: BTreeSet<String>,
    origins: Vec<String>,
}

impl PolicyView {
    fn require(&self, capability: &str) -> mlua::Result<()> {
        if self.granted.contains(capability) {
            Ok(())
        } else {
            Err(mlua::Error::RuntimeError(format!(
                "capability \"{capability}\" was not approved"
            )))
        }
    }

    fn allows_origin(&self, url: &str) -> bool {
        let Some(scheme_end) = url.find("://") else {
            return false;
        };
        let rest = &url[scheme_end + 3..];
        let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let origin = format!("{}{}", &url[..scheme_end + 3], &rest[..host_end])
            .trim_end_matches('/')
            .to_string();
        self.origins
            .iter()
            .any(|allowed| allowed.trim_end_matches('/') == origin)
    }
}

fn json_to_lua(lua: &Lua, value: &Json, depth: usize) -> mlua::Result<Value> {
    if depth > 64 {
        return Ok(Value::Nil);
    }
    match value {
        Json::Null => Ok(Value::Nil),
        Json::Bool(value) => Ok(Value::Boolean(*value)),
        Json::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(Value::Integer(value))
            } else if let Some(value) = number.as_u64() {
                match i64::try_from(value) {
                    Ok(value) => Ok(Value::Integer(value)),
                    Err(_) => Ok(Value::Number(value as f64)),
                }
            } else {
                Ok(Value::Number(number.as_f64().unwrap_or_default()))
            }
        }
        Json::String(value) => Ok(Value::String(lua.create_string(value)?)),
        Json::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, json_to_lua(lua, item, depth + 1)?)?;
            }
            Ok(Value::Table(table))
        }
        Json::Object(items) => {
            let table = lua.create_table_with_capacity(0, items.len())?;
            for (key, item) in items {
                table.raw_set(key.as_str(), json_to_lua(lua, item, depth + 1)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

fn lua_to_json(value: Value, depth: usize) -> Json {
    let mut budget = 50_000usize;
    lua_to_json_budget(value, depth, &mut budget)
}

fn lua_to_json_budget(value: Value, depth: usize, budget: &mut usize) -> Json {
    if depth > 32 || *budget == 0 {
        return Json::Null;
    }
    *budget -= 1;
    match value {
        Value::Nil => Json::Null,
        Value::Boolean(value) => Json::Bool(value),
        Value::Integer(value) => Json::Number(Number::from(value)),
        Value::Number(value) => Number::from_f64(value)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        Value::String(value) => {
            Json::String(String::from_utf8_lossy(&value.as_bytes()).to_string())
        }
        Value::Table(table) => {
            let mut pairs: Vec<(Value, Value)> = Vec::new();
            for pair in table.pairs::<Value, Value>() {
                let Ok(pair) = pair else {
                    continue;
                };
                pairs.push(pair);
                if pairs.len() >= 50_000 {
                    break;
                }
            }
            let sequence = !pairs.is_empty()
                && pairs.iter().all(|(key, _)| match key {
                    Value::Integer(index) => *index >= 1 && (*index as usize) <= pairs.len(),
                    _ => false,
                })
                && {
                    let mut indices: Vec<i64> = pairs
                        .iter()
                        .filter_map(|(key, _)| match key {
                            Value::Integer(index) => Some(*index),
                            _ => None,
                        })
                        .collect();
                    indices.sort_unstable();
                    indices
                        .iter()
                        .enumerate()
                        .all(|(position, index)| *index as usize == position + 1)
                };
            if sequence {
                let mut entries: Vec<(i64, Value)> = pairs
                    .into_iter()
                    .filter_map(|(key, value)| match key {
                        Value::Integer(index) => Some((index, value)),
                        _ => None,
                    })
                    .collect();
                entries.sort_by_key(|(index, _)| *index);
                return Json::Array(
                    entries
                        .into_iter()
                        .map(|(_, value)| lua_to_json_budget(value, depth + 1, budget))
                        .collect(),
                );
            }
            if pairs.is_empty() {
                return Json::Array(Vec::new());
            }
            let mut object = Map::new();
            for (key, value) in pairs {
                let key = match key {
                    Value::String(key) => String::from_utf8_lossy(&key.as_bytes()).to_string(),
                    Value::Integer(key) => key.to_string(),
                    Value::Number(key) => key.to_string(),
                    _ => continue,
                };
                object.insert(key, lua_to_json_budget(value, depth + 1, budget));
            }
            Json::Object(object)
        }
        Value::Error(error) => Json::String(error.to_string()),
        _ => Json::Null,
    }
}

pub fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

pub fn timeout_duration() -> Duration {
    Duration::from_millis(DEFAULT_TIMEOUT_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_nulls_reach_lua_as_nil() {
        let lua = Lua::new();
        let value = serde_json::json!({
            "project": null,
            "session": { "title": "Hello", "directory": null },
            "items": [null, 1, "two"],
        });
        let converted = json_to_lua(&lua, &value, 0).unwrap();
        let Value::Table(table) = converted else {
            panic!("expected a table");
        };
        assert!(matches!(
            table.raw_get::<Value>("project").unwrap(),
            Value::Nil
        ));
        let Value::Table(session) = table.raw_get::<Value>("session").unwrap() else {
            panic!("expected the session table");
        };
        assert_eq!(session.raw_get::<String>("title").unwrap(), "Hello");
        assert!(matches!(
            session.raw_get::<Value>("directory").unwrap(),
            Value::Nil
        ));
        let Value::Table(items) = table.raw_get::<Value>("items").unwrap() else {
            panic!("expected the items table");
        };
        assert!(matches!(items.raw_get::<Value>(1).unwrap(), Value::Nil));
        assert!(matches!(
            items.raw_get::<Value>(2).unwrap(),
            Value::Integer(1)
        ));
    }

    #[test]
    fn mixed_tables_keep_string_keys() {
        let lua = Lua::new();
        let value = lua
            .load("return { [1] = 'a', extra = 1 }")
            .eval::<Value>()
            .unwrap();
        let json = lua_to_json(value, 0);
        assert_eq!(json["1"], "a");
        assert_eq!(json["extra"], 1);
    }

    #[test]
    fn json_tables_round_trip_through_lua() {
        let lua = Lua::new();
        let original = serde_json::json!({
            "empty": [],
            "name": "deploy",
            "nested": { "count": 2, "ok": true },
        });
        let converted = json_to_lua(&lua, &original, 0).unwrap();
        let round_trip = lua_to_json(converted, 0);
        assert_eq!(round_trip, original);
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;

    struct Stub;

    impl HostBridge for Stub {
        fn call(&mut self, _call: HostCall) -> Result<Json, String> {
            Ok(Json::Null)
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

    fn manifest() -> Manifest {
        serde_json::from_value(serde_json::json!({
            "id": "test",
            "name": "Test",
            "version": "1.0.0"
        }))
        .unwrap()
    }

    fn runtime(grants: &[String]) -> LuaRuntime {
        LuaRuntime::new(
            &manifest(),
            grants,
            HostContext {
                theme: "dark".into(),
                locale: "en".into(),
                ..HostContext::default()
            },
            Box::new(Stub),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }

    #[test]
    fn an_empty_session_is_nil_in_lua() {
        let mut runtime = runtime(&[]);
        runtime
            .load_source(
                "function panel() local ctx = host.context() return { { kind = 'text', markdown = ctx.session and ctx.session.title or 'none' } } end",
                "main.lua",
            )
            .unwrap();
        let value = runtime.call_multi("panel", Vec::new()).unwrap().unwrap();
        assert_eq!(value[0]["markdown"], "none");
    }

    #[test]
    fn a_denied_capability_stops_the_call() {
        let mut runtime = runtime(&[]);
        runtime
            .load_source(
                "function panel() return host.http({ url = 'https://example.com' }) end",
                "main.lua",
            )
            .unwrap();
        let error = runtime.call_multi("panel", Vec::new()).unwrap_err();
        assert!(error.to_string().contains("was not approved"), "{error}");
    }

    #[test]
    fn storage_requires_its_capability() {
        let mut runtime = runtime(&[]);
        runtime
            .load_source(
                "function panel() host.storage_set('key', 'value') end",
                "main.lua",
            )
            .unwrap();
        let error = runtime.call_multi("panel", Vec::new()).unwrap_err();
        assert!(error.to_string().contains("storage"), "{error}");
    }

    #[test]
    fn a_runaway_call_times_out() {
        let mut runtime = runtime(&[]);
        runtime.policy.timeout_ms = 50;
        runtime
            .load_source("function panel() while true do end end", "main.lua")
            .unwrap();
        let error = runtime.call_multi("panel", Vec::new()).unwrap_err();
        assert!(matches!(error, RuntimeError::Timeout), "{error}");
    }
}
