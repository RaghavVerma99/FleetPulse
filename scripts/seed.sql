-- Seed data for local demo. Safe to re-run: uses fixed UUIDs and ON CONFLICT.
-- Device: dev-001 (00000000-0000-0000-0000-000000000001)
-- Raw device key (store only hash): fp_dev_demo_key_001
-- Replace the hash if you change the key; generate with argon2 in app/tests.

insert into devices (id, name, status, firmware_version, last_seen, created_at)
values (
  '00000000-0000-0000-0000-000000000001',
  'demo-device-001',
  'provisioning',
  '0.1.0',
  null,
  now()
)
on conflict (id) do update set name = excluded.name;

-- Argon2id hash for "fp_dev_demo_key_001" generated offline for demo only.
-- If auth fails, create a key via SQL with a freshly generated hash.
insert into api_keys (id, device_id, key_hash, created_at, revoked_at)
values (
  '00000000-0000-0000-0000-000000000101',
  '00000000-0000-0000-0000-000000000001',
  '$argon2id$v=19$m=19456,t=2,p=1$Q0hBTkdFTUVTQUxU$4L5vXlL5T5Wl5q3QvYkQ8v6l9r6s8gq2Qf0f5o3jP5I',
  now(),
  null
)
on conflict (id) do nothing;
