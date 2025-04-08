// Logs at chrome://serviceworker-internals

// let recorder;
//
//
//
//
//

const getStreamId = async (tabId) => {
  const streamId = await new Promise((resolve, reject) => {
    chrome.tabCapture.getMediaStreamId({ consumerTabId: tabId }, (id) => {
      if (chrome.runtime.lastError) {
        reject(chrome.runtime.lastError.message);
      } else {
        resolve(id);
      }
    });
  });
  return streamId;
};

// const startRecording = async (port) => {
//   console.log("start recording");
//   const tab = await getCurrentTabId();
//   console.log("current tab", tab);

//   const client = new WebSocket(`ws://localhost:${port}`, []);

//   await new Promise((resolve) => {
//     if (client.readyState === WebSocket.OPEN) resolve();
//     client.addEventListener("open", () => resolve());
//   });

//   const stream = await new Promise(async (resolve, reject) => {
//     console.log("starting stream for tab", tab);
//     const streamId = await new Promise((resolve, reject) => {
//       chrome.tabCapture.getMediaStreamId({ consumerTabId: tab }, (id) => {
//         if (chrome.runtime.lastError) {
//           reject(chrome.runtime.lastError.message);
//         } else {
//           resolve(id);
//         }
//       });
//     });

//     const stream = await navigator.mediaDevices.getUserMedia({
//       audio: {
//         mandatory: {
//           chromeMediaSource: "tab",
//           chromeMediaSourceId: streamId,
//         },
//       },
//       video: {
//         mandatory: {
//           chromeMediaSource: "tab",
//           chromeMediaSourceId: streamId,
//         },
//       },
//     });

//     resolve(stream);
//   });

//   recorder = new MediaRecorder(stream, {
//     audioBitsPerSecond: 128_000,
//     videoBitsPerSecond: 2_500_000,
//     mimeType: "video/webm",
//   });

//   recorder.ondataavailable = async (e) => {
//     if (!e.data.size) return;
//     console.log("data available", e);
//     const buffer = await e.data.arrayBuffer();
//     client.send(buffer);
//   };

//   recorder.onerror = () => recorder.stop();

//   recorder.onstop = function () {
//     try {
//       const tracks = stream.getTracks();

//       tracks.forEach(function (track) {
//         track.stop();
//       });

//       // if (client.readyState === WebSocket.OPEN) client.close();
//     } catch (error) {}
//   };

//   stream.onremovetrack = () => {
//     try {
//       recorder.stop();
//     } catch (error) {}
//   };

//   recorder.start(20);
// };

// const stopRecording = async (index) => {
//   console.log("stop recording", index);
//   if (!recorder) return;
//   if (recorder.state === "inactive") return;
//   recorder.stop();
// };

const getCurrentTabId = async () => {
  let queryOptions = { active: true, lastFocusedWindow: true };
  let [tab] = await chrome.tabs.query(queryOptions);
  return tab.id;
};

chrome.runtime.onMessage.addListener(async (message) => {
  console.log("got message here", message);
  if (message.command === "get-stream-id") {
    const tabId = await getCurrentTabId();
    const streamId = await getStreamId(tabId);
    console.log("got stream id", streamId);
    chrome.tabs.sendMessage(tabId, {
      command: "stream-id",
      streamId,
    });
  }
});
