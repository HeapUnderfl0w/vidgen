use anyhow::Context;
use std::{
	path::{Path, PathBuf},
	process::Stdio,
};
use tokio::process::Command;

#[cfg(windows)]
mod ffmpeg_names {
	pub const FFMPEG: &str = "ffmpeg.exe";
	pub const FFPROBE: &str = "ffprobe.exe";
}
#[cfg(not(windows))]
mod ffmpeg_names {
	pub const FFMPEG: &str = "ffmpeg";
	pub const FFPROBE: &str = "ffprobe";
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Ffmpeg {
	path: Option<PathBuf>,
}

impl Ffmpeg {
	pub fn new() -> Self { Ffmpeg { path: None } }

	pub fn new_with_path(p: String) -> Self {
		Ffmpeg {
			path: Some(PathBuf::from(p)),
		}
	}

	fn path_for_name(&self, n: &str) -> PathBuf {
		match &self.path {
			Some(path) => path.join(n),
			None => PathBuf::from(n),
		}
	}

	pub fn ffprobe(&self) -> PathBuf { self.path_for_name(ffmpeg_names::FFPROBE) }

	pub fn ffmpeg(&self) -> PathBuf { self.path_for_name(ffmpeg_names::FFMPEG) }
}

pub async fn ensure_ffmpeg_dir(dir: Option<String>) -> anyhow::Result<Ffmpeg> {
	if let Some(path) = dir {
		let ffmpeg = Ffmpeg::new_with_path(path.clone());
		if !Path::exists(&ffmpeg.ffmpeg()) {
			anyhow::bail!(
				"you specified the path {} but {} does not exist there",
				path,
				ffmpeg.ffmpeg().display()
			);
		}
		if !Path::exists(&ffmpeg.ffprobe()) {
			anyhow::bail!(
				"you specified the path {} but {} does not exist there",
				path,
				ffmpeg.ffprobe().display()
			)
		}

		Ok(ffmpeg)
	} else {
		let ffmpeg = Ffmpeg::new();

		program_is_callable(&ffmpeg.ffmpeg())
			.await
			.context("cannot find ffmpeg")?;
		program_is_callable(&ffmpeg.ffprobe())
			.await
			.context("cannot find ffprobe")?;

		Ok(ffmpeg)
	}
}

async fn program_is_callable(name: &Path) -> anyhow::Result<()> {
	Command::new(name)
		.arg("-version")
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.with_context(|| format!("failed to call {}, is it in path?", name.display()))?
		.wait()
		.await
		.with_context(|| format!("{} exited with a non-zero error code", name.display()))
		.map(|_| ())
}
