use std::{path::PathBuf, process::Stdio};

use anyhow::Context;
use tokio::{
    fs::{self, File},
    io::BufReader,
    process::{Child, Command},
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};

use crate::framelist::FrameList;

macro_rules! snd_chk {
    ($chs:expr) => {
        if let Err(_) = $chs {
            return Ok(());
        }
    };
}

#[derive(Debug)]
pub struct Runner {
    child:  Child,
    notify: Sender<Message>,
    frames: FrameList,
}

impl Runner {
    pub fn start(mut command: Command, frames: FrameList) -> anyhow::Result<RunnerHandle> {
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start ffmpeg process")?;
        let (notify_tx, notify_rx) = channel(64);

        let runner = Runner {
            child,
            notify: notify_tx,
            frames,
        };

        let handle = tokio::spawn(runner.run());

        Ok(RunnerHandle {
            task:   handle,
            events: notify_rx,
        })
    }

    async fn run(mut self) -> anyhow::Result<()> {
        let mut stdin = self
            .child
            .stdin
            .take()
            .context("no stdin, is ffmpeg running?")?;

        let ts_start = time::Instant::now();
        snd_chk!(
            self.notify
                .send(Message::Start {
                    frames: self.frames.frames.len() as u64,
                })
                .await
        );

        for frame in &self.frames.frames {
            snd_chk!(
                self.notify
                    .send(Message::Frame {
                        fid:  frame.0,
                        path: frame.1.display().to_string(),
                    })
                    .await
            );
            let mut file =
                BufReader::new(File::open(&frame.1).await.context("failed to open frame")?);
            tokio::io::copy_buf(&mut file, &mut stdin)
                .await
                .context("failed to stream frame")?;

            fs::remove_file(&frame.1)
                .await
                .context("failed to remove frame")?;
        }

        drop(stdin);
        let _ = self.child.wait().await;

        snd_chk!(
            self.notify
                .send(Message::Stop {
                    time: ts_start.elapsed(),
                })
                .await
        );
        Ok(())
    }
}

pub struct RunnerHandle {
    events: Receiver<Message>,
    task:   JoinHandle<anyhow::Result<()>>,
}

impl RunnerHandle {
    pub async fn join(self) -> anyhow::Result<()> { self.task.await.context("await failed")? }

    pub async fn event(&mut self) -> Option<Message> { self.events.recv().await }
}

#[derive(Debug)]
pub enum Message {
    Start { frames: u64 },
    Frame { fid: u64, path: String },
    Stop { time: time::Duration },
}
