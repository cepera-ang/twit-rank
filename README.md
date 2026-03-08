# twit-rank

Local X archiver and search UI.

## Warning

- This project is vibe-coded. Treat it as an experimental personal tool, not a hardened production system.
- It requires real X account credentials (`auth_token` and `ct0`) from your own logged-in session to work.
- Using your own credentials with an unofficial client may violate X/Twitter expectations or terms and could trigger rate limits, session invalidation, account friction, or other enforcement. Use it at your own risk.

## Project goals

Current release goal:
- archive X timelines into a local SQLite database
- browse and search the local archive through a small embedded web UI
- support lightweight per-post feedback for future ranking work

Longer-term goal:
- become a customizable X reader / processor with stronger ranking, filtering, and personal feed tooling

The current product is primarily an archiver plus search interface. The ranking module still exists in the codebase, but it is not an active user-facing feature yet.

## What this is

- A local archiver for the X `Following` and `For you` timelines, plus any optional lists you add, using your own logged-in web session.
- A local web UI for browsing and searching what has already been archived.

## What this is not

- Not an official X/Twitter client.
- Not a general-purpose social app replacement.
- Not a live X search client. The `Search` page searches your local archive, not X itself.
- Not a login flow. You must provide your own session cookies.

## Architecture

`twit-rank` is one Rust process that:
- reads settings file from `state/settings.toml`
- fetches timelines directly from X using cookie-backed GraphQL sessions
- stores tweets and feedback in `state/archive.sqlite`
- serves an Axum JSON API and embedded React frontend

## Quick start

Prerequisites:
- Rust stable toolchain
- Node.js and npm
- `trunk` for the embedded Tetris wasm build
- `wasm32-unknown-unknown` Rust target for the embedded Tetris wasm build

Install the extra frontend/wasm tools once:

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

### Option 1: use the setup page

1. Start the app:

   ```bash
   cargo run
   ```

2. Open <http://127.0.0.1:3030>
3. If no settings are configured, open the **Setup** page in the UI.
4. Optionally add any X lists you want to archive as `List ID` plus optional `Slug`.
5. Paste at least one X session (`auth_token` + `ct0`) and save.
6. Restart `twit-rank` so the archiver picks up the new sessions.

### Getting X credentials

You need two cookie values from your own logged-in browser session:

- `auth_token`
- `ct0`

One straightforward way to get them:

1. Log into [x.com](https://x.com) in a desktop browser.
2. Open browser developer tools.
3. Open the cookies/storage view for `https://x.com`.
4. Find the `auth_token` and `ct0` cookies.
5. Copy their values into the `Setup` page or `state/settings.toml`.

Do not commit or share `state/settings.toml`.

Sessions can expire or get invalidated. If archiving stops working, refresh those cookie values.

### Getting list IDs

Lists are optional. Even with no lists configured, the archiver still collects your `Following` and `For you` timelines.

- If you have a list URL like `https://x.com/i/lists/123456789012345678`, the numeric tail is the list ID.
- The optional slug is just a readable local label used in the UI and feed names.
- In settings, lists are stored as either:
  - `123456789012345678`
  - `123456789012345678:my-list`

You can add multiple lists in the `Setup` page.

### Option 2: create the settings file manually

Copy [settings.example.toml](settings.example.toml) to `state/settings.toml` and fill in your session cookie values.

Example:

```toml
archive_path = "state/archive.sqlite"
list_ids = ["123456789012345678:my-list"]
poll_mins = 15
max_pages = 20
page_delay_ms = 2000
feed_delay_ms = 30000

[[sessions]]
username = "your_handle"
auth_token = "..."
ct0 = "..."
```

Then run:

```bash
cargo run
```

`cargo run` defaults to `run`, which starts the web UI on `127.0.0.1:3030` and starts the archiver if sessions are configured.

## Minimal first use

1. Run `cargo run`.
2. Open <http://127.0.0.1:3030>.
3. Open `Setup`.
4. Optionally add one or more lists.
5. Add one session with `auth_token` and `ct0`.
6. Save and restart the app.
7. Wait for the archiver to collect at least one `Following`, `For you`, or list page.
8. Open `Home` to browse archived tweets or `Search` to query the archive.

## UI

Published UI surfaces:
- `Home`: browse archived feeds
- `Search`: query the archive by text, regex, author, feed, dates, counts, media, and tweet kind
- `Setup`: edit the merged settings file

Placeholder navigation entries from earlier iterations were removed.

## API

| Endpoint | Description |
|----------|-------------|
| `GET /api/posts?feed=forYou&limit=50&offset=0` | Fetch archived posts with pagination |
| `GET /api/search?...` | Search archived tweets with structured filters and optional regex |
| `GET /api/post/<id>` | Fetch a single archived or on-demand tweet |
| `GET /api/lists` | List available archived list feeds |
| `GET /api/feeds` | List feed types (`following`, `forYou`, `list:*`) |
| `GET /api/build` | Build metadata for the running backend |
| `GET /api/settings/status` | Setup status for the UI |
| `GET /api/settings` | Current merged settings |
| `POST /api/settings` | Save merged settings |
| `GET /api/ai/context?feed=forYou&limit=50` | Plain text export for LLM context |
| `POST /api/feedback` | Submit like/dislike `{ id, value }` |

## Settings

Default settings path:

```text
state/settings.toml
```

Important fields:
- `archive_path`: archive SQLite path
- `list_ids`: X list IDs, stored as either `id` or `id:slug`
- `poll_mins`, `max_pages`, `page_delay_ms`, `feed_delay_ms`: archiver behavior
- `tid_disable`, `tid_pairs_url`: X client transaction-id behavior
- `[[sessions]]`: X cookie sessions

CLI overrides:
- `--config <PATH>`
- `--archive <PATH>`
- `--list-ids <IDS>`
- `--poll-mins <MINUTES>`
- `--max-pages <COUNT>`
- `--tid-disable`
- `--tid-pairs-url <URL>`
- `--bind <ADDR>`

## Archive notes

- The archive schema is created and migrated automatically on startup.
- Startup logs per-step timing and binds the web UI before any slow legacy archive cleanup runs.
- `twit-rank` takes an exclusive sibling lock file (`state/archive.sqlite.lock` by default) so a second writer process exits immediately.
- The archiver always collects the built-in `following` and `forYou` feeds. Configured lists are additional optional sources.
- Search uses normalized `tweets.search_text` instead of rendered HTML.
- Search only covers tweets that have already been archived locally.
- Tweet debug fields retained in the archive:
  - `tweets.entities_json`
  - `tweets.x_raw_json`
  - `tweets.search_text`
  - `tweets.username_lc`

## Troubleshooting

- The UI opens but no tweets appear:
  - Check that at least one session has valid `auth_token` and `ct0`.
  - Restart after saving setup changes.
  - Wait for the first `following` / `forYou` archive tick to finish.
- Search returns nothing:
  - Search only uses your local archive.
  - If the archive is still empty, let the archiver collect some tweets first.
- Archiving stops unexpectedly:
  - Your X session cookies may have expired or been invalidated. Refresh `auth_token` and `ct0`.

## Development

```bash
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

cd frontend
npm ci
npm run lint
npm run build
```

If you change frontend/wasm dependencies or delete generated Tetris assets, make sure the wasm toolchain is installed locally too:

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

To skip the automatic frontend build inside Cargo:

```bash
TWIT_RANK_SKIP_FRONTEND_BUILD=1 cargo check
```

For local backend iteration, especially under WSL, the same env var also avoids the frontend rebuild on `cargo run`:

```bash
TWIT_RANK_SKIP_FRONTEND_BUILD=1 cargo run
```

## Platform notes

- The project is developed as a local desktop/server app.
- Native CI is defined for Ubuntu LTS, macOS, and Windows in [`.github/workflows/ci.yml`](.github/workflows/ci.yml).
- Local cross-checking from Windows to Linux/macOS is limited by missing target C toolchains for bundled SQLite and native TLS.
- The frontend build requires Node.js and npm on every platform because the Rust build embeds `frontend/dist`.
- The embedded Tetris mini-game also requires `trunk` and the `wasm32-unknown-unknown` Rust target when the wasm assets need to be rebuilt.

## License

MIT. See [LICENSE](LICENSE).
