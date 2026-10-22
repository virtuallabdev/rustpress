CREATE TABLE identities (
    id uuid PRIMARY KEY,
    provider text NOT NULL CHECK (provider IN ('google', 'microsoft', 'meta')),
    subject text NOT NULL,
    role text NOT NULL CHECK (role IN ('admin', 'editor', 'author')),
    UNIQUE(provider, subject)
);
CREATE TABLE credentials (
    token_hash text PRIMARY KEY,
    identity_id uuid REFERENCES identities(id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('admin', 'editor', 'author', 'agent')),
    label text NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE oauth_states (
    state_hash text PRIMARY KEY,
    browser_hash text NOT NULL,
    provider text NOT NULL,
    verifier text NOT NULL,
    expires_at timestamptz NOT NULL
);
CREATE TABLE content_types (
    slug text PRIMARY KEY,
    name text NOT NULL,
    fields jsonb NOT NULL DEFAULT '[]'
);
INSERT INTO content_types (slug, name) VALUES ('page', 'Page'), ('post', 'Post');
CREATE TABLE contents (
    id uuid PRIMARY KEY,
    content_type text NOT NULL REFERENCES content_types(slug),
    slug text NOT NULL UNIQUE,
    title text NOT NULL,
    blocks jsonb NOT NULL,
    fields jsonb NOT NULL DEFAULT '{}',
    extra_fields jsonb NOT NULL DEFAULT '[]',
    seo jsonb NOT NULL DEFAULT '{}',
    status text NOT NULL CHECK (status IN ('draft', 'published', 'archived')),
    publish_at timestamptz NOT NULL DEFAULT now(),
    author_id uuid REFERENCES identities(id),
    version integer NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX published_content ON contents(publish_at DESC, id) WHERE status = 'published';
CREATE INDEX content_search ON contents USING gin (
    to_tsvector('simple', title || ' ' || blocks::text)
);
CREATE TABLE revisions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    content_id uuid NOT NULL REFERENCES contents(id),
    version integer NOT NULL,
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(content_id, version)
);
CREATE TABLE navigation (
    name text PRIMARY KEY,
    items jsonb NOT NULL
);
INSERT INTO navigation (name, items) VALUES ('main', '[]');
CREATE TABLE media (
    id uuid PRIMARY KEY,
    alt text NOT NULL,
    width integer NOT NULL,
    height integer NOT NULL,
    variants jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor text NOT NULL,
    action text NOT NULL,
    resource text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
