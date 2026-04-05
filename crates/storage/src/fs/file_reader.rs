use std::{
    fmt::Debug,
    fs::File as SyncFile,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use derive_more::derive::From;
use http_body::{Body, Frame};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt},
};
use tokio_util::io::poll_read_buf;

use super::FileContentBytes;

const DEFAULT_READ_CHUNK_SIZE: usize = 64 * 1024;

/// StorageFileReader is a wrapper around different types of readers.
#[derive(From)]
pub enum StorageFileReader {
    /// File Readers will be the most common type of reader.
    /// For this reason, we will give it a special variant. To prevent dynamic dispatch.
    File(File),
    /// An Async Reader type. This will be used for remote storage. Such as S3.
    AsyncReader(Pin<Box<dyn tokio::io::AsyncRead + Send>>),
    /// Content already in memory.
    Bytes(FileContentBytes),
}
impl StorageFileReader {
    pub async fn read_to_vec(self, size_hint: usize) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(size_hint);
        match self {
            StorageFileReader::File(mut file) => {
                file.read_to_end(&mut buf).await?;
            }
            StorageFileReader::AsyncReader(mut reader) => {
                tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf).await?;
            }
            StorageFileReader::Bytes(bytes) => return Ok(bytes.into()),
        }
        Ok(buf)
    }
}

impl From<SyncFile> for StorageFileReader {
    fn from(file: SyncFile) -> Self {
        StorageFileReader::File(File::from_std(file))
    }
}
impl StorageFileReader {
    /// Convert the reader into an `http_body::Body`.
    ///
    /// `size_hint_bytes` is the total size of the response (if known). It is used only for
    /// the `Body::size_hint` implementation and does not influence buffering.
    pub fn into_body(self, size_hint_bytes: usize) -> StorageFileReaderBody {
        let chunk_capacity = size_hint_bytes.clamp(1, DEFAULT_READ_CHUNK_SIZE);
        StorageFileReaderBody {
            reader: Some(self),
            buf: BytesMut::with_capacity(chunk_capacity),
            chunk_capacity,
            size_hint_bytes,
        }
    }
}
impl Debug for StorageFileReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageFileReader::File(_) => f.write_str("StorageFileReader::File"),
            StorageFileReader::AsyncReader(_) => f.write_str("StorageFileReader::AsyncReader"),
            StorageFileReader::Bytes(_) => f.write_str("StorageFileReader::Bytes"),
        }
    }
}
impl AsyncRead for StorageFileReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            StorageFileReader::File(file) => Pin::new(file).poll_read(cx, buf),
            StorageFileReader::AsyncReader(reader) => Pin::new(reader).poll_read(cx, buf),
            StorageFileReader::Bytes(bytes) => match bytes {
                FileContentBytes::Content(content) => {
                    let len = std::cmp::min(buf.remaining(), content.len());
                    if len == 0 {
                        return Poll::Ready(Ok(()));
                    }
                    buf.put_slice(&content[..len]);
                    content.drain(..len);
                    Poll::Ready(Ok(()))
                }
                FileContentBytes::Bytes(bytes) => {
                    let len = std::cmp::min(buf.remaining(), bytes.len());
                    if len == 0 {
                        return Poll::Ready(Ok(()));
                    }
                    let chunk = bytes.split_to(len);
                    buf.put_slice(&chunk);
                    Poll::Ready(Ok(()))
                }
            },
        }
    }
}
#[pin_project::pin_project]
#[derive(Debug)]
pub struct StorageFileReaderBody {
    #[pin]
    reader: Option<StorageFileReader>,
    buf: BytesMut,
    chunk_capacity: usize,
    size_hint_bytes: usize,
}
impl Body for StorageFileReaderBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let mut this = self.as_mut().project();

        let reader = match this.reader.as_pin_mut() {
            Some(r) => r,
            None => return Poll::Ready(None),
        };

        if this.buf.capacity() == 0 {
            this.buf.reserve(*this.chunk_capacity);
        }

        match poll_read_buf(reader, cx, &mut this.buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => {
                self.project().reader.set(None);
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(Ok(0)) => {
                self.project().reader.set(None);
                Poll::Ready(None)
            }
            Poll::Ready(Ok(_)) => {
                let chunk = this.buf.split();
                let frozen = chunk.freeze();
                Poll::Ready(Some(Ok(Frame::data(frozen))))
            }
        }
    }
    fn is_end_stream(&self) -> bool {
        self.reader.is_none()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        let mut hint = http_body::SizeHint::default();
        if self.size_hint_bytes > 0 {
            hint.set_lower(self.size_hint_bytes as u64);
            hint.set_upper(self.size_hint_bytes as u64);
        }
        hint
    }
}

#[cfg(test)]
mod tests;
