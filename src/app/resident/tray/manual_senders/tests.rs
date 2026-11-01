use std::{sync::mpsc, thread};

use super::ManualWorkerSenders;
use crate::app::resident::WorkerCommand;

#[test]
fn mode_fence_waits_for_each_runtime_port() -> Result<(), Box<dyn std::error::Error>> {
    let (capture_sender, capture_receiver) = mpsc::sync_channel(1);
    let (recording_sender, recording_receiver) = mpsc::sync_channel(1);
    let (hotkey_sender, hotkey_receiver) = mpsc::sync_channel(1);
    let (pipe_sender, pipe_receiver) = mpsc::sync_channel(1);
    let media_workers = [capture_receiver, recording_receiver].map(|receiver| {
        thread::spawn(move || {
            for _ in 0..2 {
                if let Ok(WorkerCommand::FenceMode { result_sender }) = receiver.recv() {
                    let _ = result_sender.send(false);
                }
            }
        })
    });
    let ingress_workers = [hotkey_receiver, pipe_receiver].map(|receiver| {
        thread::spawn(move || {
            if let Ok(WorkerCommand::FenceMode { result_sender }) = receiver.recv() {
                let _ = result_sender.send(false);
            }
        })
    });
    let senders = ManualWorkerSenders::new(
        Some(capture_sender),
        Some(recording_sender),
        None,
        Some(hotkey_sender),
        Some(pipe_sender),
    );

    senders
        .quiesce_extension_inputs()
        .map_err(|()| std::io::Error::other("fence"))?;
    for worker in media_workers.into_iter().chain(ingress_workers) {
        worker.join().map_err(|_| std::io::Error::other("worker"))?;
    }
    Ok(())
}

#[test]
fn mode_fence_ignores_media_work_completed_before_the_transition() -> Result<(), std::io::Error> {
    let (capture_sender, capture_receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        for accepted in [true, false] {
            if let Ok(WorkerCommand::FenceMode { result_sender }) = capture_receiver.recv() {
                let _ = result_sender.send(accepted);
            }
        }
    });
    let senders = ManualWorkerSenders::new(Some(capture_sender), None, None, None, None);

    assert_eq!(senders.quiesce_extension_inputs(), Ok(()));
    worker.join().map_err(|_| std::io::Error::other("worker"))?;
    Ok(())
}

#[test]
fn mode_fence_detects_work_enqueued_while_pipe_ingress_closes() -> Result<(), std::io::Error> {
    let (capture_sender, capture_receiver) = mpsc::sync_channel(2);
    let (pipe_sender, pipe_receiver) = mpsc::sync_channel(1);
    let ingress_capture = capture_sender.clone();
    let pipe = thread::spawn(move || {
        if let Ok(WorkerCommand::FenceMode { result_sender }) = pipe_receiver.recv() {
            let _ = ingress_capture.send(WorkerCommand::RefreshTrayConnectionStatus);
            let _ = result_sender.send(false);
        }
    });
    let capture = thread::spawn(move || {
        let mut accepted = false;
        let mut fences = 0_u8;
        while let Ok(command) = capture_receiver.recv() {
            match command {
                WorkerCommand::FenceMode { result_sender } => {
                    let _ = result_sender.send(accepted);
                    accepted = false;
                    fences = fences.saturating_add(1);
                    if fences == 2 {
                        break;
                    }
                }
                _ => accepted = true,
            }
        }
    });
    let senders =
        ManualWorkerSenders::new(Some(capture_sender), None, None, None, Some(pipe_sender));

    assert_eq!(senders.quiesce_extension_inputs(), Err(()));
    pipe.join().map_err(|_| std::io::Error::other("pipe"))?;
    capture
        .join()
        .map_err(|_| std::io::Error::other("capture"))?;
    Ok(())
}

#[test]
fn rollback_waits_for_hotkey_projection_restore() -> Result<(), std::io::Error> {
    let (hotkey_sender, hotkey_receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        if let Ok(WorkerCommand::RestoreHotkeys { result_sender }) = hotkey_receiver.recv() {
            let _ = result_sender.send(Ok(()));
        }
    });
    let senders = ManualWorkerSenders::new(None, None, None, Some(hotkey_sender), None);

    assert_eq!(senders.restore_hotkeys(), Ok(()));
    worker.join().map_err(|_| std::io::Error::other("worker"))?;
    Ok(())
}
