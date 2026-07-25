# S104 — Server Admin Template Extraction

**Wave: UX-QoL · Depends on:** S73 (Server ops surface, admin dashboard)

## Outcome

The admin dashboard HTML is extracted from a Rust format string into a proper template file. The dashboard auto-refreshes via a JavaScript `fetch()` loop instead of `<meta http-equiv="refresh">` (which causes full-page flash). The HTML is served from a template renderer, not embedded in source.

## Context

`admin.rs:310-414` contains a 300+ line `render_dashboard_html()` function that builds an HTML string via `format!()`. This is:
- **Unmaintainable** — HTML, CSS, and JS embedded in Rust strings
- **Unsafe** — no CSP headers, inline styles everywhere
- **Ugly UX** — `<meta http-equiv="refresh" content="15">` causes full page reload, flashing the page white

### Key files

| File | Role |
|------|------|
| `reachlock-server/src/ws/admin.rs` | Admin routes + dashboard HTML (lines 310-454) |
| `reachlock-server/Cargo.toml` | Add template dependency |
| `reachlock-server/templates/` | New directory — template files |

## Freeze first

### Template engine choice

Use **`askama`** — compile-time template checking, Rust-native, no runtime overhead:

```toml
# reachlock-server/Cargo.toml
askama = "0.12"
```

### Template file

Create `reachlock-server/templates/dashboard.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>ReachLock Admin</title>
  <style>
    /* Same CSS as current, extracted from Rust string */
    :root {
      --bg: #0d1117;
      --text: #c9d1d9;
      --accent: #58a6ff;
      --card-bg: #161b22;
      --border: #30363d;
    }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      background: var(--bg);
      color: var(--text);
      font-family: -apple-system, BlinkMacSystemFont, sans-serif;
      padding: 2rem;
    }
    h1 { color: var(--accent); margin-bottom: 1.5rem; }
    .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 1rem; margin-bottom: 2rem; }
    .card { background: var(--card-bg); border: 1px solid var(--border); border-radius: 6px; padding: 1rem; }
    .card h3 { color: #8b949e; font-size: 0.8rem; text-transform: uppercase; margin-bottom: 0.5rem; }
    .card .value { font-size: 1.8rem; font-weight: 600; color: #f0f6fc; }
    table { width: 100%; border-collapse: collapse; margin-bottom: 2rem; }
    th, td { text-align: left; padding: 0.5rem; border-bottom: 1px solid var(--border); }
    .spinner { display: none; text-align: center; padding: 1rem; color: var(--accent); }
    .stale .spinner { display: block; }
    .stale .cards, .stale table { opacity: 0.5; }
  </style>
</head>
<body>
  <h1>ReachLock Dashboard</h1>
  <nav>
    <a href="/admin/dashboard?key={{ key }}">Dashboard</a>
    <a href="/admin/players?key={{ key }}">Players</a>
    <a href="/admin/audit?key={{ key }}">Audit Log</a>
  </nav>
  <div id="content">
    {% include "dashboard_cards.html" %}
  </div>
  <div class="spinner">Refreshing…</div>
  <script>
    // Auto-refresh every 15s via fetch, replacing only the #content div
    // (no full page reload, no flash)
    const KEY = "{{ key }}";
    let stale = false;
    setInterval(async () => {
      stale = true;
      document.body.classList.add('stale');
      try {
        const resp = await fetch(`/admin/dashboard?key=${KEY}&partial=1`);
        const html = await resp.text();
        document.getElementById('content').innerHTML = html;
        document.body.classList.remove('stale');
      } catch(e) {
        // Keep showing stale data rather than blanking
        document.body.classList.remove('stale');
      }
    }, 15000);
  </script>
</body>
</html>
```

### Askama template struct

```rust
use askama::Template;

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    key: String,
    connected: usize,
    active_sessions: usize,
    uptime_hms: String,
    admin_configured: bool,
    db_status: String,
    health_rows: Vec<HealthRow>,
}

struct HealthRow {
    name: String,
    status: String,
    color: String,
}
```

### Partial template for cards (dashboard_cards.html)

```html
<div class="cards">
  <div class="card"><h3>Connected</h3><div class="value">{{ connected }}</div></div>
  <div class="card"><h3>Sessions</h3><div class="value">{{ active_sessions }}</div></div>
  <div class="card"><h3>Uptime</h3><div class="value">{{ uptime_hms }}</div></div>
  <div class="card"><h3>Admin Key</h3><div class="value">{{ admin_status }}</div></div>
</div>
<section>
  <h2>Database</h2>
  <p>{{ db_status }}</p>
</section>
<section>
  <h2>Health Checks</h2>
  <table>
    <thead><tr><th>Check</th><th>Status</th></tr></thead>
    <tbody>
      {% for row in health_rows %}
      <tr><td>{{ row.name }}</td><td style="color:{{ row.color }}">{{ row.status }}</td></tr>
      {% endfor %}
    </tbody>
  </table>
</section>
```

## Deliverables

### 1. Add askama dependency

- [ ] Add `askama = "0.12"` to `reachlock-server/Cargo.toml`
- [ ] Configure template directory: `askama` default is `templates/` relative to crate root

### 2. Create template files

- [ ] Create `reachlock-server/templates/dashboard.html` — main dashboard template
- [ ] Create `reachlock-server/templates/dashboard_cards.html` — partial for the cards+health section
- [ ] Extract all CSS from the Rust `format!()` string into the `<style>` block
- [ ] Add `--bg`, `--text`, `--accent` CSS custom properties for future dark/light theme support

### 3. Create Askama template structs

- [ ] Add `DashboardTemplate` struct with all fields
- [ ] Add `DashboardCardsTemplate` struct for the partial view

### 4. Replace `render_dashboard_html()` with template rendering

- [ ] In `admin_dashboard` handler:
  - Build `DashboardTemplate` from `AppState`
  - Call `.render().unwrap()` and return the HTML
- [ ] Handle `?partial=1` query param: return only the `dashboard_cards.html` partial (for JS fetch)

### 5. Add Content-Security-Policy header

- [ ] Add `Content-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'` response header
- [ ] Note: `'unsafe-inline'` for style/script is acceptable for an internal admin dashboard

### 6. Remove the old `render_dashboard_html()` function

- [ ] Delete lines 310-414 of `admin.rs` (the `format!()` block)

## Acceptance gates

```bash
cargo build -p reachlock-server
cargo clippy -p reachlock-server -- -D warnings

# Manual:
# 1. Set REACHLOCK_ADMIN_KEY=test123
# 2. Start server: cargo run -p reachlock-server
# 3. Open http://localhost:40711/admin/dashboard?key=test123
# 4. Verify dashboard renders with cards, health table, uptime
# 5. Wait 15s → page auto-refreshes without flash (JS fetch replaces content)

make check
```

## Non-goals

- Full SPA admin dashboard (this is a template extraction, not a rewrite)
- Authentication UI (the `?key=` query param stays for now)
- Admin dashboard login page
- Charts or graphs for metrics
- Player management UI (HTTP API only)
- WebSocket push for live updates (JS fetch loop is sufficient)

## Gotchas

- **`askama` requires templates at compile time.** The `templates/` directory must exist at build time. Add `templates/` to the Cargo package includes in `Cargo.toml` if needed (askama auto-discovers it).
- **`askama` uses `#[template(path = "...")]` — paths are relative to `templates/`.** The struct's `Template` derive macro reads the file at compile time and embeds it. No runtime file I/O.
- **Template render can panic on missing variables.** If a template field doesn't exist on the struct, compilation fails (not runtime). This is the main reason to use `askama` — compile-time safety.
- **`?partial=1` as a query param.** Simple flag-based partial rendering avoids needing an `Accept: application/json` header or separate endpoint. The handler checks for `params.get("partial")` and renders the partial template instead of the full page.
- **Don't remove the `/admin/log-level` and other endpoints.** This sprint only touches the dashboard HTML rendering. All other admin endpoints stay exactly as-is.
