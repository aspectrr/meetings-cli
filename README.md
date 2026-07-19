# meetings-cli

Rust CLI for querying [hyprnote](https://hyprnote.com) / [Anarlog](https://docs.anarlog.so) meeting sessions with semantic search.

Reads sessions from the Anarlog SQLite database at `~/Library/Application Support/hyprnote/app.db` (opened read-only). Memos are stored there as ProseMirror JSON and converted to text on load.

## Install

```bash
cargo build --release
# binary at target/release/meetings
```

## Usage

### List sessions
```bash
meetings list
meetings list --json
```

Use `--db-path /custom/path/to/app.db` to point at a non-default database.

### Show a session memo
```bash
meetings show "Senior Helpers"
meetings show <session-id>
```

### List speaker utterances
```bash
meetings speakers "Senior Helpers"
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

## Agent integration

All commands support `--json` for machine-readable output. Pattern for agents:

1. `meetings list --json` → find relevant session IDs
2. `meetings show <id>` → read the memo/notes
3. `meetings index` → build the vector index (run once, or after new sessions)
4. `meetings search "<query>" --json` → semantic search across all sessions
5. `meetings speakers <id> --json` → get speaker-separated transcript

## How it works

- Reads session metadata, transcripts, memos, and participants directly from Anarlog's `app.db` SQLite database (read-only, `immutable=1`). Older Anarlog versions wrote flat files (`_meta.json`/`_memo.md`/`transcript.json`); newer versions store everything in the DB, so the CLI reads the DB as the single source of truth.
- Memos are ProseMirror JSON documents, converted to plain text for embedding and display.
- **fastembed** (ONNX Runtime) runs `BAAI/bge-small-en-v1.5` locally for embeddings — no API calls, no server
- Transcripts are chunked into 60s segments + memos stored as whole chunks
- Cosine similarity search against local index
- Speaker detection uses channel changes + pause thresholds from transcript word-level data
