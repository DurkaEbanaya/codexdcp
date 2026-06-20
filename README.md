# CodexDCP — Codex Developer Chaos Platform

MCP-коннектор, который превращает браузерный ChatGPT в Codex-подобного агента для OpenCode. Rust MCP-сервер через stdio + Chrome DevTools Protocol (CDP) управляет headless Chrome, который работает с веб-интерфейсом ChatGPT. Включает OpenAI-совместимый HTTP-провайдер.

## Архитектура

```
┌─────────────┐      stdio (MCP)     ┌──────────────────────┐     CDP WebSocket     ┌─────────────────────┐
│   OpenCode    │  ──────────────────> │      CodexDCP         │  <────────────────>  │  Headless Chrome    │
│  (MCP client) │                      │  (Rust MCP server)    │   Runtime.evaluate   │  (Brave/Chromium)   │
└─────────────┘                        └──────────────────────┘                      └─────────────────────┘
                                               │                                              │
                                               │ HTTP (OpenAI API)                            │ DOM
                                               ▼                                              ▼
                                      ┌──────────────────────┐                   ┌─────────────────────┐
                                      │  HTTP provider :8766  │                   │   chatgpt.com       │
                                      └──────────────────────┘                   └─────────────────────┘
```

- **Rust MCP server** — реализует протокол MCP через stdio, предоставляет инструменты `chatgpt_coder`, `chatgpt_ask`.
- **CDP bridge** — внутри сервера, запускает headless Chrome, подключается через DevTools Protocol, выполняет JS через `Runtime.evaluate`.
- **HTTP provider** — OpenAI-совместимый API на порту 8766 (`/v1/chat/completions`, `/v1/models`, `/health`), поддерживает streaming (SSE).
- **Headless Chrome** — запускается как child-процесс, использует persistent profile для переиспользования cookies/login сессии.

## Возможности

- **Headless Chrome** — браузер работает в фоне, не отвлекает пользователя
- **Временный чат** — все запросы идут в temporary chat (история не сохраняется)
- **Cookie reuse** — профиль Brave/Chrome копируется один раз, логин не требуется
- **Anti-detection** — `--disable-blink-features=AutomationControlled` обходит Cloudflare
- **MCP-инструменты** — делегируй задачи по коду и вопросы в ChatGPT прямо из OpenCode
- **HTTP-провайдер** — используй ChatGPT как модель-провайдер (OpenAI-совместимый API)
- **Выбор модели** — передавай `model: "GPT-4o"`, `"o1"`, и т.д.
- **Сохранение markdown** — ответы сохраняют code blocks, заголовки, ссылки, таблицы
- **Стриминг** — частичные ответы через SSE для HTTP-провайдера
- **Ретраи с backoff** — автоматический повтор при временных ошибках
- **Кастомный system prompt** — через CLI-флаг или env-переменную
- **Graceful shutdown** — Ctrl+C корректно останавливает Chrome и сервер

## Требования

- Rust >= 1.85 (edition 2024; проверено на 1.96)
- Chrome, Brave или Chromium браузер
- Аккаунт ChatGPT (активная сессия в браузере для копирования cookies)
- OpenCode с поддержкой MCP

## Установка

### Шаг 1. Получить бинарник

**Вариант A — из исходников:**

```bash
git clone https://github.com/DurkaEbanaya/codexdcp.git
cd codexdcp
cargo build --release
# Бинарник: target/release/codexdcp
```

**Вариант B — предсобранный:**

Скачайте со страницы [Releases](https://github.com/DurkaEbanaya/codexdcp/releases).

### Шаг 2. Скопировать cookies из браузера

CodexDCP использует отдельный профиль Chrome, в который нужно один раз скопировать cookies из вашего браузера:

```bash
# Создаём профиль
mkdir -p ~/.codexdcp/chrome-profile/Default

# Копируем cookies и localStorage из Brave
BRAVE="$HOME/Library/Application Support/BraveSoftware/Brave-Browser/Default"
cp "$BRAVE/Cookies" ~/.codexdcp/chrome-profile/Default/
cp "$BRAVE/Login Data" ~/.codexdcp/chrome-profile/Default/ 2>/dev/null
cp -r "$BRAVE/Local Storage" ~/.codexdcp/chrome-profile/Default/ 2>/dev/null
cp "$BRAVE/Preferences" ~/.codexdcp/chrome-profile/Default/ 2>/dev/null

# Для Chrome вместо Brave:
# CHROME="$HOME/Library/Application Support/Google/Chrome/Default"
```

Альтернатива — запустить один раз с `--visible`, залогиниться вручную и закрыть:

```bash
./target/release/codexdcp --visible --chrome-path "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
```

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
        "--cdp-port", "9222",
        "--chrome-path", "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
      ],
      "enabled": true
    }
  }
}
```

Укажите путь к вашему браузеру через `--chrome-path`. Если не указать, бинарник попытается автоопределить.

### Шаг 4. (Опционально) Подключить как провайдер моделей

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

### Шаг 5. (Опционально) Slash-команды

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
    }
  }
}
```

## Инструменты MCP

| Инструмент | Описание | Параметры |
|---|---|---|
| `chatgpt_coder` | Делегировать задачу по коду (Codex-style) | `task`, `context`, `language`, `model`, `format` |
| `chatgpt_ask` | Задать общий вопрос | `prompt`, `model`, `format` |

Временный чат включается автоматически при каждой отправке запроса — история не сохраняется.

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
# → {"status":"ok","connected":true,"mode":"temporary_chat"}
```

## CLI-флаги

```
--http-host        127.0.0.1    хост HTTP-провайдера
--http-port        0            порт HTTP (0 = выключен)
--default-timeout  120          таймаут ответа ChatGPT (сек)
--system-prompt    -            кастомный системный промпт
--max-retries      2            кол-во ретраев
--retry-delay-ms   2000         начальная задержка ретрая (мс)
--chrome-path      -            путь к Chrome/Brave/Chromium
--chrome-profile   ~/.codexdcp/chrome-profile  путь к user-data-dir
--headless         true         headless режим Chrome
--cdp-port         9222         порт DevTools Protocol
--visible                       запустить Chrome с видимым окном (для логина)
--selectors-path   -            путь к кастомным селекторам
--log-level        info         уровень логирования
```

Все флаги также доступны через env-переменные (`CODEXDCP_*` или `CHATGPT_BRIDGE_*`).

## Отладка

- Бинарник с debug-логами: `codexdcp --log-level debug`
- Видимый Chrome для дебага: `codexdcp --visible --log-level debug`
- Проверка CDP: `curl http://127.0.0.1:9222/json/version`
- Проверка HTTP: `curl http://127.0.0.1:8766/health`
- Логи Chrome наследуются в stderr codexdcp

## Безопасность и ограничения

- DOM ChatGPT может меняться — селекторы обновляются в `src/js.rs` (`DEFAULT_SELECTORS`).
- Профиль Chrome содержит cookies с доступом к ChatGPT. Не коммитьте `~/.codexdcp/chrome-profile/`.
- Автоматизация веб-интерфейса ChatGPT может нарушать Terms of Service OpenAI. Используйте на свой страх и риск.
- Headless Chrome запускается как child-процесс и завершается при остановке сервера.

## Разработка

```bash
cargo build --release                      # сборка
cargo test                                 # тесты
cargo clippy --tests -- -D warnings        # линтер
cargo run -- --help                        # помощь
cargo run -- --visible --log-level debug   # запуск с видимым Chrome
```

## Лицензия

MIT
