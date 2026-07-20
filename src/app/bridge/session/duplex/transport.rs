//! Split transport adapters for duplex resident-pipe sessions.

use std::io::{Read, Write};

use crate::ipc::AgentPipeReadShutdown;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentPipeConnectionContext {
    pub(crate) host_exe_sha256: String,
    pub(crate) parent_fingerprint: String,
    pub(crate) parent_label: String,
    pub(crate) parent_category: &'static str,
}

pub(crate) trait AgentPipeSplit: Read + Write + Sized {
    type Reader: Read + Send;
    type Writer: Write + Send;
    type Shutdown: AgentPipeReadShutdown + Clone + 'static;

    fn connection_context(&self) -> Option<&AgentPipeConnectionContext> {
        None
    }

    fn split_for_session(self) -> std::io::Result<(Self::Reader, Self::Writer, Self::Shutdown)>;
}

#[cfg(windows)]
impl AgentPipeSplit for crate::ipc::WindowsPipe {
    type Reader = crate::ipc::WindowsPipeReader;
    type Writer = crate::ipc::WindowsPipeWriter;
    type Shutdown = crate::ipc::WindowsPipeShutdown;

    fn split_for_session(self) -> std::io::Result<(Self::Reader, Self::Writer, Self::Shutdown)> {
        self.split_with_shutdown()
    }
}

pub(crate) struct SplitSessionPipe<'a, R, W> {
    reader: &'a mut R,
    writer: &'a mut W,
}

impl<'a, R, W> SplitSessionPipe<'a, R, W> {
    pub(crate) fn new(reader: &'a mut R, writer: &'a mut W) -> Self {
        Self { reader, writer }
    }
}

impl<R, W> Read for SplitSessionPipe<'_, R, W>
where
    R: Read,
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buffer)
    }
}

impl<R, W> Write for SplitSessionPipe<'_, R, W>
where
    W: Write,
{
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
