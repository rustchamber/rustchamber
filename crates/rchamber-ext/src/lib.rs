pub mod check;
pub mod link;
pub mod manifest;
pub mod openui;
pub mod registry;
pub mod runtime;
pub mod skill;
pub mod storage;
pub mod widget;
pub mod worker;

pub use check::{CheckReport, check_package};
pub use link::{InstallLink, parse_install_link, parse_session_link};
pub use manifest::{
    API_VERSION, ActionContribution, BlockContribution, CommandContribution, Contributes,
    FileEditorContribution, Integration, Manifest, ManifestError, Mentions, PanelContribution,
    Runtime, RuntimeKind, StatusSection, ToolRenderer,
};
pub use openui::{ParseError, parse as parse_openui};
pub use registry::{InstallSource, InstalledExtension, Registry, RegistryError, default_root};
pub use runtime::{
    HostBridge, HostCall, HostContext, HostEffect, LuaRuntime, RuntimeError, SessionContext,
};
pub use skill::{AUTHORING_SKILL, materialize_authoring_skill};
pub use storage::{ExtensionStorage, StorageError};
pub use widget::{
    BlockResponse, ButtonVariant, Cell, ChoiceOption, Direction, KeyValueItem, ListItem, Series,
    StepItem, StepStatus, TabItem, Tone, Widget,
};
pub use worker::{
    WorkerEvent, WorkerHandle, WorkerRequest, spawn as spawn_worker, spawn_with_events,
};
