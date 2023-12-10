use std::task::Poll;

use futures_core::Stream;

pub struct CloseableChannel<T> {
    inner: tokio_stream::wrappers::ReceiverStream<Option<T>>,
}

impl<T> CloseableChannel<T> {
    pub fn new(inner: tokio_stream::wrappers::ReceiverStream<Option<T>>) -> Self {
        Self { inner }
    }
}

impl<T> Unpin for CloseableChannel<T> {}

impl<T> Stream for CloseableChannel<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(v) = self.inner.as_mut().poll_recv(cx) {
            match v {
                Some(v) => Poll::Ready(v),
                None => Poll::Ready(None),
            }
        } else {
            Poll::Pending
        }
    }
}
