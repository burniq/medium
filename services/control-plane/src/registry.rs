use overlay_protocol::{DeviceCatalogResponse, DeviceRecord, RegisterNodeRequest, SshEndpoint};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct RegistryStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct ServiceRoute {
    pub node_id: String,
    pub tcp_addr: String,
    pub user_name: Option<String>,
}

impl RegistryStore {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::migrate!("../../migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn in_memory() -> anyhow::Result<Self> {
        Self::connect("sqlite::memory:").await
    }

    pub async fn register_node(&self, request: &RegisterNodeRequest) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            insert into nodes (id, label)
            values (?1, ?2)
            on conflict(id) do update set
              label = excluded.label,
              updated_at = current_timestamp,
              last_seen_at = current_timestamp
            "#,
        )
        .bind(&request.node_id)
        .bind(&request.node_label)
        .execute(&mut *tx)
        .await?;

        sqlx::query("delete from node_endpoints where node_id = ?1")
            .bind(&request.node_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("delete from node_services where node_id = ?1")
            .bind(&request.node_id)
            .execute(&mut *tx)
            .await?;

        for endpoint in &request.endpoints {
            sqlx::query(
                r#"
                insert into node_endpoints (node_id, kind, schema_version, addr, priority)
                values (?1, ?2, ?3, ?4, ?5)
                "#,
            )
            .bind(&request.node_id)
            .bind(endpoint.kind.as_str())
            .bind(i64::from(endpoint.schema_version))
            .bind(&endpoint.addr)
            .bind(i64::from(endpoint.priority))
            .execute(&mut *tx)
            .await?;
        }

        for service in &request.services {
            sqlx::query(
                r#"
                insert into node_services (id, node_id, kind, schema_version, target, user_name, label)
                values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(&service.id)
            .bind(&request.node_id)
            .bind(service.kind.as_str())
            .bind(i64::from(service.schema_version))
            .bind(&service.target)
            .bind(&service.user_name)
            .bind(&service.label)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn list_devices(&self) -> anyhow::Result<DeviceCatalogResponse> {
        let rows = sqlx::query(
            r#"
            select
              n.id as node_id,
              n.label as node_label,
              ns.id as service_id,
              ns.user_name as user_name,
              ne.addr as endpoint_addr
            from nodes n
            left join node_services ns on ns.node_id = n.id and ns.kind = 'ssh' and ns.schema_version = 1
            left join node_endpoints ne on ne.node_id = n.id and ne.kind = 'tcp_proxy' and ne.schema_version = 1
            order by n.id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let devices = rows
            .into_iter()
            .map(|row| {
                let endpoint_addr: Option<String> = row.try_get("endpoint_addr")?;
                let ssh = match (
                    row.try_get::<Option<String>, _>("service_id")?,
                    row.try_get::<Option<String>, _>("user_name")?,
                    endpoint_addr,
                ) {
                    (Some(service_id), Some(user_name), Some(addr)) => {
                        let (host, port) = split_addr(&addr)?;
                        Some(SshEndpoint {
                            service_id,
                            host,
                            port,
                            user: user_name,
                        })
                    }
                    _ => None,
                };

                Ok::<_, anyhow::Error>(DeviceRecord {
                    id: row.try_get("node_id")?,
                    name: row.try_get("node_label")?,
                    ssh,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DeviceCatalogResponse { devices })
    }

    pub async fn resolve_service_route(&self, service_id: &str) -> anyhow::Result<ServiceRoute> {
        let row = sqlx::query(
            r#"
            select
              ns.node_id as node_id,
              ns.user_name as user_name,
              ne.addr as endpoint_addr
            from node_services ns
            join node_endpoints ne on ne.node_id = ns.node_id
            where ns.id = ?1
              and ne.kind = 'tcp_proxy'
              and ne.schema_version = 1
            order by ne.priority desc, ne.id asc
            limit 1
            "#,
        )
        .bind(service_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(ServiceRoute {
            node_id: row.try_get("node_id")?,
            tcp_addr: row.try_get("endpoint_addr")?,
            user_name: row.try_get("user_name")?,
        })
    }
}

fn split_addr(addr: &str) -> anyhow::Result<(String, u16)> {
    let (host, port) = addr
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid addr {}", addr))?;
    Ok((host.to_string(), port.parse()?))
}
