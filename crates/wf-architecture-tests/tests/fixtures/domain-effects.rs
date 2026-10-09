use std::{env as environment, fs, net, sync::atomic::AtomicU64, time::SystemTime};
async fn effect() { operation().await; }
fn noise() { println!("effect"); }
