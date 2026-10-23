CREATE TABLE translation_groups (
    id uuid PRIMARY KEY
);
INSERT INTO translation_groups(id) SELECT id FROM contents;
ALTER TABLE contents ADD COLUMN locale text NOT NULL DEFAULT 'it' CHECK (locale IN ('it','en'));
ALTER TABLE contents ADD COLUMN translation_group uuid REFERENCES translation_groups(id);
UPDATE contents SET translation_group=id;
ALTER TABLE contents ALTER COLUMN translation_group SET NOT NULL;
ALTER TABLE contents DROP CONSTRAINT contents_slug_key;
ALTER TABLE contents ADD CONSTRAINT contents_locale_slug_key UNIQUE(locale,slug);
ALTER TABLE contents ADD CONSTRAINT contents_group_locale_key UNIQUE(translation_group,locale);
ALTER TABLE contents ADD CONSTRAINT contents_locale_route CHECK (locale <> 'it' OR slug <> 'en');
ALTER TABLE contents ADD COLUMN markdown text CHECK (octet_length(markdown) <= 500000);
ALTER TABLE contents ADD CONSTRAINT contents_body_format CHECK (markdown IS NULL OR blocks='[]'::jsonb);
DROP INDEX content_search;
CREATE INDEX content_search ON contents USING gin (
    to_tsvector('simple',title || ' ' || COALESCE(markdown,blocks::text))
);
CREATE INDEX localized_published_content ON contents(locale,publish_at DESC,id) WHERE status='published';
ALTER TABLE navigation ADD COLUMN locale text NOT NULL DEFAULT 'it' CHECK (locale IN ('it','en'));
ALTER TABLE navigation DROP CONSTRAINT navigation_pkey;
ALTER TABLE navigation ADD PRIMARY KEY(name,locale);
INSERT INTO navigation(name,locale,items) VALUES ('main','en','[]');
