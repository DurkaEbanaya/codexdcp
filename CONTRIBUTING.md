# Contributing to CodexDCP

Спасибо за интерес к проекту! Вот как внести вклад:

## Разработка

```bash
git clone https://github.com/anomalyco/codexdcp.git
cd codexdcp
cargo build
cargo test
cargo clippy --tests -- -D warnings
```

## Структура проекта

- `src/` — Rust MCP server, WebSocket bridge, HTTP provider
- `browser_extension/` — Chrome/Edge extension (JS, JSON)
- `.github/workflows/` — CI и release pipelines

## Правила

1. **Код** — без `unwrap()` в production-коде (clippy warn). Используйте `?` или явную обработку.
2. **Логи** — только в stderr. stdout зарезервирован для MCP JSON-RPC.
3. **Селекторы** — DOM-селекторы ChatGPT должны быть в `browser_extension/selectors.json`, не в Rust-коде.
4. **Тесты** — новые фичи должны сопровождаться тестами в `src/bridge.rs` (модуль `tests`).
5. **Коммиты** — используйте [conventional commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `refactor:`.

## Process

1. Fork репозитория
2. Создайте ветку: `git checkout -b feat/my-feature`
3. Коммитьте: `git commit -m 'feat: add awesome thing'`
4. Push: `git push origin feat/my-feature`
5. Откройте Pull Request

## Обновление селекторов

Когда ChatGPT меняет DOM-структуру:

1. Откройте `chatgpt.com`, найдите новые селекторы через DevTools
2. Обновите `browser_extension/selectors.json`
3. Перезагрузите расширение в `chrome://extensions`
4. Протестируйте через `chatgpt_status` и простой вопрос

Не нужно править `background.js` или Rust-код — только JSON.
