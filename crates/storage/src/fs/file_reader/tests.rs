use http_body_util::BodyExt;

use crate::{FileContentBytes, StorageFileReader};

#[tokio::test]
async fn into_body_streams_in_chunks_instead_of_buffering_entire_payload() -> anyhow::Result<()> {
    let size = 256 * 1024;
    let payload = vec![7u8; size];
    let reader = StorageFileReader::Bytes(FileContentBytes::Content(payload.clone()));
    let mut body = reader.into_body(size);

    let mut total = 0usize;
    let mut max_chunk = 0usize;

    while let Some(frame) = body.frame().await {
        let frame = frame?;
        let chunk = match frame.into_data() {
            Ok(data) => data,
            Err(_frame) => continue,
        };
        total += chunk.len();
        max_chunk = std::cmp::max(max_chunk, chunk.len());
    }

    assert_eq!(total, size);
    assert!(
        max_chunk <= 64 * 1024,
        "max_chunk should be <= 64KiB, got {max_chunk}"
    );
    Ok(())
}
