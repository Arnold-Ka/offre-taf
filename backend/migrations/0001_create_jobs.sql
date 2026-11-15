CREATE TABLE IF NOT EXISTS jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    company TEXT NOT NULL,
    location TEXT,
    work_mode TEXT,
    contract_type TEXT NOT NULL DEFAULT '',
    relocation_support BOOLEAN NOT NULL DEFAULT FALSE,
    description TEXT NOT NULL DEFAULT '',
    source_url TEXT NOT NULL UNIQUE,
    source_name TEXT NOT NULL DEFAULT '',
    eligibility_confidence TEXT NOT NULL DEFAULT '',
    posted_at TIMESTAMPTZ,
    collected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_read BOOLEAN NOT NULL DEFAULT FALSE
);

ALTER TABLE jobs ADD COLUMN IF NOT EXISTS relocation_support BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS contract_type TEXT NOT NULL DEFAULT '';
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS description TEXT NOT NULL DEFAULT '';
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS source_name TEXT NOT NULL DEFAULT '';
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS eligibility_confidence TEXT NOT NULL DEFAULT '';
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS is_read BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE jobs ADD COLUMN IF NOT EXISTS mentions_java BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS remote_scope TEXT NOT NULL DEFAULT '';
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Favourites: added once, with a one-time backfill of listings that mention Java
-- (\m...\M are word boundaries, so "JavaScript" does not match). Later runs never
-- re-favourite a listing the user removed from favourites.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'jobs' AND column_name = 'is_favorite'
    ) THEN
        ALTER TABLE jobs ADD COLUMN is_favorite BOOLEAN NOT NULL DEFAULT FALSE;
        UPDATE jobs SET is_favorite = TRUE, mentions_java = TRUE
        WHERE title ~* '\mjava\M' OR description ~* '\mjava\M';
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS jobs_posted_at_idx ON jobs (posted_at DESC);
CREATE INDEX IF NOT EXISTS jobs_is_read_idx ON jobs (is_read);
CREATE INDEX IF NOT EXISTS jobs_is_favorite_idx ON jobs (is_favorite) WHERE is_favorite;
CREATE INDEX IF NOT EXISTS jobs_source_name_idx ON jobs (source_name);
