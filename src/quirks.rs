use crate::runner::Message;
use anyhow::Context;
use std::{
    mem,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::oneshot, task::JoinHandle};

pub struct KeysightQuirks {
    path:    PathBuf,
    kill:    oneshot::Receiver<()>,
    message: Arc<Mutex<Option<Message>>>,
    total:   u64,
}

impl KeysightQuirks {
    pub fn start(source_path: PathBuf, total: u64) -> KeysightQuirksHandle {
        let path = source_path.join("_progress.json");

        let (kill_tx, kill_rx) = oneshot::channel();
        let last_message = Arc::new(Mutex::new(None));
        let handle_msg = Arc::clone(&last_message);
        let quirks = KeysightQuirks {
            path,
            kill: kill_rx,
            message: handle_msg,
            total,
        };
        let handle = tokio::spawn(quirks.run());

        KeysightQuirksHandle {
            task:   handle,
            events: last_message,
            kill:   kill_tx,
        }
    }

    async fn run(mut self) -> anyhow::Result<()> {
        let mut frames = 0;
        loop {
            tokio::select! {
                biased;
                _ = &mut self.kill => {
                    self.write_progress(frames, self.total, String::new(), true).await?;
                    break;
                },
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }

            let msg = mem::take(&mut *self.message.lock().unwrap());
            let (fid, pth) = match msg {
                Some(Message::Frame { fid, path }) => (fid, path),
                _ => continue,
            };

            frames = fid;

            self.write_progress(frames, self.total, pth, false).await?;
        }
        Ok(())
    }

    async fn write_progress(&self, f: u64, t: u64, p: String, d: bool) -> anyhow::Result<()> {
        let json = serde_json::to_string(&ProgressFile {
            frames: f,
            total:  t,
            path:   p,
            done:   d,
        })
        .context("failed to serialize json")?;

        tokio::fs::write(&self.path, json.as_bytes())
            .await
            .context("failed to write progress file")
    }
}

pub struct KeysightQuirksHandle {
    task:   JoinHandle<anyhow::Result<()>>,
    events: Arc<Mutex<Option<Message>>>,
    kill:   oneshot::Sender<()>,
}

impl KeysightQuirksHandle {
    pub fn push_msg(&self, msg: Message) { *self.events.lock().unwrap() = Some(msg); }

    pub async fn stop(self) -> anyhow::Result<()> {
        let _ = self.kill.send(());
        self.task
            .await
            .context("failed to wait for quirks task")?
            .context("failed to wait for quirks task")
    }
}

#[derive(Debug, serde::Serialize)]
struct ProgressFile {
    frames: u64,
    total:  u64,
    path:   String,
    done:   bool,
}
