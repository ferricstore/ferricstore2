pub(crate) fn check(source: &str, runtime: bool, integration_test: bool) -> Option<String> {
    let limit = if runtime && !integration_test {
        400
    } else {
        600
    };
    let lines = source.lines().count();
    (lines > limit).then(|| {
        format!(
            "{lines} physical lines exceeds {limit}; split the file by responsibility without weakening module boundaries"
        )
    })
}
