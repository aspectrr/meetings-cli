---
name: meetings-cli
description: Query hyprnote meeting sessions — search transcripts and memos with semantic search, list sessions, show notes, and extract speaker utterances. Use when the user asks about meeting content, action items, decisions, or what was discussed in a specific meeting.
---

# Meetings CLI

A CLI tool for querying hyprnote meeting sessions stored locally. Provides semantic search across meeting transcripts and memos.

## Prerequisites

- `meetings` CLI must be installed (`cargo install --path /Users/collinpfeifer/GitHub/meetings-cli`)
- Index must be built before searching: `meetings index`

## When to Use

- User asks "what was discussed in..." or "find meetings about..."
- User wants action items, decisions, or notes from meetings
- User asks about specific people or topics in meetings
- User wants to search across meeting transcripts semantically
- User references "that meeting with X" or "the Senior Helpers call"

## Commands

### List all sessions
```bash
meetings list --json
```
Returns JSON array of sessions with `id`, `title`, `created_at`, `participants`.

### Show a session's memo/notes
```bash
meetings show "Senior Helpers"
meetings show <session-id>
```
Matches by session ID or case-insensitive title substring. Outputs the `_memo.md` content.

### List speaker utterances with timestamps
```bash
meetings speakers "Senior Helpers" --json
```
Returns JSON array of `{channel, start_ms, end_ms, text, speaker_label}`.

### Index sessions (must run before search, re-run when new sessions added)
```bash
meetings index
```
Downloads ONNX embedding model on first run (~130MB). Indexes all sessions into `~/.meetings-cli/index.bin`.

### Semantic search across all sessions
```bash
meetings search "action items for hiring" --json --top-k 5
```
Returns JSON array of `{rank, score, session_id, title, chunk_type, text, start_ms, end_ms}`.

- `chunk_type`: `"memo"` (meeting notes) or `"transcript"` (audio transcript segment)
- `score`: cosine similarity (0-1, higher = more relevant)
- `start_ms`/`end_ms`: time offset in transcript (null for memo chunks)

## Agent Workflow

1. **Find relevant meetings**: `meetings list --json` to get session IDs and titles
2. **Semantic search**: `meetings search "<query>" --json --top-k 10` to find relevant chunks
3. **Read full notes**: `meetings show "<title>"` for the complete memo
4. **See who said what**: `meetings speakers "<title>" --json` for timestamped utterances

## Tips

- Always use `--json` for parseable output
- Title matching is case-insensitive substring — `meetings show "senior helpers"` works
- Run `meetings index` once before first search, and again after new sessions are added
- The search uses local ONNX embeddings (bge-small-en-v1.5) — no API calls needed
- Combine search results with `meetings show` to get full context around hits
- For "find action items from X meeting" → search with relevant terms, then show the memo for structured notes
