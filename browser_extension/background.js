const WS_URL = 'ws://localhost:8765';
const PING_INTERVAL_MS = 15_000;
const RECONNECT_ALARM = 'mcp-reconnect';
const RECONNECT_INTERVAL_MIN = 0.5;

let ws = null;
let isConnecting = false;
let pingTimer = null;
let selectors = null;

// ─── Default selectors (fallback if selectors.json fails to load) ──

const DEFAULT_SELECTORS = {
  promptInput: ['textarea#prompt-textarea', 'div[contenteditable="true"]', 'textarea'],
  sendButton: ['button[data-testid="send-button"]', 'button[aria-label*="Send"]', 'button[aria-label*="send"]'],
  stopButton: ['button[data-testid="stop-button"]', 'button[aria-label*="Stop"]', 'button[aria-label*="stop"]'],
  newChatButton: ['button[data-testid="create-new-chat-button"]', 'a[href="/"]'],
  newChatText: ['New chat', 'Новый чат'],
  assistantMessage: ['[data-message-author-role="assistant"]'],
  conversationTurn: ['[data-testid^="conversation-turn-"]'],
  markdownContainer: ['div.markdown', 'div.prose', 'div[class*="markdown"]'],
  userMarker: ['[class*="text-message"]', '[data-message-author-role="user"]'],
  modelSelector: ['button[data-testid*="model-selector"]', 'button[aria-haspopup][class*="model"]', 'button[aria-expanded][class*="model"]', 'button[data-testid="model-switcher-button"]'],
  modelDropdownItem: ['[role="menuitem"]', '[role="option"]', 'button[role="menuitemradio"]', 'a[role="menuitem"]'],
};

// ─── Selectors loading ───────────────────────────────────────

async function ensureSelectors() {
  if (selectors) return selectors;
  try {
    const url = chrome.runtime.getURL('selectors.json');
    const resp = await fetch(url);
    selectors = await resp.json();
    console.log('[CodexDCP] selectors loaded from selectors.json');
  } catch (err) {
    console.warn('[CodexDCP] using default selectors:', err.message);
    selectors = DEFAULT_SELECTORS;
  }
  return selectors;
}

// ─── WebSocket connection ───────────────────────────────────

function connect() {
  if (isConnecting) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

  isConnecting = true;
  if (ws) { try { ws.close(); } catch {} ws = null; }

  console.log('[CodexDCP] connecting to', WS_URL);
  ws = new WebSocket(WS_URL);

  ws.onopen = () => {
    isConnecting = false;
    console.log('[CodexDCP] connected');
    sendMessage({ type: 'register', client: 'codexdcp-extension' });
    startPing();
    chrome.alarms.clear(RECONNECT_ALARM);
    ensureSelectors();
  };

  ws.onmessage = async (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === 'pong') return;
      if (msg.type === 'request') {
        const onPartial = (text) => sendMessage({ type: 'partial', id: msg.id, text });
        const response = await handleRequest(msg, onPartial);
        sendMessage(response);
      }
    } catch (err) {
      console.error('[CodexDCP] message error:', err);
    }
  };

  ws.onerror = (err) => console.error('[CodexDCP] ws error:', err);

  ws.onclose = () => {
    isConnecting = false;
    ws = null;
    console.log('[CodexDCP] disconnected');
    stopPing();
    scheduleReconnect();
  };
}

function sendMessage(msg) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(msg));
}

function startPing() {
  stopPing();
  pingTimer = setInterval(() => sendMessage({ type: 'ping' }), PING_INTERVAL_MS);
}

function stopPing() {
  if (pingTimer) { clearInterval(pingTimer); pingTimer = null; }
}

function scheduleReconnect() {
  chrome.alarms.create(RECONNECT_ALARM, {
    delayInMinutes: RECONNECT_INTERVAL_MIN,
    periodInMinutes: RECONNECT_INTERVAL_MIN,
  });
}

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === RECONNECT_ALARM) connect();
});

chrome.runtime.onMessage.addListener((request, _sender, _sendResponse) => {
  if (request.action === 'content_script_loaded') connect();
  return false;
});

// ─── Tab helpers ────────────────────────────────────────────

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findChatGPTTab() {
  const tabs = await chrome.tabs.query({
    url: ['https://chatgpt.com/*', 'https://chat.openai.com/*'],
  });
  if (tabs.length === 0) return null;
  return tabs.find((t) => t.active) || tabs[0];
}

function waitForTabLoad(tabId, timeoutMs) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      chrome.tabs.onUpdated.removeListener(listener);
      reject(new Error('Tab load timeout'));
    }, timeoutMs);

    function listener(id, info) {
      if (id === tabId && info.status === 'complete') {
        clearTimeout(timeout);
        chrome.tabs.onUpdated.removeListener(listener);
        setTimeout(resolve, 4000);
      }
    }

    chrome.tabs.onUpdated.addListener(listener);
    chrome.tabs.get(tabId, (tab) => {
      if (chrome.runtime.lastError) return;
      if (tab.status === 'complete') {
        clearTimeout(timeout);
        chrome.tabs.onUpdated.removeListener(listener);
        setTimeout(resolve, 4000);
      }
    });
  });
}

// ─── Request handler ────────────────────────────────────────

async function handleRequest(msg, onPartial) {
  const { id, method, params } = msg;
  const sels = await ensureSelectors();
  const format = params?.format || 'markdown';

  const tab = await findChatGPTTab();
  if (!tab || !tab.id) {
    return { type: 'response', id, error: { message: 'No ChatGPT tab found. Open https://chatgpt.com.' } };
  }

  // ── new_chat: just click the button ──
  if (method === 'new_chat') {
    try {
      await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: pageNewChat,
        args: [sels],
        world: 'MAIN',
      });
      return { type: 'response', id, result: { text: 'New chat started.' } };
    } catch (err) {
      return { type: 'response', id, error: { message: err.message } };
    }
  }

  // ── send_message: model select + new chat + send + poll ──

  // Step 1: Select model if requested
  if (params?.model) {
    console.log('[CodexDCP] selecting model:', params.model);
    try {
      await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: pageClickModelButton,
        args: [sels],
        world: 'MAIN',
      });
      await sleep(500);
      await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: pageSelectModel,
        args: [params.model, sels],
        world: 'MAIN',
      });
      await sleep(500);
    } catch (err) {
      console.warn('[CodexDCP] model selection failed:', err.message);
    }
  }

  // Step 2: New chat if requested
  if (params?.new_chat) {
    try {
      await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: pageNewChat,
        args: [sels],
        world: 'MAIN',
      });
      await sleep(1000);
    } catch (err) {
      console.warn('[CodexDCP] new chat failed:', err.message);
    }
  }

  // Step 3: Send prompt
  let sendError = null;
  try {
    const sendResult = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: pageSendPrompt,
      args: [params?.prompt || '', sels],
      world: 'MAIN',
    });
    if (sendResult?.[0]?.result?.error) {
      sendError = sendResult[0].result.error;
    }
  } catch (err) {
    sendError = { message: err.message };
  }

  if (sendError) {
    return { type: 'response', id, error: sendError };
  }

  // Step 4: Poll for response
  const timeoutSec = Math.min(params?.timeout || 60, 90);
  const deadline = Date.now() + timeoutSec * 1000;
  let lastText = '';
  let stableCount = 0;
  let result = null;

  console.log('[CodexDCP] polling for response (timeout:', timeoutSec + 's)');

  while (Date.now() < deadline) {
    await sleep(1000);

    try {
      const pollResult = await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: pageReadAndCheck,
        args: [format, sels],
        world: 'MAIN',
      });
      result = pollResult?.[0]?.result;
    } catch (err) {
      continue;
    }

    const text = result?.text;
    const isGenerating = result?.isGenerating;

    if (text && text.length > 0) {
      if (text !== lastText) {
        if (onPartial) onPartial(text);
        lastText = text;
        stableCount = 0;
      } else if (!isGenerating) {
        stableCount++;
        if (stableCount >= 2) {
          console.log('[CodexDCP] response stable:', text.length, 'chars');
          return { type: 'response', id, result: { text, format } };
        }
      }
    }
  }

  // Step 5: Timeout — return whatever we have
  if (lastText) {
    console.log('[CodexDCP] timeout, returning partial:', lastText.length, 'chars');
    return { type: 'response', id, result: { text: lastText, format } };
  }

  // Step 6: Fallback — reload page and read persisted conversation
  console.log('[CodexDCP] fallback: reload + read');
  try {
    await chrome.tabs.reload(tab.id);
    await waitForTabLoad(tab.id, 30000);
    const readResult = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: pageReadAndCheck,
      args: [format, sels],
      world: 'MAIN',
    });
    result = readResult?.[0]?.result;
    if (result?.text) {
      console.log('[CodexDCP] fallback result:', result.text.length, 'chars');
      return { type: 'response', id, result: { text: result.text, format } };
    }
  } catch (err) {
    console.error('[CodexDCP] fallback error:', err.message);
  }

  return {
    type: 'response',
    id,
    error: {
      message: 'No response received after ' + timeoutSec + 's. ' +
        'Check that ChatGPT is responding in the browser tab.'
    },
  };
}

// ─── Injected: new chat ─────────────────────────────────────

async function pageNewChat(sels) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  let btn = null;
  for (const sel of (sels.newChatButton || [])) {
    btn = document.querySelector(sel);
    if (btn) break;
  }
  if (!btn) {
    const texts = sels.newChatText || ['New chat'];
    btn = Array.from(document.querySelectorAll('button, a')).find((b) =>
      texts.some((t) => b.textContent.includes(t))
    );
  }
  if (btn) { btn.click(); await sleep(1000); }
}

// ─── Injected: send prompt ──────────────────────────────────

async function pageSendPrompt(prompt, sels) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  function findPromptInput() {
    for (const sel of (sels.promptInput || [])) {
      const el = document.querySelector(sel);
      if (el) return el;
    }
    return null;
  }

  function findSendButton() {
    for (const sel of (sels.sendButton || [])) {
      const btn = document.querySelector(sel);
      if (btn) return btn;
    }
    return null;
  }

  const input = findPromptInput();
  if (!input) return { error: { message: 'Prompt input not found.' } };

  if (input.tagName === 'TEXTAREA') {
    const setter = Object.getOwnPropertyDescriptor(
      window.HTMLTextAreaElement.prototype, 'value'
    ).set;
    setter.call(input, prompt);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  } else {
    input.innerText = prompt;
    input.dispatchEvent(new InputEvent('input', {
      bubbles: true, data: prompt, inputType: 'insertText'
    }));
  }

  await sleep(200);

  const sendBtn = findSendButton();
  if (sendBtn && !sendBtn.disabled) {
    sendBtn.click();
  } else {
    input.focus();
    input.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true, cancelable: true
    }));
    input.dispatchEvent(new KeyboardEvent('keypress', {
      key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true, cancelable: true
    }));
    input.dispatchEvent(new KeyboardEvent('keyup', {
      key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true
    }));
  }

  return { ok: true };
}

// ─── Injected: read assistant text + check if generating ────

async function pageReadAndCheck(format, sels) {
  function queryFirst(selectors) {
    for (const sel of selectors) {
      const el = document.querySelector(sel);
      if (el) return el;
    }
    return null;
  }

  function findStopButton(sels) {
    return !!queryFirst(sels.stopButton || []);
  }

  function getAssistantElements(sels) {
    let els = document.querySelectorAll((sels.assistantMessage || ['[data-message-author-role="assistant"]']).join(', '));
    if (els.length > 0) return Array.from(els);

    const turnSel = (sels.conversationTurn || ['[data-testid^="conversation-turn-"]']).join(', ');
    const turns = document.querySelectorAll(turnSel);
    const mdSel = (sels.markdownContainer || ['div.markdown']).join(', ');
    const userSel = (sels.userMarker || ['[data-message-author-role="user"]']).join(', ');

    const assistantTurns = Array.from(turns).filter((el) => {
      const hasMarkdown = el.querySelector(mdSel);
      const hasUserMarker = el.querySelector(userSel);
      return hasMarkdown && !hasUserMarker;
    });
    return assistantTurns.length > 0 ? assistantTurns : Array.from(turns);
  }

  function htmlToMarkdown(element) {
    function convertNode(node) {
      if (node.nodeType === Node.TEXT_NODE) {
        return node.textContent;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) return '';

      const tag = node.tagName.toLowerCase();
      const children = Array.from(node.childNodes).map(convertNode).join('');

      switch (tag) {
        case 'pre': {
          const code = node.querySelector('code');
          const lang = code?.className?.match(/language-(\w+)/)?.[1] || '';
          const text = code ? code.textContent : node.textContent;
          return '\n```' + lang + '\n' + text.trim() + '\n```\n';
        }
        case 'code':
          if (node.parentElement?.tagName === 'PRE') return node.textContent;
          return '`' + node.textContent + '`';
        case 'h1': return '\n# ' + children.trim() + '\n';
        case 'h2': return '\n## ' + children.trim() + '\n';
        case 'h3': return '\n### ' + children.trim() + '\n';
        case 'h4': return '\n#### ' + children.trim() + '\n';
        case 'h5': return '\n##### ' + children.trim() + '\n';
        case 'h6': return '\n###### ' + children.trim() + '\n';
        case 'strong':
        case 'b':
          return '**' + children + '**';
        case 'em':
        case 'i':
          return '*' + children + '*';
        case 'del':
        case 's':
          return '~~' + children + '~~';
        case 'a': {
          const href = node.getAttribute('href') || '';
          return '[' + children + '](' + href + ')';
        }
        case 'ul': {
          return '\n' + Array.from(node.children).map(li => '- ' + convertNode(li).trim()).join('\n') + '\n';
        }
        case 'ol': {
          return '\n' + Array.from(node.children).map((li, i) => (i + 1) + '. ' + convertNode(li).trim()).join('\n') + '\n';
        }
        case 'li':
          return children;
        case 'p':
          return '\n' + children + '\n';
        case 'br':
          return '\n';
        case 'blockquote':
          return '\n' + children.split('\n').map(l => '> ' + l).join('\n') + '\n';
        case 'hr':
          return '\n---\n';
        case 'table':
          return convertTable(node);
        case 'span':
        case 'div':
        case 'sup':
        case 'sub':
          return children;
        default:
          return children;
      }
    }

    function convertTable(table) {
      const rows = Array.from(table.querySelectorAll('tr'));
      if (rows.length === 0) return '';
      const result = [];
      rows.forEach((row, i) => {
        const cells = Array.from(row.querySelectorAll('th, td')).map(c => c.textContent.trim());
        result.push('| ' + cells.join(' | ') + ' |');
        if (i === 0) {
          result.push('| ' + cells.map(() => '---').join(' | ') + ' |');
        }
      });
      return '\n' + result.join('\n') + '\n';
    }

    return convertNode(element).trim();
  }

  const isGenerating = findStopButton(sels);
  const els = getAssistantElements(sels);
  if (els.length === 0) return { isGenerating, text: null };

  const last = els[els.length - 1];
  const mdSel = (sels.markdownContainer || ['div.markdown']).join(', ');
  const md = last.querySelector(mdSel);

  let text = null;
  if (format === 'text' || !md) {
    text = last.innerText?.trim() || null;
  } else {
    text = htmlToMarkdown(md);
    if (!text) text = last.innerText?.trim() || null;
  }

  return { isGenerating, text };
}

// ─── Injected: model selection ──────────────────────────────

async function pageClickModelButton(sels) {
  for (const sel of (sels.modelSelector || [])) {
    const btn = document.querySelector(sel);
    if (btn) { btn.click(); return true; }
  }
  // Fallback: find button near the prompt input with aria-haspopup
  const input = document.querySelector('textarea#prompt-textarea') ||
                document.querySelector('div[contenteditable="true"]');
  if (input) {
    const container = input.closest('form') || input.parentElement?.parentElement;
    if (container) {
      const btn = container.querySelector('button[aria-haspopup], button[aria-expanded]');
      if (btn) { btn.click(); return true; }
    }
  }
  return false;
}

async function pageSelectModel(modelName, sels) {
  const lower = modelName.toLowerCase();
  const itemSel = (sels.modelDropdownItem || ['[role="menuitem"]', '[role="option"]']).join(', ');
  const items = document.querySelectorAll(itemSel);
  for (const item of items) {
    const text = item.textContent?.trim().toLowerCase() || '';
    if (text.includes(lower)) {
      item.click();
      return true;
    }
  }
  // Fallback: try all buttons and links
  const allBtns = document.querySelectorAll('button, a');
  for (const btn of allBtns) {
    const text = btn.textContent?.trim().toLowerCase() || '';
    if (text.includes(lower) && text.length < 50) {
      btn.click();
      return true;
    }
  }
  return false;
}

// ─── Lifecycle ──────────────────────────────────────────────

ensureSelectors().then(() => connect());
