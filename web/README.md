# Polycopier Web UI

React + TypeScript + Vite dashboard for Polycopier. It is served by the Rust backend; see
[`docs/WEB_UI.md`](../docs/WEB_UI.md) for features and the security model.

```bash
npm ci
npm run lint    # ESLint
npm run build   # type-check + build to web/dist (served by `cargo run -- --ui`)
```

* `src/api.ts` — typed API client and DTOs mirroring `src/api.rs`; attaches the per-run
  access token (`?token=` in the URL → `x-polycopier-token` header) to every request.
* `src/App.tsx` — dashboard; `src/SettingsManager.tsx` — settings; `src/SetupWizard.tsx` —
  first-run setup.

`npm run dev` works for UI-only work, but API calls need the backend on port 3000 and the
Vite dev origin is rejected by the backend's Origin check; prefer `npm run build` + `--ui`.
