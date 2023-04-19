//! Daemon Server
//!
//! This module houses the daemon server, some implementation notes for which
//! are below.
//!
//! ## Implementation Notes
//!
//! The basic goals of the daemon are to watch for, and be able to provide
//! details about, filesystem changes. It is organised as an async server, which
//! holds a `HashGlobWatcher` which holds data about hashes, globs to watch for
//! that hash, and files that have been updated for that hash. In addition, this
//! server can be interrogated over grpc to register interest in particular
//! globs, and to query for changes for those globs.

use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use globwatch::{StopSource, Watcher};
use tokio::{
    select,
    signal::ctrl_c,
    sync::{
        oneshot::{Receiver, Sender},
        Mutex,
    },
};
use tonic::transport::{NamedService, Server};
use turborepo_paths::{AbsoluteNormalizedPathBuf, ForwardRelativePath};

use super::{
    bump_timeout::BumpTimeout,
    proto::{self},
    DaemonError,
};
use crate::{commands::CommandBase, get_version, globwatcher::HashGlobWatcher};

pub struct DaemonServer<T: Watcher> {
    daemon_root: AbsoluteNormalizedPathBuf,
    log_file: AbsoluteNormalizedPathBuf,

    start_time: Instant,
    timeout: Duration,
    timeout_time: Arc<BumpTimeout>,

    watcher: Arc<HashGlobWatcher<T>>,
    shutdown: Mutex<Option<Sender<()>>>,
    shutdown_rx: Option<Receiver<()>>,
}

#[derive(PartialEq, Debug)]
pub enum CloseReason {
    Timeout,
    Shutdown,
    WatcherClosed,
    ServerClosed,
    Interrupt,
}

impl DaemonServer<notify::RecommendedWatcher> {
    pub fn new(
        base: &CommandBase,
        timeout: Duration,
        log_file: AbsoluteNormalizedPathBuf,
    ) -> Result<Self, DaemonError> {
        let daemon_root = base.daemon_file_root();

        let watcher = Arc::new(HashGlobWatcher::new(
            daemon_root
                .join(ForwardRelativePath::new("flush").expect("valid forward path"))
                .to_path_buf(),
        )?);

        let (send_shutdown, recv_shutdown) = tokio::sync::oneshot::channel::<()>();

        Ok(Self {
            daemon_root,
            log_file,

            start_time: Instant::now(),
            timeout_time: Arc::new(BumpTimeout::new(timeout)),
            timeout,

            watcher,
            shutdown: Mutex::new(Some(send_shutdown)),
            shutdown_rx: Some(recv_shutdown),
        })
    }
}

impl<T: Watcher + Send + 'static> DaemonServer<T> {
    /// Serve the daemon server, while also watching for filesystem changes.
    pub async fn serve(mut self, repo_root: AbsoluteNormalizedPathBuf) -> CloseReason {
        let stop = StopSource::new();
        let watcher = self.watcher.clone();
        let watcher_fut = watcher.watch(repo_root.to_path_buf(), stop.token());

        let timer = self.timeout_time.clone();
        let timeout_fut = timer.wait();

        // if shutdown is available, then listen. otherwise just wait forever
        let shutdown_rx = self.shutdown_rx.take();
        let shutdown_fut = async move {
            match shutdown_rx {
                Some(rx) => {
                    rx.await.ok();
                }
                None => {
                    futures::pending!();
                }
            }
        };

        #[cfg(feature = "http")]
        let server_fut = {
            // set up grpc reflection
            let efd = include_bytes!("file_descriptor_set.bin");
            let reflection = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(efd)
                .build()
                .unwrap();

            Server::builder()
                .add_service(reflection)
                .add_service(crate::daemon::proto::turbod_server::TurbodServer::new(self))
                .serve_with_shutdown("127.0.0.1:5000".parse().unwrap(), shutdown_fut)
        };

        #[cfg(not(feature = "http"))]
        let (_lock, server_fut) = {
            let (lock, stream) = crate::daemon::endpoint::open_socket(self.daemon_root.clone())
                .await
                .unwrap();

            (
                lock,
                Server::builder()
                    .add_service(crate::daemon::proto::turbod_server::TurbodServer::new(self))
                    .serve_with_incoming(stream),
            )
        };

        select! {
            _ = server_fut => CloseReason::ServerClosed,
            _ = watcher_fut => CloseReason::WatcherClosed,
            _ = shutdown_fut => CloseReason::Shutdown,
            _ = timeout_fut => CloseReason::Timeout,
            _ = ctrl_c() => CloseReason::Interrupt,
        }

        // here the stop token is dropped, and the pid lock is dropped
        // causing them to be cleaned up
    }

    fn bump_timeout(&self) {
        self.timeout_time.reset();
    }
}

#[tonic::async_trait]
impl<T: Watcher + Send + 'static> proto::turbod_server::Turbod for DaemonServer<T> {
    async fn hello(
        &self,
        request: tonic::Request<proto::HelloRequest>,
    ) -> Result<tonic::Response<proto::HelloResponse>, tonic::Status> {
        self.bump_timeout();

        if request.into_inner().version != get_version() {
            return Err(tonic::Status::unimplemented("version mismatch"));
        } else {
            Ok(tonic::Response::new(proto::HelloResponse {}))
        }
    }

    async fn shutdown(
        &self,
        _request: tonic::Request<proto::ShutdownRequest>,
    ) -> Result<tonic::Response<proto::ShutdownResponse>, tonic::Status> {
        self.shutdown.lock().await.take().map(|s| s.send(()));

        // if Some(Ok), then the server is shutting down now
        // if Some(Err), then the server is already shutting down
        // if None, then someone has already called shutdown
        Ok(tonic::Response::new(proto::ShutdownResponse {}))
    }

    async fn status(
        &self,
        _request: tonic::Request<proto::StatusRequest>,
    ) -> Result<tonic::Response<proto::StatusResponse>, tonic::Status> {
        self.bump_timeout();

        Ok(tonic::Response::new(proto::StatusResponse {
            daemon_status: Some(proto::DaemonStatus {
                uptime_msec: self.start_time.elapsed().as_millis() as u64,
                log_file: self.log_file.to_str().unwrap().to_string(),
            }),
        }))
    }

    async fn notify_outputs_written(
        &self,
        request: tonic::Request<proto::NotifyOutputsWrittenRequest>,
    ) -> Result<tonic::Response<proto::NotifyOutputsWrittenResponse>, tonic::Status> {
        self.bump_timeout();

        let inner = request.into_inner();
        self.watcher
            .watch_globs(inner.hash, inner.output_globs, inner.output_exclusion_globs)
            .await;

        Ok(tonic::Response::new(proto::NotifyOutputsWrittenResponse {}))
    }

    async fn get_changed_outputs(
        &self,
        request: tonic::Request<proto::GetChangedOutputsRequest>,
    ) -> Result<tonic::Response<proto::GetChangedOutputsResponse>, tonic::Status> {
        self.bump_timeout();

        let inner = request.into_inner();
        let changed = self
            .watcher
            .changed_globs(&inner.hash, HashSet::from_iter(inner.output_globs))
            .await;

        Ok(tonic::Response::new(proto::GetChangedOutputsResponse {
            changed_output_globs: changed.into_iter().collect(),
        }))
    }
}

impl<T: Watcher> NamedService for DaemonServer<T> {
    const NAME: &'static str = "turborepo.Daemon";
}

#[cfg(test)]
mod test {
    use std::time::{Duration, Instant};

    use tokio::select;
    use turborepo_paths::{AbsoluteNormalizedPathBuf, ForwardRelativePath};

    use super::DaemonServer;
    use crate::{commands::CommandBase, Args};

    #[tokio::test]
    async fn lifecycle() {
        let tempdir = tempfile::tempdir().unwrap();
        let path: AbsoluteNormalizedPathBuf = tempdir.into_path().try_into().unwrap();

        let daemon = DaemonServer::new(
            &CommandBase::new(
                Args {
                    ..Default::default()
                },
                path.as_path().to_path_buf(),
            )
            .unwrap(),
            Duration::from_secs(60 * 60),
            path.clone(),
        )
        .unwrap();

        let pid_path = path.join(ForwardRelativePath::new("turbod.pid").unwrap());
        let sock_path = path.join(ForwardRelativePath::new("turbod.sock").unwrap());

        select! {
            _ = daemon.serve(path) => panic!("must not close"),
            _ = tokio::time::sleep(Duration::from_millis(10)) => (),
        }

        assert!(!pid_path.exists(), "pid file must be deleted");
        assert!(!sock_path.exists(), "socket file must be deleted");
    }

    #[tokio::test]
    async fn timeout() {
        let tempdir = tempfile::tempdir().unwrap();
        let path: AbsoluteNormalizedPathBuf = tempdir.into_path().try_into().unwrap();

        let daemon = DaemonServer::new(
            &CommandBase::new(
                Args {
                    ..Default::default()
                },
                path.as_path().to_path_buf(),
            )
            .unwrap(),
            Duration::from_millis(5),
            path.clone(),
        )
        .unwrap();

        let pid_path = path.join(ForwardRelativePath::new("turbod.pid").unwrap());

        let now = Instant::now();
        let close_reason = daemon.serve(path).await;

        assert!(
            now.elapsed() >= Duration::from_millis(5),
            "must wait at least 5ms"
        );
        assert_eq!(
            super::CloseReason::Timeout,
            close_reason,
            "must close due to timeout"
        );
        assert!(!pid_path.exists(), "pid file must be deleted");
    }
}
