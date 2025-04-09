# webstreamer

write interactive streams on twitch using html/js/react/whatever

## how it works

1. launches headless chrome with an extension loaded
2. extension captures video and audio from chrome
3. streams the captured media to twitch using ffmpeg

site also has access to all twitch events from the stream to react to things

## getting started

1. set environment variables:
   - `TWITCH_CLIENT_ID`: your twitch api client id
   - `TWITCH_RMTP_URL`: your twitch ingest server rtmp url
   - `WEBSITE`: the url to capture and stream
2. cargo run

## requirements

- rust toolchain
- ffmpeg
- chrome/chromium browser installed

## features

- captures browser audio and video
- headless operation
- automatic twitch authentication and connection
- realtime twitch chat events forwarded to the browser
