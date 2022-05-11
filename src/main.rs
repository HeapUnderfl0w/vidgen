use crate::{framelist::FrameList, runner::Message};
use anyhow::Context;
use clap::Parser;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{path::PathBuf, process::Stdio, str::FromStr};
use tokio::process::Command;
use tokio_stream::wrappers::ReadDirStream;

mod ffmpeg;
mod framelist;
mod quirks;
mod runner;
mod x264;

fn main() {
    let args = Args::parse();
    let wait = args.wait;

    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name(concat!(env!("CARGO_PKG_NAME"), "-worker"))
        .build()
        .expect("runtime failed to initialize")
        .block_on(program(args));

    if let Err(why) = result {
        eprintln!("error: {:?}", why);
        if wait {
            wait_before_exit();
        }
        std::process::exit(1);
    }

    if wait {
        wait_before_exit();
    }
}

fn wait_before_exit() {
    use std::io::{stdin, BufRead, BufReader};

    println!("Press [Enter] to exit.");

    let stdin = stdin();
    let mut read = BufReader::new(stdin.lock());

    let mut _garbage = String::new();
    read.read_line(&mut _garbage)
        .expect("failed to read any input from stdin");
}

/// Encode a pile of frames into a video file.
#[derive(Debug, clap::Parser)]
#[clap(version = env!("CARGO_PKG_VERSION"), name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"))]
struct Args {
    /// The source directory to read frames from
    #[clap()]
    source: String,

    /// The target file to write to. This will truncate by default
    #[clap()]
    target: String,

    /// Dimensions of the frame files
    #[clap(short, long = "input-dim", default_value = "auto")]
    input_dim: String,

    /// Dimensions of the output video
    #[clap(short, long = "output-dim", default_value = "1920x1080")]
    output_dim: String,

    /// Target fps
    #[clap(short, long = "fps", default_value = "60")]
    fps: u16,

    /// Instruct the encoder to use the given constant bitrate
    #[clap(long, parse(try_from_str=x264::Crf::parse))]
    crf: Option<x264::Crf>,

    /// Extra args passed as-is to ffmpeg. They will be included after the default arguments but before the output argument
    #[clap(long)]
    extra_arg: Option<Vec<String>>,

    /// Override the path to the ffmpeg binary directory (it should contain ffmpeg and ffprobe)
    #[clap(long)]
    ffmpeg: Option<String>,

    /// The x264 encoder preset to use
    #[clap(long, arg_enum, default_value = "medium", name = "PRESET")]
    x264_preset: x264::X264Preset,

    /// The x264 encoder tuning to use
    #[clap(long, arg_enum, name = "TUNING")]
    x264_tune: Option<x264::X264Tune>,

    /// Wait for the user to press a button before exiting
    #[clap(short, long)]
    wait: bool,

    /// Switch to keysight quirks mode
    #[clap(long)]
    keysight: bool,

    /// Splice audio into video.
    #[clap(long)]
    audio: Option<AudioOptions>,
}

#[derive(Debug)]
struct AudioOptions {
    start: f64,
    file:  PathBuf,
}

impl FromStr for AudioOptions {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut fsplit = s.split(",");

        let ts = fsplit.next().context("why the fuck?")?;
        let path = fsplit
            .next()
            .context("missing file path for audio file")?
            .parse()
            .context("the given path contains invalid characters")?;

        let start = ts
            .parse()
            .context("the start time needs to be the offset in milliseconds")?;

        Ok(AudioOptions { start, file: path })
    }
}

#[derive(Debug, serde::Deserialize)]
struct StreamData {
    width:  u32,
    height: u32,
}

#[derive(Debug, serde::Deserialize)]
struct FfprobeRes {
    streams: [StreamData; 1],
}

macro_rules! ffarg {
    ($c:ident, $arg:expr) => {{
        (&mut $c).arg($arg);
    }};
    ($c:ident, $arg:expr, $val:expr) => {{
        (&mut $c).arg($arg).arg($val);
    }};
}

const DIM_AUTO: &str = "auto";
async fn program(args: Args) -> anyhow::Result<()> {
    let ffmpeg = ffmpeg::ensure_ffmpeg_dir(args.ffmpeg.clone(), args.input_dim == DIM_AUTO)
        .await
        .context("ffmpeg discovery failed")?;

    let (frame_width, frame_height): (u32, u32) = match args.input_dim.as_str() {
        v if v == DIM_AUTO => {
            let ident_frame = find_ident_frame(&args.source)
                .await
                .context("failed to find ident frame")?;

            let data = Command::new(ffmpeg.ffprobe())
                .args(&[
                    "-v",
                    "quiet",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height",
                    "-of",
                    "json=c=1",
                ])
                .arg(&ident_frame)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .context("failed to spawn ffprobe")?
                .wait_with_output()
                .await
                .context("ffprobe did not succeed")?
                .stdout;

            let fdt: FfprobeRes = serde_json::from_slice(&data)
                .context("failed to parse stream info from ffprobe")?;
            (fdt.streams[0].width, fdt.streams[0].height)
        },
        exact => parse_resolution(exact).context("failed to parse input resolution")?,
    };

    let (target_width, target_height) =
        parse_resolution(&args.output_dim).context("failed to parse output resolution")?;

    let frames = FrameList::from_dir(&args.source)
        .await
        .context("failed to index frames")?;
    let target_duration = (1.0 / args.fps as f64) * frames.frames.len() as f64;

    let mut com = Command::new(ffmpeg.ffmpeg());
    ffarg!(com, "-y");
    ffarg!(com, "-framerate", args.fps.to_string());
    ffarg!(com, "-s", format!("{frame_width}x{frame_height}"));
    ffarg!(com, "-an");
    ffarg!(com, "-f", "image2pipe");
    ffarg!(com, "-i", "-");
    if let Some(audio) = args.audio {
        if audio.start > 0.0 {
            ffarg!(com, "-itsoffset", audio.start.to_string());
        }
        ffarg!(com, "-i", audio.file);
        ffarg!(com, "-af", "apad");
        ffarg!(com, "-map", "0:v");
        ffarg!(com, "-map", "1:a");
        ffarg!(com, "-c:a", "aac");
        ffarg!(com, "-to", target_duration.to_string());
    }
    ffarg!(com, "-c:v", "libx264");
    ffarg!(com, "-pix_fmt", "yuv420p");
    ffarg!(com, "-preset:v", args.x264_preset.to_string());
    ffarg!(
        com,
        "-vf",
        format!("scale={target_width}x{target_height}:flags=bicubic")
    );

    if let Some(tune) = args.x264_tune {
        ffarg!(com, "-tune", tune.to_string());
    }

    if let Some(crf) = args.crf {
        ffarg!(com, "-crf", crf.0.to_string());
    }

    if let Some(extra_args) = args.extra_arg {
        for arg in extra_args {
            let mut p = arg.splitn(2, '=');
            let k = p.next().unwrap();
            let v = p.next();
            match v {
                Some(v) => ffarg!(com, k, v),
                None => ffarg!(com, k),
            }
        }
    }

    ffarg!(com, args.target);
    dbg!(&com);

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        com.creation_flags(CREATE_NO_WINDOW);
    }

    let source_path = PathBuf::from(args.source);

    let mut runner = runner::Runner::start(com, frames).context("failed to start ffmpeg")?;

    let framen = match runner.event().await {
        Some(Message::Start { frames }) => frames,
        _ => anyhow::bail!("somehow missed start message"),
    };

    let pbar = ProgressBar::new(framen).with_style(
        ProgressStyle::default_bar()
            .progress_chars("#$-")
            .template("ETA {eta} | {pos}/{len} [{wide_bar:.light.green/light.blue}] {msg}"),
    );

    let quirks = if args.keysight {
        pbar.println("entering keysight quirks mode");

        Some(quirks::KeysightQuirks::start(source_path, framen))
    } else {
        None
    };

    loop {
        let event = match runner.event().await {
            Some(event) => event,
            None => {
                pbar.finish_at_current_pos();
                break;
            },
        };

        match event {
            Message::Frame { fid, path: _ } => {
                if let Some(q) = quirks.as_ref() {
                    q.push_msg(event)
                }
                pbar.set_message(format!("frame {}", fid));
                pbar.inc(1);
            },
            Message::Stop { time } => {
                pbar.finish_with_message(format!(
                    "done, took {}",
                    indicatif::HumanDuration(std::time::Duration::new(
                        time.whole_seconds() as u64,
                        time.subsec_nanoseconds() as u32
                    ))
                ));
                break;
            },
            Message::Start { frames: _ } => unreachable!("should never appear twice"),
        }
    }

    runner.join().await.context("failed to wait for task")?;
    if let Some(q) = quirks {
        q.stop().await?;
    }

    Ok(())
}

fn parse_resolution(s: &str) -> anyhow::Result<(u32, u32)> {
    let p: Vec<_> = s.split('x').collect();
    if p.len() != 2 {
        anyhow::bail!("the dimension must be specified as `WIDTHxHEIGHT` (example: `1920x1080`)");
    }

    let w = p[0].parse().context("width is not an integer")?;
    let h = p[1].parse().context("height is not an integer")?;
    Ok((w, h))
}

async fn find_ident_frame(path: &str) -> anyhow::Result<String> {
    let frame = ReadDirStream::new(
        tokio::fs::read_dir(path)
            .await
            .context("failed to list files in source directory")?,
    )
    .filter_map(|v| async { v.ok() })
    .filter_map(framelist::FrameList::filter_item)
    .fold(framelist::Frame(u64::MAX, PathBuf::new()), |acc, e| async {
        if e.0 < acc.0 {
            e
        } else {
            acc
        }
    })
    .await;

    if frame.0 == u64::MAX {
        anyhow::bail!("no valid init frame found");
    }

    Ok(frame.1.display().to_string())
}
