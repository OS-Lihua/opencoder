-- Initial schema, mirrors the Drizzle schema from the original OpenCode.
-- Tables: project, workspace, session, message, part, todo, permission

CREATE TABLE IF NOT EXISTS project (
    id TEXT PRIMARY KEY NOT NULL,
    worktree TEXT NOT NULL,
    vcs TEXT,
    name TEXT NOT NULL,
    icon_url TEXT,
    icon_color TEXT,
    sandbox TEXT,
    commands TEXT,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE TABLE IF NOT EXISTS workspace (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE TABLE IF NOT EXISTS session (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    workspace_id TEXT,
    parent_id TEXT,
    slug TEXT NOT NULL,
    directory TEXT NOT NULL,
    title TEXT NOT NULL,
    version TEXT NOT NULL,
    share_url TEXT,
    summary_additions INTEGER,
    summary_deletions INTEGER,
    summary_files INTEGER,
    summary_diffs TEXT,
    revert TEXT,
    permission TEXT,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_compacting INTEGER,
    time_archived INTEGER
);

CREATE INDEX IF NOT EXISTS session_project_idx ON session(project_id);
CREATE INDEX IF NOT EXISTS session_workspace_idx ON session(workspace_id);
CREATE INDEX IF NOT EXISTS session_parent_idx ON session(parent_id);

CREATE TABLE IF NOT EXISTS message (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    data TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS message_session_idx ON message(session_id);

CREATE TABLE IF NOT EXISTS part (
    id TEXT PRIMARY KEY NOT NULL,
    message_id TEXT NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    data TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS part_message_idx ON part(message_id);
CREATE INDEX IF NOT EXISTS part_session_idx ON part(session_id);

CREATE TABLE IF NOT EXISTS todo (
    session_id TEXT NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'medium',
    position INTEGER NOT NULL,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    PRIMARY KEY (session_id, position)
);

CREATE INDEX IF NOT EXISTS todo_session_idx ON todo(session_id);

CREATE TABLE IF NOT EXISTS permission (
    project_id TEXT PRIMARY KEY NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_share (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    secret TEXT NOT NULL,
    url TEXT NOT NULL,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE TABLE IF NOT EXISTS control_account (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    data TEXT NOT NULL,
    time_created INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    time_updated INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
