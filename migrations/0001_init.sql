create table if not exists devices (
  id text primary key,
  label text not null,
  created_at timestamptz not null default now()
);
