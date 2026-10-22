# Contributing

## Build and run

The supported target is Windows x64. Install Node.js 24 or later, pnpm 11, the Rust MSVC toolchain, Visual Studio C++ Build Tools, and WebView2 Runtime. ffprobe is optional for development and needed for live media probing.

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` previews the interface in a browser; real filesystem, SQLite, and provider commands require Tauri. To isolate application data, set `BANGUMI_MANAGER_DATA_DIR` to a separate absolute directory before starting the application. Never point automated tests at a personal media library.

## Validation and releases

```powershell
pnpm check
pnpm test
pnpm test:core
pnpm test:e2e --workers=4
```

Browser tests use Microsoft Edge and mocked Tauri commands. Rust tests use temporary directories for real file operations. If your environment configures a proxy, include `localhost` and `127.0.0.1` in `NO_PROXY` so local mock-server tests can connect directly. Tests requiring a live provider, SMB share, or ffprobe are opt-in; read the corresponding test's environment variables before enabling it. Do not commit credentials, real media titles, or provider responses containing personal library information.

Before a release build, increment the application version consistently in `package.json`, `src-tauri/Cargo.toml`, the application's entry in `src-tauri/Cargo.lock`, and any explicit version in `src-tauri/tauri.conf.json` (the default configuration reads `package.json`).

```powershell
pnpm notices
pnpm tauri build --no-bundle
```

The portable executable is produced under `src-tauri/target/release/`. Distribute it with `LICENSE` and `THIRD_PARTY_NOTICES.txt`, and provide the corresponding source under the project's license. Omit `--no-bundle` to build the configured Windows installer. Regenerate third-party notices whenever dependency locks change. The notice generator inventories installed frontend dependencies and the Windows Rust dependency graph, including build/test components.

### Automated GitHub releases

Push a `v<version>` tag matching the committed application version, for example `git tag v0.3.0` followed by `git push origin v0.3.0`. The Release workflow validates version consistency, runs the Windows checks, then builds the Windows x64 NSIS installer and a portable ZIP containing the executable and license notices. Both downloads and `SHA256SUMS.txt` are uploaded before the draft release becomes public.

For a failed run, use **Re-run failed jobs**, or manually run **Release** against the same tag. Upload failures leave a draft that can be retried. Published assets are never overwritten; publish a new version for subsequent changes. Release publishing uses the built-in `GITHUB_TOKEN` with `contents: write` only in the release job. No personal token is required. Windows code signing is not configured.

## Frontend structure

The frontend is organized by responsibility:

```text
src/
  main.tsx           React entry point
  app/               Application shell, navigation, and workflow coordination
  features/
    library/         Title catalog, seasons, and existing resource associations
    metadata/        Provider search, title import, and NFO synchronization
    organize/        Source selection, episode matching, and task creation
    resources/       Resource filters, replacement, renaming, and backups
    settings/        Preferences and timing diagnostics
    tasks/           Task queue, review, and execution status
  components/        Shared controls and media displays
    ui/              shadcn/ui primitives
  hooks/             Reusable React hooks
  lib/               IPC, runtime defaults, formatting, and media helpers
  types/             Shared data contracts, grouped by domain
  i18n/              Language state, catalogs, and diagnostic translations
  styles/            Theme and global layout
```

Keep feature-specific components, helpers, styles, and unit tests together. Use relative imports within a feature and the `@/` alias across directories. The application shell coordinates features; shared components, hooks, and utilities should not depend on feature pages. The `types` entry point only exports types; runtime defaults live in `lib/defaults.ts`. Avoid mixed utility files or compatibility re-exports for moved modules.

Browser workflows remain under `e2e/`. Unit tests are discovered recursively under `src/`. The shadcn configuration points to `src/styles/theme.css`; shared layout remains in `src/styles/global.css`, and feature styles stay with their components.

## Interface and translations

Reuse the existing shadcn/ui components in `src/components/ui/` and the composed controls in `src/components/`. Preserve keyboard access and verify long English text at the minimum window width as well as the normal desktop size.

Interface messages live in `src/i18n/zh-CN.json` and `src/i18n/en.json`. Use the typed `t()` helper and add the same key and interpolation parameters to both catalogs. Translate complete sentences, keeping interpolated filenames, paths, titles, and user-entered metadata unchanged. English counts can use `{count} {count|file|files}` for singular/plural agreement. For static label maps, use getters or functions so changing language updates them without reloading the module.

`src/i18n/backend.en.json` provides compatibility translations for existing persisted backend diagnostic messages. Add complete diagnostic templates only; do not run this translation over metadata or arbitrary record fields. Unknown system diagnostics remain visible in their original form. Language persistence, catalog completeness, interpolation, and representative English workflows are covered by tests.

The interface language is local to the WebView and independent of provider metadata preferences. Do not localize persisted naming rules, folder names, category values, or provider identifiers when switching the interface language.

## Changes and reports

Use fictional or anonymized media names in code, tests, screenshots, and examples. Keep changes focused and include the relevant validation results in a pull request. For user-visible changes, describe the behavior and any migration or recovery implications. Review diffs for tokens, local account names, absolute development paths, and generated application data before committing.

Report security concerns using the process in [SECURITY.md](SECURITY.md). Contributions are distributed under the project's [GPL-3.0-only license](LICENSE).
