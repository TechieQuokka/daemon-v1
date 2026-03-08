use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum DaemonToModule {
    Init {
        module_name: String,
        config: Value,
    },
    Command {
        id: String,
        #[serde(flatten)]
        payload: Value,
    },
    Event {
        topic: String,
        data: Option<Value>,
        publisher: String,
        timestamp: u64,
    },
    Shutdown {
        #[serde(default)]
        force: bool,
        timeout: Option<u64>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ModuleToDaemon {
    Ack { id: String },
    Error { id: String, code: u32, message: Option<String> },
    Publish { topic: String, metadata: Value },
    SubscribeRequest { topic: String },
    Log { message: String, level: Option<String> },
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let msg: DaemonToModule = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to parse message: {}", e);
                continue;
            }
        };

        match msg {
            DaemonToModule::Init { module_name, .. } => {
                // Log initialization
                let log = ModuleToDaemon::Log {
                    message: format!("Echo module '{}' initialized", module_name),
                    level: Some("info".to_string()),
                };
                send_message(&mut stdout, &log)?;

                // Subscribe to echo.* topics
                let sub = ModuleToDaemon::SubscribeRequest {
                    topic: "echo.*".to_string(),
                };
                send_message(&mut stdout, &sub)?;
            }
            DaemonToModule::Command { id, payload } => {
                // Send ACK
                let ack = ModuleToDaemon::Ack { id: id.clone() };
                send_message(&mut stdout, &ack)?;

                // Echo command as event
                let publish = ModuleToDaemon::Publish {
                    topic: "echo.response".to_string(),
                    metadata: serde_json::json!({
                        "original_id": id,
                        "echoed": payload
                    }),
                };
                send_message(&mut stdout, &publish)?;
            }
            DaemonToModule::Event { topic, data, .. } => {
                // Log received event
                let log = ModuleToDaemon::Log {
                    message: format!("Received event on topic: {}", topic),
                    level: Some("debug".to_string()),
                };
                send_message(&mut stdout, &log)?;

                // Echo back with modified topic
                if let Some(data) = data {
                    let new_topic = topic.replace("echo.", "echo.response.");
                    let publish = ModuleToDaemon::Publish {
                        topic: new_topic,
                        metadata: data,
                    };
                    send_message(&mut stdout, &publish)?;
                }
            }
            DaemonToModule::Shutdown { .. } => {
                let log = ModuleToDaemon::Log {
                    message: "Echo module shutting down".to_string(),
                    level: Some("info".to_string()),
                };
                send_message(&mut stdout, &log)?;
                break;
            }
        }
    }

    Ok(())
}

fn send_message<W: Write>(writer: &mut W, msg: &ModuleToDaemon) -> io::Result<()> {
    let json = serde_json::to_string(msg)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}
