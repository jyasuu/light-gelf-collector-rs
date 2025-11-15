use crate::gelf::{GelfMessage, MessageResponse, StoredMessage};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// Trait for message storage
pub trait MessageStore: Clone + Send + Sync + 'static {
    fn add_message(&self, gelf_message: GelfMessage, raw_message: String) -> impl std::future::Future<Output = ()> + Send;
    fn get_messages(&self, limit: Option<usize>) -> impl std::future::Future<Output = Vec<MessageResponse>> + Send;
    fn get_stats(&self) -> impl std::future::Future<Output = serde_json::Value> + Send;
    fn subscribe(&self) -> broadcast::Receiver<MessageResponse>;
}

/// Trait for broadcasting messages
pub trait MessageBroadcaster: Send + Sync {
    fn broadcast(&self, message: MessageResponse) -> Result<(), broadcast::error::SendError<MessageResponse>>;
    fn subscribe(&self) -> broadcast::Receiver<MessageResponse>;
}

/// Default broadcaster implementation
#[derive(Clone)]
pub struct DefaultBroadcaster {
    tx: broadcast::Sender<MessageResponse>,
}

impl DefaultBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }
}

impl MessageBroadcaster for DefaultBroadcaster {
    fn broadcast(&self, message: MessageResponse) -> Result<(), broadcast::error::SendError<MessageResponse>> {
        self.tx.send(message).map(|_| ())
    }

    fn subscribe(&self) -> broadcast::Receiver<MessageResponse> {
        self.tx.subscribe()
    }
}

/// In-memory message storage implementation
#[derive(Clone)]
pub struct InMemoryMessageStore {
    messages: Arc<RwLock<VecDeque<StoredMessage>>>,
    max_size: usize,
    broadcaster: Arc<dyn MessageBroadcaster + Send + Sync>,
}

impl InMemoryMessageStore {
    pub fn new(max_size: usize) -> Self {
        Self::with_broadcaster(max_size, Arc::new(DefaultBroadcaster::new(100)))
    }

    pub fn with_broadcaster(
        max_size: usize,
        broadcaster: Arc<dyn MessageBroadcaster + Send + Sync>,
    ) -> Self {
        Self {
            messages: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            broadcaster,
        }
    }
}

impl MessageStore for InMemoryMessageStore {
    fn add_message(&self, gelf_message: GelfMessage, raw_message: String) -> impl std::future::Future<Output = ()> + Send {
        let stored_message = StoredMessage::new(gelf_message, raw_message);
        let response = stored_message.to_response();
        let messages = self.messages.clone();
        let max_size = self.max_size;
        let broadcaster = self.broadcaster.clone();

        async move {
            {
                let mut messages_guard = messages.write().await;
                messages_guard.push_back(stored_message);

                // Clean up if we exceed max size
                while messages_guard.len() > max_size {
                    messages_guard.pop_front();
                }
            }

            // Broadcast the new message to subscribers (ignore if no subscribers)
            let _ = broadcaster.broadcast(response);
            debug!("Message added to store and broadcasted");
        }
    }

    fn get_messages(&self, limit: Option<usize>) -> impl std::future::Future<Output = Vec<MessageResponse>> + Send {
        let messages = self.messages.clone();
        async move {
            let messages_guard = messages.read().await;
            let limit = limit.unwrap_or(messages_guard.len());
            
            messages_guard
                .iter()
                .rev()
                .take(limit)
                .map(|stored| stored.to_response())
                .collect()
        }
    }

    fn get_stats(&self) -> impl std::future::Future<Output = serde_json::Value> + Send {
        let messages = self.messages.clone();
        let max_size = self.max_size;
        async move {
            let messages_guard = messages.read().await;
            serde_json::json!({
                "total_messages": messages_guard.len(),
                "max_capacity": max_size,
                "capacity_used_percent": (messages_guard.len() as f64 / max_size as f64) * 100.0
            })
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<MessageResponse> {
        self.broadcaster.subscribe()
    }
}