use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub fn session_alpn() -> &'static [u8] {
    b"overlay/1"
}

pub struct OpenedStream {
    pub service_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHello {
    pub token: String,
    pub service_id: String,
}

pub async fn write_session_hello<W>(writer: &mut W, hello: &SessionHello) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut payload = serde_json::to_vec(hello)?;
    payload.push(b'\n');
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_session_hello<R>(reader: &mut R) -> anyhow::Result<SessionHello>
where
    R: AsyncRead + Unpin,
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
