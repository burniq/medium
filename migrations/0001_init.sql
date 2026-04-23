create table if not exists devices (
  id text primary key,
  label text not null,
  created_at timestamptz not null default now()
);

create table if not exists owner_bootstrap_codes (
  code text primary key,
  consumed_at timestamptz,
  created_at timestamptz not null default now()
);

create table if not exists device_certificates (
  device_id text not null,
  cert_pem text not null,
  expires_at timestamptz not null,
  primary key (device_id, expires_at)
);
