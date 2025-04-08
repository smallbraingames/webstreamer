use crate::env;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};
use tokio::{spawn, sync::mpsc::Receiver};
use tokio_tungstenite::tungstenite::Bytes;
use tracing::{info, warn};

pub async fn stream(mut stream_rx: Receiver<Bytes>) {
    info!("starting ffmpeg process");
    let twitch_rmtp_url = env!("TWITCH_RMTP_URL");
    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-i",
            "-",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-b:v",
            "3000k",
            "-maxrate",
            "3000k",
            "-bufsize",
            "6000k",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "30",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-ar",
            "44100",
            "-f",
            "flv",
            &twitch_rmtp_url,
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = ffmpeg.stdin.take().unwrap();

    let stderr = ffmpeg.stderr.take().unwrap();

    spawn(async move {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => info!("ffmpeg: {}", line),
                Err(e) => warn!("Error reading ffmpeg stderr: {}", e),
            }
        }
    });

    while let Some(data) = stream_rx.recv().await {
        stdin.write_all(&data).unwrap();
    }
    drop(stdin);
    match ffmpeg.wait() {
        Ok(status) => info!("ffmpeg process exited with status: {}", status),
        Err(e) => warn!("failed to wait for ffmpeg process: {}", e),
    }
}
