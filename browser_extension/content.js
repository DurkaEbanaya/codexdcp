console.log('[CodexDCP] content script loaded');

// Tell background to (re)connect WebSocket
chrome.runtime.sendMessage({ action: 'content_script_loaded' }).catch(() => {});
