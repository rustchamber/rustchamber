use rchamber_api::models::{ModelRef, PermissionRequest, StructuredError, ToolContent};
use rchamber_api::{RawEvent, SystemEvent};
use serde_json::Value;

use crate::action::{ExecutionState, LiveEvent};

pub fn read(event: &RawEvent) -> LiveEvent {
    if let Some(system) = event.system {
        return match system {
            SystemEvent::Connected => LiveEvent::TransportConnected,
            SystemEvent::Disconnected => LiveEvent::TransportDisconnected(match &event.data {
                Value::String(text) => text.clone(),
                _ => "connection lost".into(),
            }),
        };
    }
    let data = &event.data;
    match event.kind.as_str() {
        "server.connected" => LiveEvent::TransportConnected,
        "session.created" | "session.moved" | "session.archived" | "session.unarchived"
        | "session.forked" | "session.deleted" => {
            if event.kind == "session.deleted" {
                LiveEvent::SessionDeleted {
                    session_id: string(data, "sessionID").unwrap_or_default(),
                }
            } else {
                LiveEvent::SessionsDirty
            }
        }
        "session.renamed" => LiveEvent::SessionRenamed {
            session_id: string(data, "sessionID").unwrap_or_default(),
            title: string(data, "title").unwrap_or_default(),
        },
        "session.viewed" => LiveEvent::SessionViewed {
            session_id: string(data, "sessionID").unwrap_or_default(),
            idle: number(data, "idle").unwrap_or(0.0),
        },
        "session.metadata.updated" => LiveEvent::SessionsDirty,
        "session.model.selected" => match serde_json::from_value::<ModelRef>(
            data.get("model").cloned().unwrap_or(Value::Null),
        ) {
            Ok(model) => LiveEvent::SessionModel {
                session_id: string(data, "sessionID").unwrap_or_default(),
                model,
            },
            Err(_) => LiveEvent::Unknown,
        },
        "session.agent.selected" => LiveEvent::SessionAgent {
            session_id: string(data, "sessionID").unwrap_or_default(),
            agent: string(data, "agent").unwrap_or_default(),
        },
        "session.execution.started" => LiveEvent::Execution {
            session_id: string(data, "sessionID").unwrap_or_default(),
            state: ExecutionState::Started,
            error: None,
        },
        "session.execution.succeeded" => LiveEvent::Execution {
            session_id: string(data, "sessionID").unwrap_or_default(),
            state: ExecutionState::Succeeded,
            error: None,
        },
        "session.execution.failed" => LiveEvent::Execution {
            session_id: string(data, "sessionID").unwrap_or_default(),
            state: ExecutionState::Failed,
            error: data.get("error").and_then(parse_error),
        },
        "session.execution.interrupted" => LiveEvent::Execution {
            session_id: string(data, "sessionID").unwrap_or_default(),
            state: ExecutionState::Interrupted,
            error: None,
        },
        "session.step.started" => LiveEvent::StepStarted {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            agent: string(data, "agent"),
            model: data
                .get("model")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
        },
        "session.step.ended" => LiveEvent::StepEnded {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            cost: number(data, "cost").unwrap_or(0.0),
            tokens: data
                .get("tokens")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            finish: string(data, "finish"),
        },
        "session.step.failed" => LiveEvent::StepFailed {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            error: data.get("error").and_then(parse_error),
        },
        "session.text.delta" => LiveEvent::TextDelta {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            ordinal: number(data, "ordinal").unwrap_or(0.0) as usize,
            delta: string(data, "delta").unwrap_or_default(),
        },
        "session.text.ended" => LiveEvent::TextEnded {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            ordinal: number(data, "ordinal").unwrap_or(0.0) as usize,
            text: string(data, "text").unwrap_or_default(),
        },
        "session.reasoning.delta" => LiveEvent::ReasoningDelta {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            ordinal: number(data, "ordinal").unwrap_or(0.0) as usize,
            delta: string(data, "delta").unwrap_or_default(),
        },
        "session.reasoning.ended" => LiveEvent::ReasoningEnded {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            ordinal: number(data, "ordinal").unwrap_or(0.0) as usize,
            text: string(data, "text").unwrap_or_default(),
        },
        "session.tool.input.started" => LiveEvent::ToolInputStarted {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            name: string(data, "name").unwrap_or_default(),
        },
        "session.tool.input.delta" => LiveEvent::ToolInputDelta {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            delta: string(data, "delta").unwrap_or_default(),
        },
        "session.tool.input.ended" => LiveEvent::ToolInputEnded {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            text: string(data, "text").unwrap_or_default(),
        },
        "session.tool.called" => LiveEvent::ToolCalled {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            name: string(data, "name").unwrap_or_default(),
            input: data.get("input").cloned().unwrap_or(Value::Null),
            executed: data
                .get("executed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "session.tool.progress" => LiveEvent::ToolProgress {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            metadata: data.get("metadata").cloned().unwrap_or(Value::Null),
        },
        "session.tool.success" => LiveEvent::ToolSuccess {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            content: content(data),
            executed: data
                .get("executed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "session.tool.failed" => LiveEvent::ToolFailed {
            session_id: string(data, "sessionID").unwrap_or_default(),
            message_id: string(data, "assistantMessageID").unwrap_or_default(),
            tool_id: string(data, "id").unwrap_or_default(),
            error: data.get("error").and_then(parse_error),
            content: content(data),
        },
        "permission.asked" => match serde_json::from_value::<PermissionRequest>(data.clone()) {
            Ok(request) => LiveEvent::PermissionAsked(request),
            Err(err) => {
                tracing::warn!(%err, "could not parse permission request");
                LiveEvent::Unknown
            }
        },
        "permission.replied" => LiveEvent::PermissionReplied {
            session_id: string(data, "sessionID").unwrap_or_default(),
            request_id: string(data, "requestID").unwrap_or_default(),
        },
        "session.inbox.delivered" => LiveEvent::InboxDelivered {
            session_id: string(data, "sessionID").unwrap_or_default(),
        },
        "session.inbox.enqueued" | "session.inbox.cancelled" | "session.inbox.delivery.changed" => {
            LiveEvent::MessagesDirty {
                session_id: string(data, "sessionID").unwrap_or_default(),
            }
        }
        "session.synthetic" | "session.shell.started" | "session.shell.ended" => {
            LiveEvent::MessagesDirty {
                session_id: string(data, "sessionID").unwrap_or_default(),
            }
        }
        "session.compaction.ended" | "session.compaction.failed" | "session.revert.committed" => {
            LiveEvent::MessagesDirty {
                session_id: string(data, "sessionID").unwrap_or_default(),
            }
        }
        _ => LiveEvent::Unknown,
    }
}

fn string(data: &Value, key: &str) -> Option<String> {
    data.get(key).and_then(Value::as_str).map(str::to_string)
}

fn number(data: &Value, key: &str) -> Option<f64> {
    data.get(key).and_then(Value::as_f64)
}

fn parse_error(value: &Value) -> Option<StructuredError> {
    serde_json::from_value(value.clone()).ok().or_else(|| {
        value.as_str().map(|message| StructuredError {
            kind: "error".into(),
            message: message.to_string(),
            status: None,
            retry_after_ms: None,
        })
    })
}

fn content(data: &Value) -> Vec<ToolContent> {
    data.get("content")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ToolContent>>(value).ok())
        .unwrap_or_default()
}
