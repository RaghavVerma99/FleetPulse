-- Initial schema for the FleetPulse MVP.
--
-- Four tables only: devices, their credentials, telemetry batches, and the
-- readings inside those batches. Redis, streams, and alerts are deferred
-- (see docs/MVP.md).

create table devices (
    id uuid primary key,
    name text not null,
    status text not null default 'provisioning'
        check (status in ('provisioning', 'online', 'offline', 'retired')),
    firmware_version text,
    last_seen timestamptz,
    created_at timestamptz not null default now()
);

create table api_keys (
    id uuid primary key,
    device_id uuid not null references devices (id) on delete cascade,
    key_hash text not null,
    created_at timestamptz not null default now(),
    revoked_at timestamptz
);

-- A key hash identifies at most one live credential.
create unique index api_keys_active_hash_idx
    on api_keys (key_hash)
    where revoked_at is null;

-- One row per accepted upload. The client-supplied id is the idempotency key.
create table telemetry_batches (
    id uuid primary key,
    device_id uuid not null references devices (id) on delete cascade,
    received_at timestamptz not null default now(),
    reading_count integer not null check (reading_count > 0)
);

create table readings (
    id uuid primary key default gen_random_uuid(),
    device_id uuid not null references devices (id) on delete cascade,
    batch_id uuid not null references telemetry_batches (id) on delete cascade,
    ts timestamptz not null,
    key text not null,
    value double precision not null,
    metadata jsonb not null default '{}'::jsonb
);

-- The query pattern that matters: newest readings for one device.
create index readings_device_ts_idx on readings (device_id, ts desc);
