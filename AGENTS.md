# Repository Guidelines

## Project Overview

Frpc-Desktop is a cross-platform Tauri 2 desktop application. The renderer uses Vue 3, TypeScript, Vite, Pinia, Vue Router, Element Plus, and vue-i18n. The Tauri (Rust) backend manages frpc processes, local persistence (SQLite), downloads, system integration, and IPC commands.

Use Node.js 22.12 or newer, npm, and a stable Rust toolchain. Keep changes focused; do not edit generated output or downloaded dependencies.

## Repository Layout

- `src/`: Vue renderer application.
  - `views/`: route-level screens.
  - `components/`: shared UI components.
  - `store/`: Pinia stores and renderer-side application state.
  - `lang/`: English and Simplified Chinese translations.
  - `ipc/router.ts`: renderer-side IPC route table (paths are the IPC contract).
  - `utils/ipcUtils.ts`: renderer IPC helpers (Tauri invoke/listen wrappers).
- `src-tauri/`: Rust backend (replaces the former `electron/` directory).
  - `src/main.rs`, `src/lib.rs`, `src/app.rs`: Tauri startup, window, tray, listeners, lifecycle.
  - `src/ipc/`: `#[tauri::command]` handlers (port of the former controllers) and route map.
  - `src/service/`: business logic (frpc process, versions, TOML generation, logs, system).
  - `src/db/`: SQLite (rusqlite) repositories, migrations runner, NeDB importer.
  - `src/model/`: serde structs mirroring `types/` (camelCase JSON contract).
  - `src/core/`: constants, business errors, `ApiResponse` wrapper, paths, logger.
  - `migrations/`: SQL migration files (embedded at compile time).
  - `src/json/`: embedded frp release / checksum JSON.
  - `tests/`: Rust integration tests.
- `types/`: global TypeScript declarations shared with the renderer.
- `public/`: packaged static assets and platform icons.
- `screenshots/`: README assets; do not update unless documentation visuals change.
- `dist/`, `release/`, `src-tauri/target/`, and `node_modules/`: generated content; never hand-edit or commit newly generated files unless explicitly requested.

## Common Commands

```sh
npm ci
npm run dev:tauri
npm run lint
npm run build
npm run build:tauri
```

- `npm run dev:tauri` launches the Vite dev server and the Tauri app (requires the Rust toolchain).
- `npm run build` runs `vue-tsc --noEmit` before the Vite production build.
- `npm run build:tauri` builds the renderer and bundles the desktop app.
- Rust checks live in `src-tauri/`: `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`.

There is currently no automated frontend test script. For normal code changes, run lint and build; for Rust changes run `cargo test`. For UI or backend behavior changes, also exercise the affected workflow in the development app and report what was checked.

## Architecture and Change Conventions

- Keep renderer code concerned with presentation and state. Filesystem, process, network, database, and OS behavior belong under `src-tauri/`.
- Follow the existing flow: renderer -> `invoke(command, args)` -> `#[tauri::command]` -> service/repository. Commands return `ApiResponse { bizCode, data, message }` (bizCode `A1000` = success), identical to the former Electron `ResponseUtils`.
- When adding IPC behavior, update all participating pieces: the command in `src-tauri/src/ipc/commands.rs`, the route entry in `src/ipc/router.ts` (path strings are the stable contract; `command` is the Rust command name), the service implementation, and renderer listeners or sends.
- Push events use Tauri `emit`/`listen` on stable channel names (`frpcProcess:watchFrpcLog`, `system:watchSystemUsage`, `version:downloadProgress`).
- Put shared global interfaces in `types/`; avoid duplicating cross-process payload shapes in Vue components. Rust `src/model/` structs must use the same camelCase JSON field names as the TS types.
- User-facing text must support both `src/lang/en-US.ts` and `src/lang/zh-CN.ts`. Preserve existing terminology for frp/frpc concepts.
- Preserve the current formatting style: two-space indentation, double quotes, semicolons, trailing commas only where Prettier adds them, and TypeScript/Vue conventions enforced by ESLint and Prettier. Rust code follows `cargo fmt` and clippy defaults.
- Use the `@/` alias for renderer imports. Rust code uses crate-relative imports.
- Do not introduce unrelated refactors, broad formatting churn, or dependency upgrades as part of a focused fix.

## Data and Security

- Treat configuration, tokens, proxy definitions, logs, and local paths as sensitive. Do not log secrets or include real credentials in fixtures, screenshots, or examples.
- Preserve compatibility with existing SQLite data (`frpc-desktop.sqlite3`) and frpc configuration formats. Schema or filename changes require an explicit migration or backward-compatible fallback.
- Critical path constants must stay unchanged: user data dir `%APPDATA%/Frpc-Desktop` (via `PathUtils::get_app_data`), `md5("frpc") = d9ecf567b6988bca88c46720024e12d0`, `md5("frpc-log") = 71ae86cb0cda76922533992da4fc0fa8`.
- Validate renderer-provided IPC arguments in the Rust backend before using them in paths, shell operations, downloads, or process commands.

## Documentation and Commits

The canonical frontend UI development standard is `docs/FRONTEND_UI_STANDARDS.md`. Read and follow it before creating or changing renderer UI, layout, styling, interaction states, icons, or user-facing copy.

The canonical database design and migration document is `docs/DATABASES.md`. Read and follow it before changing persistence models, SQLite schema or migrations, repositories, database paths, or data compatibility behavior.

Update both `README.md` and `README.zh_CN.md` when changing user-facing setup or behavior. Keep commit subjects short and aligned with the repository's conventional emoji-prefixed history when practical (for example, `🐛 Fix ...`, `✨ Add ...`, or `🔧 Update ...`). Do not commit build artifacts or local application data.
