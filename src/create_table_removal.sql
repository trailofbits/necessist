CREATE TABLE removal (
    span    TEXT NOT NULL,
    text    TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('nonbuildable', 'failed', 'timed-out', 'passed')),
    url     TEXT NOT NULL,
    PRIMARY KEY (span)
)
