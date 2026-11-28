-- M5: threshold rules and the alerts they fire during ingest (ADR-015).
-- Rules live in Postgres; there is no worker or stream in the MVP.

create table alert_rules (
    id uuid primary key default gen_random_uuid(),
    device_id uuid not null references devices (id) on delete cascade,
    metric_key text not null check (length(metric_key) between 1 and 128),
    operator text not null check (operator in ('gt', 'gte', 'lt', 'lte')),
    threshold double precision not null,
    severity text not null check (severity in ('info', 'warning', 'critical')),
    cooldown_seconds integer not null default 300 check (cooldown_seconds >= 0),
    enabled boolean not null default true,
    created_at timestamptz not null default now()
);

create index alert_rules_device_enabled_idx
    on alert_rules (device_id) where enabled;

create table alerts (
    id uuid primary key default gen_random_uuid(),
    device_id uuid not null references devices (id) on delete cascade,
    rule_id uuid not null references alert_rules (id) on delete cascade,
    severity text not null check (severity in ('info', 'warning', 'critical')),
    value double precision not null,
    message text not null,
    triggered_at timestamptz not null default now(),
    resolved_at timestamptz
);

create index alerts_device_triggered_idx
    on alerts (device_id, triggered_at desc, id desc);
create index alerts_rule_triggered_idx
    on alerts (rule_id, triggered_at desc);
