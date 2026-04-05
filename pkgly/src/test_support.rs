use once_cell::sync::Lazy;

pub static DB_TEST_LOCK: Lazy<tokio::sync::Mutex<()>> =
    Lazy::new(|| tokio::sync::Mutex::new(()));
