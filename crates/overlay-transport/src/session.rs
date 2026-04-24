pub fn session_alpn() -> &'static [u8] {
    b"overlay/1"
}

pub struct OpenedStream {
    pub service_id: String,
}
