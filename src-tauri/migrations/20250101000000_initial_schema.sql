-- Initial local SQLite schema for LoLShorts.
-- This database stores local app metadata only. Supabase remains the
-- authoritative source for authentication, billing, and PRO entitlement.

CREATE TABLE IF NOT EXISTS local_migrations (
    name TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);

-- Games table: stores game session information
CREATE TABLE IF NOT EXISTS games (
    game_id TEXT PRIMARY KEY,
    metadata_json TEXT NOT NULL,
    champion TEXT NOT NULL,
    game_mode TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_games_game_id ON games(game_id);
CREATE INDEX idx_games_start_time ON games(start_time DESC);

-- Events table: stores serialized local event metadata per game
CREATE TABLE IF NOT EXISTS events (
    game_id TEXT PRIMARY KEY,
    events_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Clips table: stores video clip information
CREATE TABLE IF NOT EXISTS clips (
    game_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    event_time REAL NOT NULL,
    priority INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (game_id, file_path)
);

CREATE INDEX idx_clips_game_id ON clips(game_id);
CREATE INDEX idx_clips_priority ON clips(priority DESC);
CREATE INDEX idx_clips_event_time ON clips(event_time);

-- Generic local settings table. Do not store authoritative app auth,
-- payment, or entitlement state here.
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS auto_edit_usage (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    usage_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS auto_edit_results (
    result_id TEXT PRIMARY KEY,
    metadata_json TEXT NOT NULL,
    output_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_auto_edit_results_created_at ON auto_edit_results(created_at DESC);
