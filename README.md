# meetings-cli

Rust CLI + MCP server for querying [Anarlog](https://docs.anarlog.so) / [hyprnote](https://hyprnote.com) meeting sessions with semantic search.

Reads sessions from the Anarlog SQLite database at `~/Library/Application Support/hyprnote/app.db` (opened read-only). Notes, summaries, and transcripts are stored as ProseMirror JSON or markdown and converted to text on load.

## Install

**From GitHub (recommended):**
```bash
cargo install --git https://github.com/aspectrr/meetings-cli
```

**From source:**
```bash
git clone https://github.com/aspectrr/meetings-cli && cd meetings-cli
cargo install --path .
```

Both put `meetings` on your PATH via `~/.cargo/bin`. Requires the [Rust toolchain](https://rustup.rs).

## Usage

All commands support `--db-path /custom/path/to/app.db` to point at a non-default database.

### Check database health
```bash
meetings doctor
```
Verifies DB path, read access, and session count. Exits 1 if not ready.

### List sessions
```bash
meetings list
meetings list --query "Senior Helpers"
meetings list --json
```

### Show session details (note + summaries + action items)
```bash
meetings show "Senior Helpers"
meetings show <session-id>
```

### List meetings in a recurring series
```bash
meetings history "Weekly Sync"
meetings history <session-id> --json
```

### List speaker utterances
```bash
meetings speakers "Senior Helpers" --json
meetings speakers "Senior Helpers" --pause-ms 2000
```

### Index sessions (embed chunks for search)
```bash
meetings index
meetings index --segment-ms 30000
```

First run downloads the `BAAI/bge-small-en-v1.5` ONNX embedding model (~130MB). Index stored at `~/.meetings-cli/index.bin`.

### Semantic search
```bash
meetings search "action items and hiring process"
meetings search "marketing plan Josie" --json --top-k 10
```

## MCP Server

Run `meetings mcp` to start a read-only MCP server over stdio. Exposes 4 tools:

| Tool | Description |
| --- | --- |
| `list_meetings` | List/filter meetings with metadata and document counts |
| `get_meeting` | Full details: note, AI summaries, action items, participants |
| `search_sessions` | Semantic search across notes, summaries, and transcripts. Auto-indexes when stale. |
| `get_transcript` | Speaker-segmented transcript with timestamps |

### Configure an MCP client

Add to your client config (Claude Desktop, Cursor, etc.):

```json
{
  "mcpServers": {
    "meetings": {
      "command": "meetings",
      "args": ["mcp"]
    }
  }
}
```

With a custom database path:

```json
{
  "mcpServers": {
    "meetings": {
      "command": "meetings",
      "args": ["--db-path", "/path/to/app.db", "mcp"]
    }
  }
}
```

### How it differs from the official `anarlog mcp`

The official Anarlog CLI ships its own MCP server (`anarlog mcp`). This one adds:

- **Semantic search** — `search_sessions` finds relevant passages by meaning, not just substring
- **Speaker diarization** — `get_transcript` returns grouped utterances with speaker labels, not a flat word stream
- **Auto-indexing** — `search_sessions` rebuilds the index automatically when sessions change

## Agent integration

All commands support `--json` for machine-readable output. Recommended workflow:

1. `meetings list --json` → find relevant session IDs
2. `meetings show <id>` → read note, AI summaries, and action items
3. `meetings index && meetings search "<query>" --json` → always index first, then semantic search
4. `meetings speakers <id> --json` → get speaker-separated transcript

## How it works

- Reads session metadata, transcripts, notes, summaries, action items, and participants directly from Anarlog's `app.db` SQLite database (read-only, `immutable=1`).
- Notes and summaries are stored in `session_documents` as ProseMirror JSON or markdown, converted to text on load.
- **fastembed** (ONNX Runtime) runs `BAAI/bge-small-en-v1.5` locally for embeddings — no API calls, no server.
- Text is chunked into 60s transcript segments + whole notes + whole summaries for embedding.
- Cosine similarity search against local index.
- Speaker detection uses channel changes + pause thresholds from transcript word-level data.
