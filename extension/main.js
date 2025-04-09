console.log("read extension started");

const injectFrameAnimation = () => {
  const animDiv = document.createElement("div");
  Object.assign(animDiv.style, {
    position: "fixed",
    width: "1px",
    height: "1px",
    bottom: "0px",
    right: "0px",
    backgroundColor: "transparent",
    zIndex: "-9999",
    opacity: "0.01",
    pointerEvents: "none",
  });
  document.body.appendChild(animDiv);
  let x = 0;
  const animate = () => {
    x = (x + 1) % 10;
    animDiv.style.transform = `translateX(${x}px)`;
    requestAnimationFrame(animate);
  };
  animate();
  return animDiv;
};

// Force rerenders
const frameForcer = injectFrameAnimation();

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

      const recorder = new MediaRecorder(stream, {
        audioBitsPerSecond: 128_000,
        videoBitsPerSecond: 2_500_000,
        mimeType: "video/webm",
      });

      recorder.ondataavailable = async (e) => {
        if (!e.data.size) return;
        const buffer = await e.data.arrayBuffer();
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

      recorder.start(41);

      console.log(recorder.state);
      console.log("started recorder");

      client.onmessage = async (e) => {
        console.log(e);
        window.postMessage({ type: "EXTENSION", message: e.data }, "*");
      };
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
