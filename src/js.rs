pub const INIT_SCRIPT: &str = r#"
(function() {
  const sels = __SELECTORS__;
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));

  function queryFirst(selectors) {
    for (const sel of selectors) {
      const el = document.querySelector(sel);
      if (el) return el;
    }
    return null;
  }

  function htmlToMarkdown(element) {
    function convertNode(node) {
      if (node.nodeType === Node.TEXT_NODE) return node.textContent;
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
        case 'strong': case 'b': return '**' + children + '**';
        case 'em': case 'i': return '*' + children + '*';
        case 'del': case 's': return '~~' + children + '~~';
        case 'a': { const href = node.getAttribute('href') || ''; return '[' + children + '](' + href + ')'; }
        case 'ul': return '\n' + Array.from(node.children).map(li => '- ' + convertNode(li).trim()).join('\n') + '\n';
        case 'ol': return '\n' + Array.from(node.children).map((li, i) => (i + 1) + '. ' + convertNode(li).trim()).join('\n') + '\n';
        case 'li': return children;
        case 'p': return '\n' + children + '\n';
        case 'br': return '\n';
        case 'blockquote': return '\n' + children.split('\n').map(l => '> ' + l).join('\n') + '\n';
        case 'hr': return '\n---\n';
        case 'table': return convertTable(node);
        case 'span': case 'div': case 'sup': case 'sub': return children;
        default: return children;
      }
    }
    function convertTable(table) {
      const rows = Array.from(table.querySelectorAll('tr'));
      if (rows.length === 0) return '';
      const result = [];
      rows.forEach((row, i) => {
        const cells = Array.from(row.querySelectorAll('th, td')).map(c => c.textContent.trim());
        result.push('| ' + cells.join(' | ') + ' |');
        if (i === 0) result.push('| ' + cells.map(() => '---').join(' | ') + ' |');
      });
      return '\n' + result.join('\n') + '\n';
    }
    return convertNode(element).trim();
  }

  async function pageSendPrompt(prompt) {
    function findPromptInput() {
      for (const sel of (sels.promptInput || [])) { const el = document.querySelector(sel); if (el) return el; }
      return null;
    }
    function findSendButton() {
      for (const sel of (sels.sendButton || [])) { const btn = document.querySelector(sel); if (btn) return btn; }
      return null;
    }
    const input = findPromptInput();
    if (!input) return { error: { message: 'Prompt input not found.' } };
    if (input.tagName === 'TEXTAREA') {
      const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
      setter.call(input, prompt);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
    } else {
      input.innerText = prompt;
      input.dispatchEvent(new InputEvent('input', { bubbles: true, data: prompt, inputType: 'insertText' }));
    }
    await sleep(200);
    const sendBtn = findSendButton();
    if (sendBtn && !sendBtn.disabled) { sendBtn.click(); }
    else {
      input.focus();
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true, cancelable: true }));
      input.dispatchEvent(new KeyboardEvent('keypress', { key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true, cancelable: true }));
      input.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true }));
    }
    return { ok: true };
  }

  async function pageReadAndCheck(format) {
    function findStopButton() { return !!queryFirst(sels.stopButton || []); }
    function getAssistantElements() {
      let els = document.querySelectorAll((sels.assistantMessage || ['[data-message-author-role="assistant"]']).join(', '));
      if (els.length > 0) return Array.from(els);
      const turnSel = (sels.conversationTurn || ['[data-testid^="conversation-turn-"]']).join(', ');
      const turns = document.querySelectorAll(turnSel);
      const mdSel = (sels.markdownContainer || ['div.markdown']).join(', ');
      const userSel = (sels.userMarker || ['[data-message-author-role="user"]']).join(', ');
      const assistantTurns = Array.from(turns).filter(el => el.querySelector(mdSel) && !el.querySelector(userSel));
      return assistantTurns.length > 0 ? assistantTurns : Array.from(turns);
    }
    const isGenerating = findStopButton();
    const els = getAssistantElements();
    if (els.length === 0) return { isGenerating, text: null };
    const last = els[els.length - 1];
    const mdSel = (sels.markdownContainer || ['div.markdown']).join(', ');
    const md = last.querySelector(mdSel);
    let text = null;
    if (format === 'text' || !md) { text = last.innerText?.trim() || null; }
    else { text = htmlToMarkdown(md); if (!text) text = last.innerText?.trim() || null; }
    return { isGenerating, text };
  }

  async function pageClickModelButton() {
    for (const sel of (sels.modelSelector || [])) {
      const btn = document.querySelector(sel);
      if (btn) { btn.click(); return true; }
    }
    const input = document.querySelector('textarea#prompt-textarea') || document.querySelector('div[contenteditable="true"]');
    if (input) {
      const container = input.closest('form') || input.parentElement?.parentElement;
      if (container) {
        const btn = container.querySelector('button[aria-haspopup], button[aria-expanded]');
        if (btn) { btn.click(); return true; }
      }
    }
    return false;
  }

  async function pageSelectModel(modelName) {
    const lower = modelName.toLowerCase();
    const itemSel = (sels.modelDropdownItem || ['[role="menuitem"]', '[role="option"]']).join(', ');
    const items = document.querySelectorAll(itemSel);
    for (const item of items) {
      const text = item.textContent?.trim().toLowerCase() || '';
      if (text.includes(lower)) { item.click(); return true; }
    }
    const allBtns = document.querySelectorAll('button, a');
    for (const btn of allBtns) {
      const text = btn.textContent?.trim().toLowerCase() || '';
      if (text.includes(lower) && text.length < 50) { btn.click(); return true; }
    }
    return false;
  }

  async function pageSetTempChat(enabled) {
    // In the new ChatGPT UI, temp chat is controlled via URL parameter (?temporary-chat=true).
    // This function checks if temp chat is active and navigates if needed.
    const label = document.querySelector('[data-testid="temporary-chat-label"]');
    const isOn = !!label;
    if (enabled === isOn) return { ok: true, state: enabled ? 'on' : 'off' };
    // To enable: navigate to the temp chat URL. To disable: navigate to plain URL.
    if (enabled) {
      window.location.href = 'https://chatgpt.com/?temporary-chat=true';
    } else {
      window.location.href = 'https://chatgpt.com/';
    }
    return { ok: true, state: enabled ? 'on' : 'off' };
  }

  function pageIsTempChat() {
    return !!document.querySelector('[data-testid="temporary-chat-label"]');
  }

  window.__codexdcp = { pageSendPrompt, pageReadAndCheck, pageClickModelButton, pageSelectModel, pageSetTempChat, pageIsTempChat };
})();
"#;

pub fn init_script(selectors_json: &str) -> String {
    INIT_SCRIPT.replace("__SELECTORS__", selectors_json)
}

pub fn call_send_prompt(prompt: &str) -> String {
    let escaped = serde_json::to_string(prompt).unwrap_or_else(|_| format!("\"{}\"", prompt.replace('\\', "\\\\").replace('"', "\\\"")));
    format!("window.__codexdcp && window.__codexdcp.pageSendPrompt({})", escaped)
}

pub fn call_read_and_check(format: &str) -> String {
    let escaped = serde_json::to_string(format).unwrap_or_else(|_| format!("\"{}\"", format));
    format!("window.__codexdcp && window.__codexdcp.pageReadAndCheck({})", escaped)
}

pub fn call_click_model_button() -> &'static str {
    "window.__codexdcp && window.__codexdcp.pageClickModelButton()"
}

pub fn call_select_model(model: &str) -> String {
    let escaped = serde_json::to_string(model).unwrap_or_else(|_| format!("\"{}\"", model.replace('\\', "\\\\").replace('"', "\\\"")));
    format!("window.__codexdcp && window.__codexdcp.pageSelectModel({})", escaped)
}

pub fn call_set_temp_chat(enabled: bool) -> String {
    format!("window.__codexdcp && window.__codexdcp.pageSetTempChat({})", enabled)
}

pub fn call_is_temp_chat() -> String {
    "window.__codexdcp ? window.__codexdcp.pageIsTempChat() : (document.querySelector('[data-testid=\"temporary-chat-label\"]') !== null)".to_string()
}

pub fn call_is_ready() -> &'static str {
    "typeof window.__codexdcp !== 'undefined'"
}

pub const DEFAULT_SELECTORS: &str = r#"{
  "promptInput": ["textarea#prompt-textarea", "div[contenteditable=\"true\"]", "textarea"],
  "sendButton": ["button[data-testid=\"send-button\"]", "button[aria-label*=\"Send\"]", "button[aria-label*=\"send\"]"],
  "stopButton": ["button[data-testid=\"stop-button\"]", "button[aria-label*=\"Stop\"]", "button[aria-label*=\"stop\"]"],
  "assistantMessage": ["[data-message-author-role=\"assistant\"]"],
  "conversationTurn": ["[data-testid^=\"conversation-turn-\"]"],
  "markdownContainer": ["div.markdown", "div.prose", "div[class*=\"markdown\"]"],
  "userMarker": ["[class*=\"text-message\"]", "[data-message-author-role=\"user\"]"],
  "modelSelector": ["button[data-testid*=\"model-selector\"]", ".__composer-pill[aria-haspopup=\"menu\"]", "button[aria-haspopup][class*=\"model\"]", "button[aria-expanded][class*=\"model\"]", "button[data-testid=\"model-switcher-button\"]"],
  "modelDropdownItem": ["[role=\"menuitem\"]", "[role=\"option\"]", "button[role=\"menuitemradio\"]", "a[role=\"menuitem\"]"],
  "tempChatButton": ["[data-testid=\"temporary-chat-label\"]"],
  "tempChatText": ["Temporary Chat", "Временный чат", "Чат без истории", "Temp chat", "Чат без сохранения"]
}"#;
