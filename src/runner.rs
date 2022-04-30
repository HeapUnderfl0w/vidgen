use std::{
	path::PathBuf,
	process::Stdio,
};

use anyhow::Context;
use futures::{StreamExt};
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::{
	fs::{self, File},
	io::BufReader,
	process::{Child, Command},
	sync::mpsc::{channel, Receiver, Sender},
	task::JoinHandle,
};
use tokio_stream::wrappers::ReadDirStream;

macro_rules! snd_chk {
	($chs:expr) => {
		if let Err(_) = $chs {
			return Ok(());
		}
	};
}

pub static NAME_REGEX: Lazy<Regex> =
	Lazy::new(|| Regex::new(r"(\d+)\.\w{3,4}").expect("compiled regex is invalid"));

#[derive(Debug)]
pub struct Runner {
	child:  Child,
	notify: Sender<Message>,
	source: PathBuf,
}

pub struct Frame(pub u64, pub PathBuf);

impl Runner {
	pub fn start(mut command: Command, source: PathBuf) -> anyhow::Result<RunnerHandle> {
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
			source,
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
		let frames: Vec<Frame> = ReadDirStream::new(
			tokio::fs::read_dir(&self.source)
				.await
				.context("failed to list files in source directory")?,
		)
		.filter_map(|v| async { v.ok() })
		.filter_map(Self::filter_item)
		.collect()
		.await;

		let ts_start = time::Instant::now();
		snd_chk!(
			self.notify
				.send(Message::Start {
					frames: frames.len() as u64,
				})
				.await
		);

		for frame in frames {
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

		snd_chk!(
			self.notify
				.send(Message::Stop {
					time: ts_start.elapsed(),
				})
				.await
		);
		Ok(())
	}

	pub async fn filter_item(entry: tokio::fs::DirEntry) -> Option<Frame> {
		let fname = entry.file_name().to_str()?.to_owned();
		let m = NAME_REGEX.captures(&fname)?;
		let fid = m.get(1)?.as_str().parse().ok()?;
		Some(Frame(fid, entry.path()))
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
