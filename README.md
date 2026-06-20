# CodexDCP — Codex Developer Chaos Platform

MCP-коннектор, который превращает браузерный ChatGPT в Codex-подобного агента для OpenCode. Rust MCP-сервер через stdio + WebSocket bridge к Chrome-расширению, которое управляет веб-интерфейсом ChatGPT. Включает OpenAI-совместимый HTTP-провайдер для использования ChatGPT как модели.

## Архитектура

```
┌─────────────┐      stdio (MCP)     ┌──────────────────────┐     WebSocket     ┌─────────────────────┐
│   OpenCode    │  ──────────────────> │      CodexDCP         │  <────────────>  │  Chrome extension   │
│  (MCP client) │                      │  (Rust MCP server)    │                  │  (content script)   │
└─────────────┘                        └──────────────────────┘                  └─────────────────────┘
                                              │                                          │
                                              │ HTTP (OpenAI API)                        │ DOM
                                              ▼                                          ▼
                                     ┌──────────────────────┐                 ┌─────────────────────┐
                                     │  HTTP provider :8766  │                 │   chatgpt.com       │
                                     └──────────────────────┘                 └─────────────────────┘
```

- **Rust MCP server** — реализует протокол MCP через stdio, предоставляет инструменты `chatgpt_coder`, `chatgpt_ask`, `chatgpt_new_chat`, `chatgpt_status`.
- **WebSocket bridge** — внутри сервера, слушает `ws://127.0.0.1:8765`, маршрутизирует запросы/ответы, ретраи, стриминг.
- **HTTP provider** — OpenAI-совместимый API на порту 8766 (`/v1/chat/completions`, `/v1/models`, `/health`), поддерживает streaming (SSE).
- **Chrome/Edge расширение** — подключается к серверу через WebSocket, управляет вкладкой ChatGPT через DOM, конвертирует HTML в markdown, выбирает модели.

## Возможности

- **MCP-инструменты** — делегируй задачи по коду и вопросы в ChatGPT прямо из OpenCode
- **HTTP-провайдер** — используй ChatGPT как модель-провайдер (OpenAI-совместимый API)
- **Sticky-chat режим** — все запросы в один разговор, без необходимости передавать `new_chat: false`
- **Выбор модели** — передавай `model: "GPT-4o"`, `"o1"`, и т.д.
- **Сохранение markdown** — ответы сохраняют code blocks, заголовки, ссылки, таблицы
- **Стриминг** — частичные ответы через SSE для HTTP-провайдера
- **Ретраи с backoff** — автоматический повтор при временных ошибках
- **Настраиваемые селекторы** — DOM-селекторы в `selectors.json`, обновляй без правки кода
- **Кастомный system prompt** — через CLI-флаг или env-переменную
- **Graceful shutdown** — Ctrl+C корректно останавливает все компоненты

## Требования

- Rust >= 1.85 (edition 2024; проверено на 1.96)
- Chrome, Edge, Brave или другой Chromium-браузер
- Аккаунт ChatGPT с открытой вкладкой `https://chatgpt.com`
- OpenCode с поддержкой MCP

## Установка

### Шаг 1. Получить бинарник

**Вариант A — предсобранный (без Rust):**

Скачайте бинарник со страницы [Releases](https://github.com/anomalyco/codexdcp/releases) под вашу платформу:

```bash
# macOS Apple Silicon
curl -L https://github.com/anomalyco/codexdcp/releases/latest/download/codexdcp-aarch64-apple-darwin -o codexdcp
# macOS Intel
curl -L https://github.com/anomalyco/codexdcp/releases/latest/download/codexdcp-x86_64-apple-darwin -o codexdcp
# Linux
curl -L https://github.com/anomalyco/codexdcp/releases/latest/download/codexdcp-x86_64-unknown-linux-gnu -o codexdcp
chmod +x codexdcp
```

**Вариант B — из исходников:**

```bash
git clone https://github.com/anomalyco/codexdcp.git
cd codexdcp
cargo build --release
# Бинарник: target/release/codexdcp
```

### Шаг 2. Установить расширение

1. Откройте `chrome://extensions` (или `brave://extensions`)
2. Включите **Режим разработчика** (Developer mode)
3. Нажмите **Загрузить распакованное** (Load unpacked)
4. Выберите папку `browser_extension/` из репозитория
5. Откройте `https://chatgpt.com` и авторизуйтесь
6. В консоли service worker должно появиться: `[CodexDCP] connected`

### Шаг 3. Настроить OpenCode

Добавьте сервер в `~/.config/opencode/opencode.jsonc`:

```jsonc
{
  "mcp": {
    "chatgpt-codex": {
      "type": "local",
      "command": [
        "/ABSOLUTE/PATH/TO/codexdcp",
        "--http-port", "8766",
        "--sticky-chat"
      ],
      "enabled": true
    }
  }
}
```

Замените `/ABSOLUTE/PATH/TO/` на реальный путь к бинарнику.

### Шаг 4. (Опционально) Подключить как провайдер моделей

Добавьте в секцию `provider`:

```jsonc
{
  "provider": {
    "chatgpt-browser": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "ChatGPT (Browser)",
      "options": {
        "baseURL": "http://127.0.0.1:8766/v1"
      },
      "models": {
        "gpt-4o": { "name": "GPT-4o" },
        "o1": { "name": "o1" },
        "gpt-4o-mini": { "name": "GPT-4o mini" }
      }
    }
  }
}
```

### Шаг 5. (Опционально) Команды и ключевые слова

Добавьте в секцию `command`:

```jsonc
{
  "command": {
    "навайбкодь": {
      "description": "Делегировать задачу по коду в ChatGPT",
      "template": "Use the chatgpt_coder MCP tool to delegate: $ARGUMENTS"
    },
    "спроси": {
      "description": "Задать вопрос ChatGPT",
      "template": "Use the chatgpt_ask MCP tool to ask: $ARGUMENTS"
    },
    "дальше": {
      "description": "Продолжить разговор в ChatGPT",
      "template": "Use chatgpt_ask with new_chat=false to continue: $ARGUMENTS"
    },
    "новый-чат": {
      "description": "Начать новый чат ChatGPT",
      "template": "Use chatgpt_new_chat to start a new chat"
    }
  }
}
```

### Шаг 6. Перезапустить OpenCode

```bash
opencode
```

Проверьте: скажите OpenCode "проверь статус ChatGPT" — должно вернуть:
```
ChatGPT browser extension is connected. [sticky: active conversation]
```

## Инструменты MCP

| Инструмент | Описание | Параметры |
|---|---|---|
| `chatgpt_coder` | Делегировать задачу по коду (Codex-style) | `task`, `context`, `language`, `new_chat`, `model`, `format` |
| `chatgpt_ask` | Задать общий вопрос | `prompt`, `new_chat`, `model`, `format` |
| `chatgpt_new_chat` | Начать новый чат | — |
| `chatgpt_status` | Проверить статус подключения | — |

## HTTP API

### Обычный запрос
```bash
curl http://127.0.0.1:8766/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [
      {"role": "user", "content": "Напиши fizzbuzz на Rust"}
    ]
  }'
```

### Стриминг (SSE)
```bash
curl http://127.0.0.1:8766/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "stream": true,
    "messages": [
      {"role": "user", "content": "Напиши хайку про компилятор"}
    ]
  }'
```

### Health
```bash
curl http://127.0.0.1:8766/health
# → {"status":"ok","connected":true,"has_active_chat":false}
```

## CLI-флаги

```
--ws-host          127.0.0.1    хост WebSocket
--ws-port          8765         порт WebSocket
--http-host        127.0.0.1    хост HTTP-провайдера
--http-port        0            порт HTTP (0 = выключен)
--default-timeout  120          таймаут ответа ChatGPT (сек)
--system-prompt    -            кастомный системный промпт
--max-retries      2            кол-во ретраев
--retry-delay-ms   2000         начальная задержка ретрая (мс)
--sticky-chat                   режим одного чата
--log-level        info         уровень логирования
```

Все флаги также доступны через env-переменные (`CODEXDCP_*` или `CHATGPT_BRIDGE_*`).

## Настраиваемые селекторы

DOM-селекторы вынесены в `browser_extension/selectors.json`. Когда ChatGPT меняет вёрстку, обновите JSON и перезагрузите расширение — править код не нужно.

## Отладка

- Бинарник с debug-логами: `codexdcp --log-level debug`
- Логи расширения: `chrome://extensions` → service worker → Console
- Проверка порта: `lsof -i :8765`
- Проверка HTTP: `curl http://127.0.0.1:8766/health`

## Безопасность и ограничения

- Это прототип, а не production-решение. DOM ChatGPT меняется, расширение может потребовать обновления селекторов.
- Расширение управляет вашей сессией ChatGPT. Устанавливайте только если доверяете коду.
- Автоматизация веб-интерфейса ChatGPT может нарушать Terms of Service OpenAI. Используйте на свой страх и риск.
- Поддерживается один браузерный клиент; при нескольких вкладках используется последняя подключившаяся.

## Разработка

```bash
cargo build          # сборка
cargo test           # тесты
cargo clippy --tests -- -D warnings  # линтер
cargo run -- --help  # запуск
```

## Лицензия

MIT
