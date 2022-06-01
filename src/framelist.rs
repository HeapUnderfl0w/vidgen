use anyhow::Context;
use futures::StreamExt;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;
use tokio_stream::wrappers::ReadDirStream;

pub static NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d+)\.\w{3,4}").expect("compiled regex is invalid"));

#[derive(Debug, Eq, PartialEq, Ord)]
pub struct Frame(pub u64, pub PathBuf);

impl PartialOrd for Frame {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[derive(Debug)]
pub struct FrameList {
    pub frames: Vec<Frame>,
}

impl FrameList {
    pub async fn from_dir(dir: &str) -> anyhow::Result<Self> {
        let mut frames: Vec<Frame> = ReadDirStream::new(
            tokio::fs::read_dir(dir)
                .await
                .context("failed to list files in source directory")?,
        )
        .filter_map(|v| async { v.ok() })
        .filter_map(Self::filter_item)
        .collect()
        .await;

        frames.sort();

        Ok(FrameList { frames })
    }

    pub async fn filter_item(entry: tokio::fs::DirEntry) -> Option<Frame> {
        let fname = entry.file_name().to_str()?.to_owned();
        let m = NAME_REGEX.captures(&fname)?;
        let fid = m.get(1)?.as_str().parse().ok()?;
        Some(Frame(fid, entry.path()))
    }
}
