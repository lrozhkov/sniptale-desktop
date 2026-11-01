//! Shared bounded writer supervision; an unfinished worker always remains process-owned.

use crate::{
    protocol::DeliveryRoute,
    telemetry::CursorTrajectorySinkError,
    transfer::{
        events_store::{EventsArchive, EventsWriter},
        trajectory_store::{TrajectoryArchive, TrajectoryStoreError, TrajectoryWriter},
    },
};
use std::{
    path::Path,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::JoinHandle,
    time::Duration,
};

pub(super) const FINALIZE_COMPLETE: u8 = 1;
pub(super) const FINALIZE_INCOMPLETE: u8 = 2;
const ERASE: u8 = 3;
const WRITER_WAIT: Duration = Duration::from_secs(2);

pub(super) trait HistoryArchive: std::fmt::Debug + Send + 'static {
    type Descriptor: std::fmt::Debug + Clone + Send;
    fn descriptor(&self) -> &Self::Descriptor;
    fn open(root: &Path, recording: &str, source: &str) -> Result<Self, TrajectoryStoreError>
    where
        Self: Sized;
    fn recover(&mut self) -> Result<(), TrajectoryStoreError>;
    fn erase(self) -> Result<(), TrajectoryStoreError>;
}

pub(super) trait HistoryWriter: std::fmt::Debug + Send + 'static {
    type Descriptor: std::fmt::Debug + Clone + Send;
    type Archive: HistoryArchive<Descriptor = Self::Descriptor>;
    const NAME: &'static str;
    fn create(
        root: &Path,
        recording: &str,
        source: &str,
        route: DeliveryRoute,
    ) -> Result<Self, TrajectoryStoreError>
    where
        Self: Sized;
    fn descriptor(&self) -> &Self::Descriptor;
    fn append(&mut self, bytes: &[u8]) -> Result<(), TrajectoryStoreError>;
    fn finalize(&mut self, incomplete: bool) -> Result<Self::Descriptor, TrajectoryStoreError>;
    fn into_archive(self) -> Self::Archive;
}

macro_rules! history_backend {
    ($writer:ty, $archive:ty, $descriptor:ty, $name:literal) => {
        impl HistoryWriter for $writer {
            type Descriptor = $descriptor;
            type Archive = $archive;
            const NAME: &'static str = $name;
            fn create(
                root: &Path,
                recording: &str,
                source: &str,
                route: DeliveryRoute,
            ) -> Result<Self, TrajectoryStoreError> {
                <$writer>::create(root, recording, source, route)
            }
            fn descriptor(&self) -> &Self::Descriptor {
                <$writer>::descriptor(self)
            }
            fn append(&mut self, bytes: &[u8]) -> Result<(), TrajectoryStoreError> {
                <$writer>::append(self, bytes)
            }
            fn finalize(
                &mut self,
                incomplete: bool,
            ) -> Result<Self::Descriptor, TrajectoryStoreError> {
                <$writer>::finalize(self, incomplete)
            }
            fn into_archive(self) -> Self::Archive {
                <$writer>::into_archive(self)
            }
        }
        impl HistoryArchive for $archive {
            type Descriptor = $descriptor;
            fn descriptor(&self) -> &Self::Descriptor {
                <$archive>::descriptor(self)
            }
            fn open(
                root: &Path,
                recording: &str,
                source: &str,
            ) -> Result<Self, TrajectoryStoreError> {
                <$archive>::open(root, recording, source)
            }
            fn recover(&mut self) -> Result<(), TrajectoryStoreError> {
                self.recover_interrupted()
            }
            fn erase(self) -> Result<(), TrajectoryStoreError> {
                <$archive>::erase(self)
            }
        }
    };
}

history_backend!(
    TrajectoryWriter,
    TrajectoryArchive,
    crate::protocol::trajectory::CursorTrajectoryDescriptor,
    "cursor-trajectory-writer"
);
history_backend!(
    EventsWriter,
    EventsArchive,
    crate::protocol::recording_events::EventDescriptor,
    "recording-events-writer"
);

#[derive(Debug, Default)]
pub(super) struct WriterSlot {
    pub(super) active: bool,
    pub(super) parked: Option<JoinHandle<()>>,
}

#[derive(Debug)]
pub(super) struct WriterStatus<D> {
    pub(super) descriptor: D,
    pub(super) done: bool,
    pub(super) erased: bool,
}

type SharedStatus<D> = Arc<(Mutex<WriterStatus<D>>, Condvar)>;

#[derive(Debug)]
pub(super) struct HistoryWriterSink<W: HistoryWriter> {
    pub(super) sender: SyncSender<Vec<u8>>,
    pub(super) control: Arc<AtomicU8>,
    pub(super) status: SharedStatus<W::Descriptor>,
    pub(super) worker: Option<JoinHandle<()>>,
    pub(super) slot: Arc<Mutex<WriterSlot>>,
}

pub(super) fn spawn_writer<W: HistoryWriter>(
    root: &Path,
    slot: &Arc<Mutex<WriterSlot>>,
    recording: &str,
    source: &str,
    route: DeliveryRoute,
) -> Result<HistoryWriterSink<W>, CursorTrajectorySinkError> {
    let mut state = slot
        .lock()
        .map_err(|_| CursorTrajectorySinkError::Storage)?;
    if state.active
        || state
            .parked
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    {
        return Err(CursorTrajectorySinkError::Backpressure);
    }
    if let Some(worker) = state.parked.take() {
        worker
            .join()
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
    }
    let writer = W::create(root, recording, source, route)
        .map_err(|_| CursorTrajectorySinkError::Storage)?;
    let status = Arc::new((
        Mutex::new(WriterStatus {
            descriptor: writer.descriptor().clone(),
            done: false,
            erased: false,
        }),
        Condvar::new(),
    ));
    let control = Arc::new(AtomicU8::new(0));
    let (sender, receiver) = mpsc::sync_channel(2);
    let worker_status = status.clone();
    let worker_control = control.clone();
    let root = root.to_owned();
    let recording = recording.to_owned();
    let source = source.to_owned();
    let worker = std::thread::Builder::new()
        .name(W::NAME.to_owned())
        .spawn(move || {
            run_writer::<W>(
                writer,
                &receiver,
                &worker_control,
                &worker_status,
                &root,
                &recording,
                &source,
            );
        })
        .map_err(|_| CursorTrajectorySinkError::Storage)?;
    state.active = true;
    Ok(HistoryWriterSink {
        sender,
        control,
        status,
        worker: Some(worker),
        slot: slot.clone(),
    })
}

impl<W: HistoryWriter> HistoryWriterSink<W> {
    fn wait(&self) -> std::sync::MutexGuard<'_, WriterStatus<W::Descriptor>> {
        let state = self
            .status
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match self
            .status
            .1
            .wait_timeout_while(state, WRITER_WAIT, |state| !state.done)
        {
            Ok((state, _)) => state,
            Err(poisoned) => poisoned.into_inner().0,
        }
    }

    pub(super) fn try_append(&mut self, bytes: Vec<u8>) -> Result<(), CursorTrajectorySinkError> {
        if self.control.load(Ordering::Acquire) != 0 {
            return Err(CursorTrajectorySinkError::Retired);
        }
        self.sender.try_send(bytes).map_err(|error| {
            self.control
                .fetch_max(FINALIZE_INCOMPLETE, Ordering::AcqRel);
            match error {
                TrySendError::Full(_) => CursorTrajectorySinkError::Backpressure,
                TrySendError::Disconnected(_) => CursorTrajectorySinkError::Storage,
            }
        })
    }

    pub(super) fn finish(&mut self, incomplete: bool) -> W::Descriptor {
        self.control.fetch_max(
            if incomplete {
                FINALIZE_INCOMPLETE
            } else {
                FINALIZE_COMPLETE
            },
            Ordering::AcqRel,
        );
        self.wait().descriptor.clone()
    }

    pub(super) fn erase(&mut self) -> Result<(), CursorTrajectorySinkError> {
        self.status
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .done = false;
        self.control.store(ERASE, Ordering::Release);
        let state = self.wait();
        if state.done && state.erased {
            Ok(())
        } else {
            Err(CursorTrajectorySinkError::Storage)
        }
    }
}

impl<W: HistoryWriter> Drop for HistoryWriterSink<W> {
    fn drop(&mut self) {
        self.control
            .compare_exchange(0, FINALIZE_INCOMPLETE, Ordering::AcqRel, Ordering::Acquire)
            .ok();
        let Some(worker) = self.worker.take() else {
            return;
        };
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.active = false;
        slot.parked = Some(worker);
    }
}

fn publish<D>(status: &SharedStatus<D>, descriptor: D, done: bool, erased: bool) {
    *status
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = WriterStatus {
        descriptor,
        done,
        erased,
    };
    status.1.notify_all();
}

fn run_writer<W: HistoryWriter>(
    mut writer: W,
    receiver: &Receiver<Vec<u8>>,
    control: &AtomicU8,
    status: &SharedStatus<W::Descriptor>,
    root: &Path,
    recording: &str,
    source: &str,
) {
    let mut failed = false;
    while control.load(Ordering::Acquire) == 0 {
        match receiver.recv_timeout(Duration::from_millis(16)) {
            Ok(bytes) => {
                if writer.append(&bytes).is_err() {
                    failed = true;
                    break;
                }
                publish(status, writer.descriptor().clone(), false, false);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                control.fetch_max(FINALIZE_INCOMPLETE, Ordering::AcqRel);
            }
        }
    }
    if failed {
        control.fetch_max(FINALIZE_INCOMPLETE, Ordering::AcqRel);
    }
    while let Ok(bytes) = receiver.try_recv() {
        if !failed && writer.append(&bytes).is_err() {
            failed = true;
        }
    }
    let last = writer.descriptor().clone();
    if control.load(Ordering::Acquire) == ERASE {
        let erased = writer.into_archive().erase().is_ok();
        publish(status, last, true, erased);
        return;
    }
    if !failed
        && let Ok(descriptor) =
            writer.finalize(control.load(Ordering::Acquire) != FINALIZE_COMPLETE)
    {
        publish(status, descriptor, true, false);
        wait_for_disposal(writer.into_archive(), receiver, control, status);
        return;
    }
    drop(writer);
    match W::Archive::open(root, recording, source) {
        Ok(mut archive) => {
            let recovered = archive.recover().is_ok();
            publish(
                status,
                if recovered {
                    archive.descriptor().clone()
                } else {
                    last
                },
                true,
                false,
            );
            wait_for_disposal(archive, receiver, control, status);
        }
        Err(_) => publish(status, last, true, false),
    }
}

fn wait_for_disposal<A: HistoryArchive>(
    archive: A,
    receiver: &Receiver<Vec<u8>>,
    control: &AtomicU8,
    status: &SharedStatus<A::Descriptor>,
) {
    while control.load(Ordering::Acquire) != ERASE {
        if matches!(
            receiver.recv_timeout(Duration::from_millis(16)),
            Err(RecvTimeoutError::Disconnected)
        ) {
            return;
        }
    }
    let descriptor = archive.descriptor().clone();
    let erased = archive.erase().is_ok();
    publish(status, descriptor, true, erased);
}

#[cfg(test)]
mod tests;
