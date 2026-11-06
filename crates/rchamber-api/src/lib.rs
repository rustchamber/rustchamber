pub mod client;
pub mod models;
pub mod process;

pub use client::{
    ApiError, Client, ClientConfig, PromptFile, PromptFileSource, PromptMention, RawEvent,
    Subscription, SystemEvent,
};
pub use models::{SessionCursor, SessionPage};
pub use process::{LaunchOptions, ServerHandle, launch};

pub const MAX_PAGE: u32 = 100;
