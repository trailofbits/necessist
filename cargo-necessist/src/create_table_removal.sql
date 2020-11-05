CREATE TABLE removal (
    pkg     TEXT NOT NULL,
    test    TEXT NOT NULL,
    span    TEXT NOT NULL,
    stmt    TEXT NOT NULL,
    result  TEXT NOT NULL CHECK (result IN ('inconclusive', 'skipped', 'nonbuildable', 'failed', 'timed-out', 'passed')),
    url     TEXT NOT NULL,
    PRIMARY KEY (span)
)
