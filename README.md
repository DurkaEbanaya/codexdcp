# CodexDCP — Codex Developer Chaos Platform

MCP-сервер на Rust, который совмещает браузерный ChatGPT и полноценную работу с репозиторием. Управляет headless Chrome через Chrome DevTools Protocol (CDP) для доступа к ChatGPT. Предоставляет 16 MCP-инструментов: ChatGPT bridge, filesystem, git, bash, handoff, skills. Включает OpenAI-совместимый HTTP-провайдер.

## Архитектура

```
┌─────────────┐      stdio (MCP)     ┌──────────────────────┐     CDP WebSocket     ┌─────────────────────┐
│   OpenCode    │  ──────────────────> │      CodexDCP         │  <────────────────>  │  Headless Chrome    │
│  (MCP client) │                      │  (Rust MCP server)    │   Runtime.evaluate   │  (Brave/Chromium)   │
└─────────────┘                        └──────────────────────┘                      └─────────────────────┘
                                                │                                              │
                          ┌─────────────────────┼─────────────────────┐                      │ DOM
                          │                     │ HTTP (OpenAI API)    │                      ▼
                          │                     ▼                      │              ┌─────────────────────┐
                  ┌──────────────┐     ┌──────────────────────┐        │              │   chatgpt.com       │
                  │  FS / Git /   │     │  HTTP provider :8766  │────────┘              └─────────────────────┘
                  │  Bash / Skill │     │  /v1/chat/completions │
                  └──────────────┘     └──────────────────────┘
```

- **Rust MCP server** — 16 инструментов через stdio: ChatGPT bridge + filesystem + git + bash + handoff + skills
- **CDP bridge** — запускает headless Chrome, подключается через DevTools Protocol, выполняет JS через `Runtime.evaluate`
- **HTTP provider** — OpenAI-совместимый API на порту 8766 (`/v1/chat/completions`, `/v1/models`, `/health`), поддерживает streaming (SSE)
- **Workspace tools** — прямая работа с файлами, git, shell командами в рабочей директории (без Chrome)

## Возможности

### ChatGPT bridge
- **Headless Chrome** — браузер работает в фоне, не отвлекает пользователя
- **Временный чат** — все запросы идут в temporary chat через `?temporary-chat=true` (история не сохраняется)
- **Cookie reuse** — профиль Brave/Chrome копируется один раз, логин не требуется
- **Anti-detection** — `--disable-blink-features=AutomationControlled` обходит Cloudflare
- **Выбор модели** — передавай `model: "GPT-4o"`, `"o1"`, и т.д.
- **Сохранение markdown** — ответы сохраняют code blocks, заголовки, ссылки, таблицы
- **Стриминг** — частичные ответы через SSE для HTTP-провайдера
- **Ретраи с backoff** — автоматический повтор при временных ошибках
- **Bridge readiness** — MCP tool calls ждут готовности bridge (до 60 сек) вместо немедленного таймаута

### Workspace tools (гибридный режим)
- **Filesystem** — `read_file`, `write_file`, `edit_file`, `tree` с path containment проверкой
- **Search** — `search_files` через ripgrep (с grep fallback)
- **Bash** — `bash` с safe allowlist (блокирует rm -rf, git push, curl, sudo)
- **Git** — `git_status`, `git_diff`, `show_changes`
- **Handoff** — `.ai-bridge/` с планами для Codex/OpenCode/Pi (`read_handoff`, `handoff_to_agent`)
- **Skills** — discovery и загрузка `SKILL.md` файлов (`load_skill`, `list_skills`)
- **Context** — `codex_context` (AGENTS.md chain + git state), `export_pro_context`
- **Tiered tool surface** — `--tool-mode minimal|standard|full` управляет доступными инструментами
- **Безопасность** — blocked globs (.git, .env, *.pem, *.key), write mode toggle, bash allowlist

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
  // ВАЖНО: mcp_timeout должен быть достаточно большим (120000 мс = 2 мин).
  // ChatGPT tool calls требуют 30-90 секунд: загрузка Chrome (~15 сек) +
  // навигация на temp chat (~10 сек) + ответ модели (~10-60 сек).
  // Без этой настройки OpenCode использует дефолтный таймаут (5 сек)
  // и tool calls падают с "MCP error -32001: Request timed out".
  // Filesystem/git/bash инструменты отвечают мгновенно — 120 сек это максимум, не минимум.
  "experimental": {
    "mcp_timeout": 120000
  },
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

### ChatGPT bridge (всегда доступны)

| Инструмент | Описание | Параметры |
|---|---|---|
| `chatgpt_coder` | Делегировать задачу по коду (Codex-style) | `task`, `context`, `language`, `model`, `format` |
| `chatgpt_ask` | Задать общий вопрос | `prompt`, `model`, `format` |

### Filesystem (minimal+)

| Инструмент | Описание | Параметры |
|---|---|---|
| `read_file` | Читать файл с номерами строк | `path`, `offset`, `limit` |
| `write_file` | Создать/перезаписать файл | `path`, `content` |
| `edit_file` | Точная замена строки в файле | `path`, `old_string`, `new_string` |
| `tree` | Дерево директорий | `path`, `max_depth` |
| `bash` | Shell команда (safe allowlist) | `command` |

### Search (standard+)

| Инструмент | Описание | Параметры |
|---|---|---|
| `search_files` | Regex-поиск через ripgrep/grep | `pattern`, `path`, `include` |

### Git (standard+)

| Инструмент | Описание | Параметры |
|---|---|---|
| `git_status` | Git status (short format) | — |
| `git_diff` | Git diff (--stat) | `staged` |
| `show_changes` | Сводка всех изменений | — |

### Skills (standard+)

| Инструмент | Описание | Параметры |
|---|---|---|
| `load_skill` | Загрузить SKILL.md по имени | `name` |
| `list_skills` | Список всех навыков | — |

### Handoff (standard+)

| Инструмент | Описание | Параметры |
|---|---|---|
| `read_handoff` | Читать `.ai-bridge/` директорию | — |
| `handoff_to_agent` | Записать план для агента | `plan`, `agent`, `model` |

### Context (full only)

| Инструмент | Описание | Параметры |
|---|---|---|
| `codex_context` | AGENTS.md chain + git state | — |
| `export_pro_context` | Экспорт контекста в `.ai-bridge/pro-context.md` | — |

### Режимы инструментов (`--tool-mode`)

| Режим | Инструменты |
|---|---|
| `minimal` | ChatGPT + filesystem + bash |
| `standard` | + search, git, skills, handoff |
| `full` | + context tools |

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
--max-retries      3            кол-во ретраев
--retry-delay-ms   2000         начальная задержка ретрая (мс)
--chrome-path      -            путь к Chrome/Brave/Chromium
--chrome-profile   ~/.codexdcp/chrome-profile  путь к user-data-dir
--headless         true         headless режим Chrome
--cdp-port         9222         порт DevTools Protocol
--visible                       запустить Chrome с видимым окном (для логина)
--selectors-path   -            путь к кастомным селекторам
--log-level        info         уровень логирования
--http-only                     только HTTP-провайдер (без MCP stdio)
--root             CWD          корень workspace для filesystem/git/bash
--tool-mode        standard     minimal | standard | full
--bash-mode        safe         safe | off | full
--write-mode       workspace    workspace | off
```

Все флаги также доступны через env-переменные (`CODEXDCP_*` или `CHATGPT_BRIDGE_*`).

## Безопасность

- **Path containment** — все пути проверяются относительно workspace root; выход за пределы блокируется
- **Blocked globs** — `.git/`, `.env`, `*.pem`, `*.key` и др. недоступны для чтения/записи
- **Bash allowlist** — в safe режиме (по умолчанию) выполняются только build/test/lint/git-inspect команды; `rm -rf`, `git push`, `curl`, `sudo` и др. блокируются
- **Write mode** — `--write-mode off` переводит все инструменты в read-only
- **DOM селекторы** — ChatGPT DOM может меняться, селекторы обновляются в `src/js.rs` (`DEFAULT_SELECTORS`), переопределяются через `--selectors-path`
- **Профиль Chrome** — содержит cookies с доступом к ChatGPT. Не коммитьте `~/.codexdcp/chrome-profile/`
- **Terms of Service** — автоматизация веб-интерфейса ChatGPT может нарушать ToS OpenAI. Используйте на свой страх и риск

## Отладка

- Бинарник с debug-логами: `codexdcp --log-level debug`
- Видимый Chrome для дебага: `codexdcp --visible --log-level debug`
- Только HTTP (без MCP): `codexdcp --http-only --http-port 8766 --log-level debug`
- Проверка CDP: `curl http://127.0.0.1:9222/json/version`
- Проверка HTTP: `curl http://127.0.0.1:8766/health`
- Логи Chrome наследуются в stderr codexdcp

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
