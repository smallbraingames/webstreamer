console.log("read extension started");

window.addEventListener("message", async (event) => {
  if (event.source !== window) return;
  if (event.data.type === "CAPTURE_COMMAND") {
    if (event.data.command === "open-popup") {
      chrome.runtime.sendMessage({
        command: "open-popup",
      });
    }
    if (event.data.command === "start") {
      const client = new WebSocket(`ws://localhost:${event.data.port}`, []);

      await new Promise((resolve) => {
        if (client.readyState === WebSocket.OPEN) resolve();
        client.addEventListener("open", () => resolve());
      });

      client.send("hello from extension");

      const streamIdPromise = new Promise((resolve) => {
        const messageListener = (message) => {
          console.log("got message in strema id promise listenenr", message);
          if (message.command === "stream-id") {
            chrome.runtime.onMessage.removeListener(messageListener);
            resolve(message.streamId);
          }
        };
        chrome.runtime.onMessage.addListener(messageListener);
      });

      chrome.runtime.sendMessage({
        command: "get-stream-id",
      });

      const streamId = await streamIdPromise;
      console.log("got stream id", streamId);

      const stream = await navigator.mediaDevices.getUserMedia({
        video: true,
        audio: true,
        audio: {
          mandatory: {
            chromeMediaSource: "tab",
            chromeMediaSourceId: streamId,
          },
        },
        video: {
          mandatory: {
            chromeMediaSource: "tab",
            chromeMediaSourceId: streamId,
          },
        },
      });

      console.log("got stream", stream);

      const recorder = new MediaRecorder(stream, {
        audioBitsPerSecond: 128_000,
        videoBitsPerSecond: 2_500_000,
        mimeType: "video/webm",
      });

      console.log("got recorder", recorder);

      recorder.ondataavailable = async (e) => {
        if (!e.data.size) return;
        const buffer = await e.data.arrayBuffer();
        console.log(buffer);
        client.send(buffer);
      };

      recorder.onerror = (e) => {
        console.error("mediarecorder error:", e);
        client.send(`mediarecorder error: ${e}`);
        recorder.stop();
      };

      recorder.onstop = function () {
        const tracks = stream.getTracks();
        tracks.forEach(function (track) {
          track.stop();
        });
        if (client.readyState === WebSocket.OPEN) client.close();
      };

      recorder.start(10);

      console.log(recorder.state);
      console.log("started recorder");
    }
  }
});

window.postMessage(
  {
    type: "CONTENT_READY",
  },
  "*",
);

console.log("ready");
