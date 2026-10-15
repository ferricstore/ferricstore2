fn forbidden() {
    let _ = vec![std::time::SystemTime::now()];
    let _ = vec![std::env::var("INPUT"); 3];
    let _ = format!("{}", std::fs::read_to_string("INPUT"));
    let _ = matches!(value, Some(_) if std::net::TcpStream::connect("INPUT").is_ok());
    let _ = write!(std::io::stdout(), "{}", 1);
    let _ = write!(formatter, "{now:?}", now = std::time::SystemTime::now());
}
