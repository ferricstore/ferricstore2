//! Reopening must reject missing/foreign/resealed data before serving captures.

use super::*;

#[test]
fn native_record_query_reopen_rejects_missing_current_and_historical_companions() -> TestResult {
    for family in [1_u8, 5, 10, 11] {
        let directory = crate::fixtures::directory(&format!("record-query-missing-{family}"))?;
        let path = directory.join("state");
        let mut store = fresh(&path)?;
        let data = metadata(7);
        apply(
            &mut store,
            &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
            0,
        )?;
        drop(store);
        let mut rows = crate::fixtures::raw_rows(&path)?;
        let key = rows
            .keys()
            .find(|key| key.get(6) == Some(&family))
            .ok_or("family")?
            .clone();
        rows.remove(&key).ok_or("row")?;
        replace_rows(&path, &rows)?;
        assert!(
            matches!(crate::fixtures::open(&path), Err(error) if error.to_string().contains("corrupt"))
        );
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}
