-- Seed data for local demo. Safe to re-run: uses fixed UUIDs and ON CONFLICT.
--
-- Device: dev-001 (00000000-0000-0000-0000-000000000001)
-- Raw device key (DO NOT store in prod logs): 
--   fp_00000000000000000000000000000101_3372f3f3d3804f3191361e0b5ad643fb
-- The key embeds api_keys.id as `fp_<uuid-simple>_<secret>` so auth can do a
-- direct indexed lookup instead of scanning all active keys.

insert into devices (id, name, status, firmware_version, last_seen, created_at)
values (
  '00000000-0000-0000-0000-000000000001',
  'demo-device-001',
  'provisioning',
  '0.1.0',
  null,
  now()
)
on conflict (id) do update
  set name = excluded.name,
      firmware_version = excluded.firmware_version;

insert into api_keys (id, device_id, key_hash, created_at, revoked_at)
values (
  '00000000-0000-0000-0000-000000000101',
  '00000000-0000-0000-0000-000000000001',
  '$argon2id$v=19$m=19456,t=2,p=1$fIodqlz3eoOIYC12Anm2Yw$xsros5IrZbKuEF9GwLk6CItKF/mKJ0IvB/JX7imm0A8',
  now(),
  null
)
on conflict (id) do update
  set key_hash = excluded.key_hash,
      revoked_at = null;
