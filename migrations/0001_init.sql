create table if not exists nodes (
  id text primary key,
  label text not null,
  created_at text not null default current_timestamp,
  updated_at text not null default current_timestamp,
  last_seen_at text not null default current_timestamp
);

create table if not exists node_endpoints (
  id integer primary key autoincrement,
  node_id text not null references nodes(id) on delete cascade,
  kind text not null,
  schema_version integer not null,
  addr text not null,
  priority integer not null default 0,
  created_at text not null default current_timestamp
);

create table if not exists node_services (
  id text primary key,
  node_id text not null references nodes(id) on delete cascade,
  kind text not null,
  schema_version integer not null,
  target text not null,
  user_name text,
  label text,
  created_at text not null default current_timestamp
);

create table if not exists owner_bootstrap_codes (
  code text primary key,
  consumed_at text,
  created_at text not null default current_timestamp
);

create table if not exists device_certificates (
  device_id text not null,
  cert_pem text not null,
  expires_at text not null,
  primary key (device_id, expires_at)
);
