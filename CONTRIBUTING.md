# Contributing to CodexDCP

Спасибо за интерес к проекту! Вот как внести вклад:

## Разработка

```bash
git clone https://github.com/DurkaEbanaya/codexdcp.git
cd codexdcp
cargo build
cargo test
cargo clippy --tests -- -D warnings
```

## Структура проекта

- `src/` — Rust MCP server, CDP bridge, HTTP provider, JS injection strings
- `.github/workflows/` — CI и release pipelines

## Правила

1. **Код** — без `unwrap()` в production-коде (clippy warn). Используйте `?` или явную обработку.
2. **Логи** — только в stderr. stdout зарезервирован для MCP JSON-RPC.
3. **Селекторы** — DOM-селекторы ChatGPT живут в `DEFAULT_SELECTORS` константе в `src/js.rs`, обновляются через `--selectors-path` без правки кода.
4. **Тесты** — новые фичи должны сопровождаться тестами.
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
2. Обновите `DEFAULT_SELECTORS` в `src/js.rs` (или создайте JSON-файл и передайте через `--selectors-path`)
3. Протестируйте: `cargo run -- --visible --log-level debug` и задайте простой вопрос через `chatgpt_ask`
