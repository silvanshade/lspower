//! Hashmaps for tracking pending JSON-RPC requests.

use super::{Error, Id, Response, Result};
use dashmap::{mapref::entry::Entry, DashMap};
use futures::{channel::oneshot, future};
use serde::Serialize;
use std::{
    fmt::{self, Debug, Formatter},
    future::Future,
    sync::Arc,
};

/// A hashmap containing pending server requests, keyed by request ID.
pub struct ServerRequests(Arc<DashMap<Id, future::AbortHandle>>);

impl ServerRequests {
    /// Creates a new pending server requests map.
    pub fn new() -> Self {
        ServerRequests(Arc::new(DashMap::new()))
    }

    /// Executes the given async request handler, keyed by the given request ID.
    ///
    /// If a cancel request is issued before the future is finished resolving, this will resolve to
    /// a "canceled" error response, and the pending request handler future will be dropped.
    pub fn execute<F, T>(&self, id: Id, fut: F) -> impl Future<Output = Response> + Send + 'static
    where
        F: Future<Output = Result<T>> + Send + 'static,
        T: Serialize,
    {
        if let Entry::Vacant(entry) = self.0.entry(id.clone()) {
            let (handler_fut, abort_handle) = future::abortable(fut);
            entry.insert(abort_handle);

            let requests = self.0.clone();
            future::Either::Left(async move {
                let abort_result = handler_fut.await;
                requests.remove(&id); // Remove abort handle now to avoid double cancellation.

                if let Ok(handler_result) = abort_result {
                    let result = handler_result.map(|v| serde_json::to_value(v).unwrap());
                    Response::from_parts(id, result)
                } else {
                    Response::error(Some(id), Error::request_cancelled())
                }
            })
        } else {
            future::Either::Right(async { Response::error(Some(id), Error::invalid_request()) })
        }
    }

    /// Attempts to cancel the running request handler corresponding to this ID.
    ///
    /// This will force the future to resolve to a "canceled" error response. If the future has
    /// already completed, this method call will do nothing.
    pub fn cancel(&self, id: &Id) {
        if let Some((_, handle)) = self.0.remove(id) {
            handle.abort();
            log::info!("successfully cancelled request with ID: {}", id);
        } else {
            log::warn!(
                "client asked to cancel request {}, but no such pending request exists, ignoring",
                id
            );
        }
    }

    /// Cancels all pending request handlers, if any.
    pub fn cancel_all(&self) {
        self.0.retain(|_, handle| {
            handle.abort();
            false
        });
    }
}

impl Debug for ServerRequests {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_set()
            .entries(self.0.iter().map(|entry| entry.key().clone()))
            .finish()
    }
}

/// A hashmap containing pending client requests, keyed by request ID.
pub struct ClientRequests(DashMap<Id, oneshot::Sender<Response>>);

impl ClientRequests {
    /// Creates a new pending client requests map.
    pub fn new() -> Self {
        ClientRequests(DashMap::new())
    }

    /// Inserts the given response into the map.
    ///
    /// The corresponding `.wait()` future will then resolve to the given value.
    pub fn insert(&self, r: Response) {
        match r.id() {
            None => log::warn!("received response with request ID of `null`, ignoring"),
            Some(id) => match self.0.remove(id) {
                Some((_, tx)) => tx.send(r).expect("receiver already dropped"),
                None => log::warn!("received response with unknown request ID: {}", id),
            },
        }
    }

    /// Marks the given request ID as pending and waits for its corresponding response to arrive.
    ///
    /// # Panics
    ///
    /// Panics if the request ID is already in the hashmap and is pending a matching response. This
    /// should never happen provided that a monotonically increasing `id` value is used.
    pub fn wait(&self, id: Id) -> impl Future<Output = Response> + Send + 'static {
        match self.0.entry(id) {
            Entry::Vacant(entry) => {
                let (tx, rx) = oneshot::channel();
                entry.insert(tx);
                async { rx.await.expect("sender already dropped") }
            },
            _ => panic!("concurrent waits for the same request ID can't happen, this is a bug"),
        }
    }
}

impl Debug for ClientRequests {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_set()
            .entries(self.0.iter().map(|entry| entry.key().clone()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod client_requests {
        use super::*;
        use serde_json::json;

        #[test]
        fn debug() {
            let client_requests = ClientRequests::new();
            format!("{:?}", client_requests);
        }

        #[tokio::test]
        #[should_panic]
        async fn wait_current() {
            let pending = ClientRequests::new();
            let id = Id::Number(1);
            tokio::spawn(pending.wait(id.clone()));
            tokio::spawn(pending.wait(id.clone()));
        }

        #[tokio::test]
        async fn wait_insert() {
            let pending = ClientRequests::new();

            let id = Id::Number(1);
            let wait_fut = tokio::spawn(pending.wait(id.clone()));

            let expected = Response::ok(id.clone(), json!({}));
            pending.insert(expected.clone());

            let actual = wait_fut.await.expect("task panicked");
            assert_eq!(expected, actual);
        }

        #[tokio::test]
        async fn unbalanced_insert() {
            let pending = ClientRequests::new();
            let id = Id::Number(1);
            let expected = Response::ok(id.clone(), json!({}));
            pending.insert(expected.clone());
        }
    }

    mod server_requests {
        use super::*;
        use serde_json::json;
        use std::time::Duration;

        #[test]
        fn debug() {
            let server_requests = ServerRequests::new();
            format!("{:?}", server_requests);
        }

        #[tokio::test]
        async fn execute() {
            let pending = ServerRequests::new();

            let id = Id::Number(1);
            let response = pending.execute(id.clone(), async { Ok(json!({})) }).await;

            assert_eq!(response, Response::ok(id, json!({})));
        }

        #[tokio::test]
        async fn execute_concurrent() {
            let pending = ServerRequests::new();
            let id = Id::Number(1);
            let fut0 = pending.execute(id.clone(), async { Ok(json!({})) });
            let fut1 = pending.execute(id.clone(), async { Ok(json!({})) });
            assert_eq!(fut0.await, Response::ok(id.clone(), json!({})));
            assert_eq!(fut1.await, Response::error(Some(id.clone()), Error::invalid_request()));
        }

        #[tokio::test]
        async fn cancel() {
            let pending = ServerRequests::new();

            let id = Id::Number(1);
            let handler_fut = tokio::spawn(pending.execute(id.clone(), async {
                tokio::time::sleep(Duration::from_secs(50)).await;
                Ok(json!({}))
            }));

            tokio::time::sleep(Duration::from_millis(30)).await;
            pending.cancel(&id);

            let res = handler_fut.await.expect("task panicked");
            assert_eq!(res, Response::error(Some(id), Error::request_cancelled()));
        }

        #[tokio::test]
        async fn cancel_non_existent() {
            let pending = ServerRequests::new();
            let id = Id::Number(1);
            pending.cancel(&id);
        }

        #[tokio::test]
        async fn cancel_all() {
            let pending = ServerRequests::new();

            let id1 = Id::Number(1);
            let handler_fut1 = tokio::spawn(pending.execute(id1.clone(), async {
                tokio::time::sleep(Duration::from_secs(50)).await;
                Ok(json!({}))
            }));

            let id2 = Id::Number(2);
            let handler_fut2 = tokio::spawn(pending.execute(id2.clone(), async {
                tokio::time::sleep(Duration::from_secs(50)).await;
                Ok(json!({}))
            }));

            tokio::time::sleep(Duration::from_millis(30)).await;
            pending.cancel_all();

            let res1 = handler_fut1.await.expect("task panicked");
            assert_eq!(res1, Response::error(Some(id1), Error::request_cancelled()));

            let res2 = handler_fut2.await.expect("task panicked");
            assert_eq!(res2, Response::error(Some(id2), Error::request_cancelled()));
        }
    }
}
