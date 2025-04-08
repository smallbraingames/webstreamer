// Logs at chrome://serviceworker-internals

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

const getCurrentTabId = async () => {
  let queryOptions = { active: true, lastFocusedWindow: true };
  let [tab] = await chrome.tabs.query(queryOptions);
  return tab.id;
};

chrome.runtime.onMessage.addListener(async (message) => {
  if (message.command === "get-stream-id") {
    const tabId = await getCurrentTabId();
    const streamId = await getStreamId(tabId);
    chrome.tabs.sendMessage(tabId, {
      command: "stream-id",
      streamId,
    });
  } else if (message.command === "open-popup") {
    chrome.action.openPopup();
  }
});
