//! Resident adapter for durable Manual artifact publication.

use std::sync::mpsc::{Receiver, SyncSender};

use crate::assets::AssetPublishService;

use super::{AgentComponent, WorkerCommand, WorkerEvent};

pub(super) fn run_asset_publisher_worker(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    service: Option<&AssetPublishService>,
    tray_sender: Option<&SyncSender<WorkerCommand>>,
) {
    let _ = event_sender.send(WorkerEvent::Started(component));
    loop {
        match command_receiver.recv() {
            Ok(WorkerCommand::Stop) | Err(_) => break,
            Ok(WorkerCommand::PrepareManualAsset { job, result_sender }) => {
                let result = service.map_or_else(
                    || {
                        Err(crate::assets::AssetPublishServiceError::Store(
                            crate::assets::AssetJobStoreError::Io,
                        ))
                    },
                    |service| {
                        service.save_prepared(&job)?;
                        service.bind_prepared(job.id()).map(|_| ())
                    },
                );
                let _ = result_sender.send(result);
            }
            Ok(WorkerCommand::PublishManualAsset { job_id, mut source }) => {
                let result = service.map_or_else(
                    || {
                        Err(crate::assets::AssetPublishServiceError::Store(
                            crate::assets::AssetJobStoreError::Io,
                        ))
                    },
                    |service| service.publish_finalized(&job_id, &mut *source),
                );
                if result.is_err()
                    && let Some(sender) = tray_sender
                {
                    let _ = sender.try_send(WorkerCommand::ShowTrayNotice {
                        kind: crate::tray::TrayNoticeKind::ActionRejected,
                    });
                }
            }
            Ok(_) => {}
        }
    }
}

#[cfg(windows)]
pub(super) fn default_asset_publish_service() -> Option<AssetPublishService> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)?
        .join("Sniptale")
        .join("NativeAgent")
        .join("asset-jobs");
    AssetPublishService::open(root).ok()
}

#[cfg(not(windows))]
pub(super) fn default_asset_publish_service() -> Option<AssetPublishService> {
    None
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread};

    use tempfile::tempdir;

    use super::{AssetPublishService, run_asset_publisher_worker};
    use crate::{
        app::resident::{AgentComponent, WorkerCommand, WorkerEvent},
        assets::{
            AssetFormat, AssetJob, AssetJobId, FinalizedArtifactSource,
            FinalizedArtifactSourceError, ManualDestination,
        },
        transfer::hashing::sha256_hex,
        tray::TrayNoticeKind,
    };

    struct Source {
        bytes: Vec<u8>,
        sha256: String,
        read: bool,
    }

    impl FinalizedArtifactSource for Source {
        fn total_bytes(&self) -> u64 {
            self.bytes.len() as u64
        }
        fn sha256(&self) -> &str {
            &self.sha256
        }
        fn read_next(
            &mut self,
            _maximum_bytes: usize,
        ) -> Result<Option<Vec<u8>>, FinalizedArtifactSourceError> {
            if self.read {
                Ok(None)
            } else {
                self.read = true;
                Ok(Some(self.bytes.clone()))
            }
        }
    }

    #[test]
    fn asset_worker_publishes_a_bound_manual_job() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let bytes = b"\x89PNG\r\n\x1a\npixels".to_vec();
        let (output, job) = prepared_job(directory.path())?;
        let service = AssetPublishService::open(directory.path().join("jobs"))
            .map_err(|_| std::io::Error::other("service"))?;
        let (events_tx, events_rx) = mpsc::sync_channel(1);
        let (commands_tx, commands_rx) = mpsc::sync_channel(1);
        let (tray_tx, tray_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            run_asset_publisher_worker(
                AgentComponent::AssetPublisher,
                &events_tx,
                &commands_rx,
                Some(&service),
                Some(&tray_tx),
            );
        });
        assert_eq!(
            events_rx.recv()?,
            WorkerEvent::Started(AgentComponent::AssetPublisher)
        );
        let sha256 = sha256_hex(&bytes);
        let (prepared_tx, prepared_rx) = mpsc::sync_channel(1);
        commands_tx.send(WorkerCommand::PrepareManualAsset {
            job: Box::new(job),
            result_sender: prepared_tx,
        })?;
        prepared_rx
            .recv()?
            .map_err(|_| std::io::Error::other("prepare"))?;
        let job_id =
            AssetJobId::parse("asset-job-1".to_owned()).map_err(|_| std::io::Error::other("id"))?;
        commands_tx.send(WorkerCommand::PublishManualAsset {
            job_id,
            source: Box::new(Source {
                bytes,
                sha256,
                read: false,
            }),
        })?;
        commands_tx.send(WorkerCommand::Stop)?;
        worker.join().map_err(|_| std::io::Error::other("worker"))?;
        assert!(output.join("capture-1.png").is_file());
        assert!(matches!(
            tray_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ));
        Ok(())
    }

    #[test]
    fn asset_worker_notifies_once_when_manual_publication_fails()
    -> Result<(), Box<dyn std::error::Error>> {
        let (events_tx, events_rx) = mpsc::sync_channel(1);
        let (commands_tx, commands_rx) = mpsc::sync_channel(1);
        let (tray_tx, tray_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            run_asset_publisher_worker(
                AgentComponent::AssetPublisher,
                &events_tx,
                &commands_rx,
                None,
                Some(&tray_tx),
            );
        });
        assert_eq!(
            events_rx.recv()?,
            WorkerEvent::Started(AgentComponent::AssetPublisher)
        );
        commands_tx.send(WorkerCommand::PublishManualAsset {
            job_id: AssetJobId::parse("asset-job-failed".to_owned())
                .map_err(|_| std::io::Error::other("id"))?,
            source: Box::new(Source {
                bytes: b"unpublished".to_vec(),
                sha256: sha256_hex(b"unpublished"),
                read: false,
            }),
        })?;
        commands_tx.send(WorkerCommand::Stop)?;
        worker.join().map_err(|_| std::io::Error::other("worker"))?;
        assert!(matches!(
            tray_rx.recv()?,
            WorkerCommand::ShowTrayNotice {
                kind: TrayNoticeKind::ActionRejected
            }
        ));
        assert!(matches!(
            tray_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ));
        Ok(())
    }

    fn prepared_job(
        path: &std::path::Path,
    ) -> Result<(std::path::PathBuf, AssetJob), std::io::Error> {
        let output = path.join("output");
        std::fs::create_dir(&output)?;
        let destination = ManualDestination::open("destination-1".to_owned(), output.clone())
            .map_err(|_| std::io::Error::other("destination"))?;
        let job = AssetJob::prepare(
            AssetJobId::parse("asset-job-1".to_owned()).map_err(|_| std::io::Error::other("id"))?,
            AssetFormat::Png,
            &destination,
            "capture-1.png".to_owned(),
        )
        .map_err(|_| std::io::Error::other("job"))?;
        Ok((output, job))
    }
}

#[cfg(test)]
#[path = "assets_worker/protected_publish_tests.rs"]
mod protected_publish_tests;
