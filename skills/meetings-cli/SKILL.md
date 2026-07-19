---
name: meetings-cli
description: Query Anarlog/hyprnote meeting sessions — search transcripts and summaries with semantic search, list sessions, show notes/summaries/action items, and extract speaker utterances. Use when the user asks about meeting content, action items, decisions, or what was discussed in a specific meeting.
---

# Meetings CLI

A CLI tool for querying Anarlog (formerly hyprnote) meeting sessions stored locally. Provides semantic search across meeting notes, summaries, and transcripts, plus speaker diarization that the official anarlog CLI lacks.

## Prerequisites

- `meetings` binary on PATH (or at `target/release/meetings` in the repo)
- Reads from Anarlog's `app.db` SQLite database at `~/Library/Application Support/hyprnote/app.db` (override with `--db-path`)
- Index must be built before searching: `meetings index`

## When to Use

- User asks "what was discussed in..." or "find meetings about..."
- User wants action items, decisions, or notes from meetings
- User asks about specific people or topics in meetings
- User wants to search across meeting transcripts semantically
- User references "that meeting with X" or "the Senior Helpers call"

## Commands

### Check database health
```bash
meetings doctor
```
Verifies DB path, read access, and session count. Exits 1 if not ready.

### List sessions (optionally filtered)
```bash
meetings list
meetings list --query "Senior Helpers"
meetings list --json
```
JSON returns `id`, `title`, `kind`, `status`, `created_at`, `updated_at`, `started_at`, `ended_at`, `series_id`, `participants`, `has_note`, `summary_count`, `action_item_count`.

### Show session details (note + summaries + action items)
```bash
meetings show "Senior Helpers"
meetings show <session-id>
```
Matches by session ID or case-insensitive title substring. Displays the human note, AI-generated summaries, and action items with status.

### List meetings in a recurring series
```bash
meetings history "Weekly Sync"
meetings history <session-id> --json
```
Finds the session's `series_id`, then lists all sessions in that series.

### List speaker utterances with timestamps
```bash
meetings speakers "Senior Helpers" --json
```
Returns JSON array of `{channel, start_ms, end_ms, text, speaker_label}`.

### Index sessions (must run before search, re-run when new sessions added)
```bash
meetings index
```
Downloads ONNX embedding model on first run (~130MB). Indexes all sessions into `~/.meetings-cli/index.bin`. Chunks include notes, summaries, and transcript segments.

### Semantic search across all sessions

**IMPORTANT: Always run `meetings index` before `meetings search`.** The index is a static snapshot — it does not auto-refresh when new meetings are added. Re-indexing takes seconds and ensures search covers the latest sessions.

```bash
meetings index && meetings search "action items for hiring" --json --top-k 5
```
Returns JSON array of `{rank, score, session_id, title, chunk_type, text, start_ms, end_ms}`.

- `chunk_type`: `"note"` (human notes), `"summary"` (AI summary), or `"transcript"` (audio segment)
- `score`: cosine similarity (0-1, higher = more relevant)
- `start_ms`/`end_ms`: time offset in transcript (null for note/summary chunks)

## Agent Workflow

1. **Find relevant meetings**: `meetings list --json` (or `--query "topic"`) to get session IDs and titles
2. **Semantic search**: `meetings index && meetings search "<query>" --json --top-k 10` — always index first to ensure fresh results, then search across notes, summaries, and transcripts
3. **Read full details**: `meetings show "<title>"` for the note, AI summary, and action items
4. **See who said what**: `meetings speakers "<title>" --json` for timestamped utterances

## Tips

- Always use `--json` for parseable output
- Title matching is case-insensitive substring — `meetings show "senior helpers"` works
- Run `meetings index` once before first search, and again after new sessions are added
- The search uses local ONNX embeddings (bge-small-en-v1.5) — no API calls needed
- `show` now surfaces AI summaries separately from human notes — use it to get both
- For "find action items from X meeting" → `show` displays structured action items; `search` finds relevant context across all sessions
- This CLI reads the same `app.db` as the official `anarlog` CLI but adds semantic search + speaker diarization that anarlog lacks
