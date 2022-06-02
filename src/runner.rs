use std::process::Stdio;

use anyhow::Context;
use tokio::{
    fs::{self, File},
    io::BufReader,
    process::{Child, Command},
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tracing::Instrument;

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
        info!("starting ffmpeg child");
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

        info!("starting task");
        let handle = tokio::spawn(runner.run());

        Ok(RunnerHandle {
            task:   handle,
            events: notify_rx,
        })
    }

    #[instrument(skip(self), name = "ffmpeg")]
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
                .in_current_span()
                .await
        );

        info!("starting encoding");

        for frame in &self.frames.frames {
            let frame_span = error_span!("frame", id=%frame.0, source=?frame.1.display());
            let result = async {
                snd_chk!(
                    self.notify
                        .send(Message::Frame {
                            fid:  frame.0,
                            path: frame.1.display().to_string(),
                        })
                        .await
                );
                debug!("opening file");
                let mut file = BufReader::new(
                    File::open(&frame.1)
                        .in_current_span()
                        .await
                        .context("failed to open frame")?,
                );

                debug!("copy data");
                tokio::io::copy_buf(&mut file, &mut stdin)
                    .in_current_span()
                    .await
                    .context("failed to stream frame")?;

                debug!("cleaning up");
                fs::remove_file(&frame.1)
                    .in_current_span()
                    .await
                    .context("failed to remove frame")?;

                debug!("cleaned up");

                Ok::<(), anyhow::Error>(())
            }
            .instrument(frame_span)
            .await;

            if let Err(why) = result {
                error!(current_frame=%frame.0, error=%why, "error while reading frame");
                return Err(why);
            }
        }

        drop(stdin);

        info!("waiting for ffmpeg to finish up");
        let _ = self.child.wait().in_current_span().await;

        snd_chk!(
            self.notify
                .send(Message::Stop {
                    time: ts_start.elapsed(),
                })
                .in_current_span()
                .await
        );

        info!("done encoding");
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
