use rchamber_api::models::Message;
use rchamber_api::{Client, ClientConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = std::env::var("OC_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:4777".into());
    let password = std::env::var("OC_PASSWORD").unwrap_or_else(|_| "devtest".into());
    let mut config = ClientConfig::new(endpoint)?;
    config.password = Some(password);
    config.directory = std::env::var("OC_DIR").ok();
    let client = Client::new(config)?;
    let info = client.info().await?;
    println!("server {} pid {}", info.version, info.pid);
    let sessions = client.list_sessions(5).await?;
    println!("sessions: {}", sessions.len());
    if let Some(session) = sessions.first() {
        println!("first: {} {:?}", session.id, session.title);
        let page = client.messages_page(&session.id, 100).await?;
        println!("messages: {} cursor {:?}", page.data.len(), page.cursor);
        for message in &page.data {
            match message {
                Message::Unknown => println!("unknown message"),
                other => println!("{:?} {:?}", other.id(), message_kind(other)),
            }
        }
        if let Some(first_assistant) = page.data.iter().find_map(|m| match m {
            Message::Assistant(a) => Some(a),
            _ => None,
        }) {
            println!("assistant parts: {}", first_assistant.content.len());
            for part in first_assistant.content.iter().take(4) {
                println!("  part: {part:?}");
            }
        }
    }
    let models = client.models().await?;
    println!("models: {}", models.len());
    let agents = client.agents().await?;
    println!("agents: {}", agents.len());
    Ok(())
}

fn message_kind(message: &Message) -> &'static str {
    match message {
        Message::User(_) => "user",
        Message::Assistant(_) => "assistant",
        Message::Synthetic(_) => "synthetic",
        Message::System(_) => "system",
        Message::Shell(_) => "shell",
        Message::Skill(_) => "skill",
        Message::Compaction(_) => "compaction",
        Message::Idle(_) => "idle",
        Message::AgentSwitched(_) => "agent",
        Message::ModelSwitched(_) => "model",
        Message::LocationSwitched(_) => "location",
        Message::Unknown => "unknown",
    }
}
