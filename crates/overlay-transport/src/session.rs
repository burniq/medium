use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub fn session_alpn() -> &'static [u8] {
    b"overlay/1"
}

pub struct OpenedStream {
    pub service_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum RelayHello {
    Node {
        node_id: String,
        shared_secret: String,
    },
    Client {
        node_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHello {
    pub token: String,
    pub service_id: String,
}

pub async fn write_relay_hello<W>(writer: &mut W, hello: &RelayHello) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin,
{
    write_json_line(writer, hello).await
}

pub async fn read_relay_hello<R>(reader: &mut R) -> anyhow::Result<RelayHello>
where
    R: AsyncRead + Unpin,
{
    read_json_line(reader).await
}

pub async fn write_session_hello<W>(writer: &mut W, hello: &SessionHello) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin,
{
    write_json_line(writer, hello).await
}

pub async fn read_session_hello<R>(reader: &mut R) -> anyhow::Result<SessionHello>
where
    R: AsyncRead + Unpin,
{
    read_json_line(reader).await
}

async fn write_json_line<W, T>(writer: &mut W, value: &T) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut payload = serde_json::to_vec(value)?;
    payload.push(b'\n');
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

async fn read_json_line<R, T>(reader: &mut R) -> anyhow::Result<T>
where
    R: AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut line = Vec::new();
    loop {
        let byte = reader.read_u8().await?;
        if byte == b'\n' {
            break;
        }
        line.push(byte);
        if line.len() > 16 * 1024 {
            anyhow::bail!("session hello too large");
        }
    }
    if line.is_empty() {
        anyhow::bail!("missing session hello");
    }
    Ok(serde_json::from_slice(&line)?)
}
