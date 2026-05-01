pub fn line<I, K, V>(phase: &str, result: &str, fields: I) -> String
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut output = format!(
        "p2p_diag phase={} result={}",
        format_value(phase),
        format_value(result)
    );
    for (key, value) in fields {
        output.push(' ');
        output.push_str(key.as_ref());
        output.push('=');
        output.push_str(&format_value(value.as_ref()));
    }
    output
}

fn format_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\')
    {
        return value.to_string();
    }

    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(ch),
        }
    }
    output.push('"');
    output
}
