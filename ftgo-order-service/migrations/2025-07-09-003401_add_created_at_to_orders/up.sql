ALTER TABLE orders ADD COLUMN created_at timestamptz NOT NULL DEFAULT NOW();
