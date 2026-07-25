# Agent App Storage

## Storage Root

The default storage root is:

```text
./.agent-app
```

The path is relative to the current working directory so development runs remain inspectable. A later release may move the default to an OS-specific app data directory, but that must be specified before changing behavior.

## Layout

```text
.agent-app/
  config.json
  sessions/
    <conversation-id>.json
```

## Conversation Format

Each conversation file stores:

- `id`: stable conversation id.
- `messages`: ordered messages.
- `created_at`: Unix timestamp in milliseconds.
- `updated_at`: Unix timestamp in milliseconds.

Each message stores:

- `role`: `user`, `assistant`, or `system`.
- `content`: message body.
- `timestamp`: Unix timestamp in milliseconds.

## Consistency Rules

- Core is the only layer allowed to read or write conversation files.
- Entry layers pass conversation ids and user input into core; they do not mutate storage.
- Missing conversations are created on first write.
- Clearing a conversation removes its persisted file when persistence is enabled.
- Corrupt session files are ignored by listing APIs and surfaced as storage errors only when directly requested.

## Configuration

The first milestone uses defaults and does not require `config.json`. Configuration persistence is reserved for a later spec update.
