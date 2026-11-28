-- Supporting indexes for the M3 read paths and M4 pagination.
-- Kept in a separate migration so 0001 stays immutable for deployed databases.

-- latest-per-metric uses DISTINCT ON (key) ordered by newest reading.
create index if not exists readings_device_key_ts_idx
    on readings (device_id, key, ts desc, id desc);

-- Device deletion cascades to these tables; index the referencing columns.
create index if not exists api_keys_device_id_idx
    on api_keys (device_id);
create index if not exists telemetry_batches_device_id_idx
    on telemetry_batches (device_id);

-- Cursor pagination on GET /v1/devices (newest first).
create index if not exists devices_created_id_idx
    on devices (created_at desc, id desc);
